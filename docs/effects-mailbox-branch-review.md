# Branch regression review

Baseline: `a57673e2` (`origin/main`). Scope: the staged resumable-effects/mailbox changes.

All four findings are resolved. Each is recorded below with what the fix was and
what pins it. Three further defects surfaced while reproducing them; they are
listed at the end with their status, because a review that quietly drops what it
found is worth less than one that names it.

## Findings

### P1 — The new answer guard rejects a valid erased `any` value — FIXED

`reject_unrepresentable_answer` treated every `i64` as a scalar and rejected it when the handled answer was a string or pointer. That is not true for `any`: its ABI is an erased machine word, and that word may contain a boxed pointer.

```osprey
effect Mixed {
    a: fn(int) -> int
    b: fn() -> any
}

fn dynamicValue() -> any = "dyn" + "amic"
fn body() -> string !Mixed = perform Mixed.b()

let out = handle Mixed
    a x => resume(x)
    b => dynamicValue()
in body()

print("len=${length(out)}")
```

Prints `len=7` again. The guard is deleted: `Value` carries no field that
separates an erased `any` word from a genuine `int`, so no code-generation-side
shape test can decide this. The rule moved to inference, below.

### P1 — The new mismatch quarantine still permits silent pointer corruption — FIXED

The guard only rejected scalar arm values flowing to plain pointer answers. The inverse direction fell through to `coerce_to`, which boxes a pointer as `i64`, so a non-resuming `string` arm in a resuming region whose body answers `int` returned a heap address as a successful integer. `Result` answers bypassed the guard entirely through the earlier `make_ok`/`repack_to_inner` branches and showed the same corruption inside `Success`.

Both directions and the `Result` answer are now rejected by the checker, which
still has the semantic types. `infer_handler` classified a non-resuming arm by
the value-substitution rule — correct only when NO arm in the region resumes and
the handler is inlined at each `perform`. When some other arm resumes, a
non-resuming arm abandons the continuation: the operation's result is never
produced, and the arm's value becomes the whole `handle` expression's answer. It
is now checked against that, after the handled body has pinned it, so the blame
reads in the right direction:

```text
handler arm `Mixed.b` never resumes, so its value becomes the whole `handle`
expression's result — but it is `string` and that result is `int`. Give the arm
a `resume`, or make every arm of this handler agree with the handled
expression's type
```

Specified as `[EFFECTS-HANDLER-ARMS]` in `docs/specs/0017-AlgebraicEffects.md`.
Pinned by `examples/failscompilation/effect_arm_answer_type_mismatch.ospo` and
its new ML twin — both directions and a `Result` answer, three diagnostics each —
by three unit tests in `crates/osprey-types/src/expr.rs` (including one that the
fully non-resuming region still substitutes values, so
`[EFFECTS-GENERIC-INSTANTIATION]` is not weakened), and by the accepted
counterparts in `tests/regressions/effects/abort_vs_resume.test.osp`.

### P1 — A queued managed perform leaks when the handler aborts the region — FIXED

The generated suspend function retains every managed operand before calling `__osprey_coro_suspend`. A performer queued behind `coro->in_flight` could then observe `coro->abort` and `pthread_exit` before `mailbox_new` took ownership — neither constructing a mailbox nor releasing the references already retained for it.

`__osprey_coro_suspend` now releases the supplied managed slots on that path,
outside the lock, and still exits. Returning normally would be wrong twice over:
its return value IS the operation's result, so an aborted region would hand the
performer a fabricated `0` that a `string` operation would `inttoptr` and
dereference, and the abandoned body would go on running user code after the
`handle` had already produced its answer.

The suspicion about the second `pthread_exit` (after the suspension wait) is
refuted and now says so in a comment: by then the mailbox exists and is retired
either by the dispatcher or by `__osprey_coro_free`, so releasing there would be
a double free.

Pinned by `t_aborted_queued_perform_releases_its_operands` in
`compiler/runtime/effects_runtime_tests.c` — deterministic, with one managed and
one scalar slot so the release path is proved to read the kinds rather than the
count. Verified red before the fix (`osp_arc_live_objects` assertion fails) and
green after. `mailbox_free` and the new path share one `release_operands`, so
the two cannot drift on which words are pointers.

### P2 — Most continuation code is no longer coverage-gated — FIXED

`coverage-thresholds.json` contained only an `effects_runtime` C entry, and the gate matches configured project names to exact `runtime/<name>.c` files, so `effects_coro.c` was instrumented but could not fail a threshold.

`effects_coro` is gated at 88 (measured 91.74 macOS / 90.80 Linux) and
`file_runtime`, which had the same pre-existing gap, at 78 (81.63 / 80.52).
Floors follow the file's own rule: weakest measured platform minus ~2.

The root cause is fixed too, because otherwise the next file split repeats it:
the gate now enumerates the filesystem as well as the JSON, and fails on any
native `runtime/*.c` that is neither gated nor named in the new `C_COV_EXEMPT`
(`web_runtime`, `wasm_builtins_runtime`, `term_runtime`, `test_runtime`, each
with its reason recorded at the variable). Verified by dropping `term_runtime`
from the exemption list and watching the gate go red.

## Also found while reproducing these

Four defects outside the review's scope, two fixed and two open. All of them
predate this branch — `a57673e2` has byte-identical `__osprey_coro_abort`, the
same `toString` lowering and the same erasure handling — so none is a regression
from the mailbox work.

### Heap buffer overflow in `toString` of a `Result` — FIXED

Not from this branch, but it is what the reported `http_state_levels` failure
actually was. `toString` formatted `Success(%s)` / `Error(%s)` into a fixed
64-byte block with `sprintf`. The substituted string is a runtime value of any
length, so this is a heap overflow, not a size heuristic: `toString` of a
`readFile` success wrote the whole file past the end of a 64-byte allocation.
Truthful I/O error messages merely made it fire, at exit 133 under the default
and gc backends and as visibly corrupted output.

Both `%s` templates now measure with `osp_format_size` and fill an exactly-sized
buffer, sharing one `format_sized` with string interpolation — which had already
been fixed this way and whose duplicate copy is now gone. `int_to_string` keeps
a fixed block, because 21 bytes is a property of `i64`, not of a runtime value.
Pinned by `tests/regressions/basics/errors/error_messages.test.osp` and its ML
twin, which render a 130-character payload and assert the exact byte count.

### Abandoning a region leaks the killed frames' heap operands — OPEN

`pthread_exit` runs no epilogue, so every heap value the killed stack's frames
owned is abandoned — including the performer's own reference to an operand,
which is a separate scope from the mailbox's. One live object at exit under
`--memory=arc`:

```osprey
effect Label { tag: fn(string) -> string }

fn ask(subject) !Label = perform Label.tag(subject)

let answer = handle Label
    tag subject => match subject == "alpha" {
        true  => "stopped at ${subject}"
        false => resume("saw ${subject}")
    }
in ask("al" + "pha")
```

Not reachable with scalar operands, which is why `abort_vs_resume` passes the
leak oracle today. Reclaiming them needs generated cleanup along the abort path
— unwinding — not a release the runtime could issue, because the owning slots
are `alloca`s in every frame on the killed stack. Recorded in
`docs/specs/0017-AlgebraicEffects.md` with this reproduction.

### Abandoning a region whose body awaits a spawned fiber deadlocks — OPEN

`__osprey_coro_abort` joins the body thread, and a body blocked in `await` of a
fiber the same abort has just killed inside its own `perform` never returns. The
process hangs forever. Resolving it means deciding what `await` of an abandoned
fiber yields — the cancellation question in
[issue #177](https://github.com/Nimblesite/osprey/issues/177). Recorded in the
same spec section. No test ships for it: a test that hangs never reports, which
is worse than one that fails.

## Notes

- `let n: int = erase("dynamic")` printing a pointer-derived integer is
  `[TYPE-ANY]` as specified — `any` carries no runtime tag and "code that
  consumes its representation must already know what was passed". It is a sharp
  edge, not an undocumented hole, and `check_abandoning_arm` accepting an `any`
  arm is consistent with it.
- `fn intish() -> any = 7` recovered as a `string` dies with SIGSEGV. `any`
  unifies with everything, so nothing rejects recovering a pointer from a word
  that was never one. Pre-existing, and the concrete cost of "code that consumes
  its representation must already know what was passed"; narrowing it so an
  un-erasure requires a `match` is its own piece of work. Recorded under
  `[TYPE-ANY]`.

### ARC use-after-free on every heap value erased into `any` — FIXED

Also pre-existing, also not from this branch, and reachable with no effects at
all: `fn erased() -> any = "era" + "sed"` read back as a string was EMPTY under
`--memory=arc`. The deleted codegen guard had merely kept the same shape from
being reachable through a handler.

The ledger matches owners by register, so once a pointer is `ptrtoint`-boxed the
epilogue no longer recognises it, released the referent, and handed the caller a
dangling word — no crash, no diagnostic, and nothing for the leak oracle to see,
because a premature free leaks nothing. Ownership now crosses the erasure in
both directions: the producing boundary TRANSFERS its `+1` while the pointer is
still visible (`arc::transfer_out`, factored out of `epilogue` rather than
duplicated), and the recovering boundary OWNS what the word carries. One half
alone is not a fix — the transfer without the hand-over turns the free into a
leak, which is what I first concluded and was wrong about.

Deliberately not placed inside `cast::coerce_to`: its `inttoptr` arm also serves
borrowed reads under the uniform collection element ABI, and owning those would
over-release. The rule stays at the two boundaries that actually transfer.

Pinned by `tests/regressions/basics/types/any_type_comprehensive.test.osp` — the
file whose every `any` had carried a scalar, which is exactly why an erased heap
value went uncovered — plus two codegen unit tests for the mechanism, and the
`any` arm of `abort_vs_resume`, which now builds its string at runtime instead
of using a literal that would have passed either way.
- `file_runtime.c` did not compile for `wasm32-wasip1` (`strerror_r` is absent
  from wasi-libc) and the failure was silent: the archive rule's `for` loop is a
  non-final member of an `&&` list, where `set -e` does not apply, and the
  archive then globbed whatever objects existed. Both are fixed — the loop exits
  on failure and the archive names its units explicitly rather than globbing.

---

## Soft recommendations

- It may be worth treating abandoned continuation frames as the next ownership priority: the remaining ARC leak needs generated cleanup or unwinding rather than another mailbox-local release.
- The await/abort deadlock probably deserves an explicit cancellation semantic before implementation. A subprocess test with a hard timeout could pin the current failure without hanging CI indefinitely.
- Keeping semantic handler-answer checks in inference should avoid repeating the erased-`any` false positive that the temporary codegen guard introduced.
- The new coverage inventory check should help prevent future runtime file splits from silently escaping the ratchet; retaining that check alongside per-file thresholds seems worthwhile.
