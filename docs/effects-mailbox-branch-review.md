# Branch regression review

Baseline: `origin/main` at `a57673e2`. Reviewed through branch commit
`e5031dc9` plus the current working-tree changes.

All three findings are resolved; each carries its resolution below. Both P1
reproductions were confirmed against this branch before being fixed — the
review was right, and the `any` ownership convention it names was mine.

## Review findings

### P1 — The new `any` ownership convention causes ARC use-after-free and leaks

`coerce_return` now transfers a managed value before erasing it to `i64`, and
registers every `i64 -> pointer` recovery as owned
(`crates/osprey-codegen/src/lower.rs:298-317`). That only balances when the word
was produced by the exact `pointer -> any` path. An `any` parameter is borrowed,
so forwarding it does not create the `+1` that the recovery side assumes.

```osprey
fn identity(x: any) -> any = x
fn make() -> string = identity("a" + "b")
print("v=${make()}")
```

Under `--memory=arc`, `origin/main` prints `v=ab`; this branch exits zero and
prints `v=`. The recovered pointer is entered in the ARC ledger without a
retain. The epilogue then moves that fictitious owner out, releases the real
owner of the argument, and returns a dangling pointer.

The converse also regresses: a heap value that remains erased has no typed
consumer at which to surrender its transferred reference. This is directly
reachable through the new mailbox:

```osprey
effect Boxed { take: fn(any) -> Unit }

fn erased() -> any = "a" + "b"
fn body() -> int !Boxed = {
    perform Boxed.take(erased())
    42
}

let out = handle Boxed
    take x => resume()
in body()
print(toString(out))
```

`origin/main` finishes with zero live ARC objects. This branch finishes with one
live three-byte object (`ab`). `slot_is_managed` classifies an explicit `any`
parameter from its `i64` ABI as scalar
(`crates/osprey-codegen/src/effect_mailbox.rs:23-35,72-89`), so the mailbox can
neither retain nor release the transferred pointer. The same leak occurs when a
heap-bearing `any` is observed only in erased form.

The added tests cover a direct producer followed by a direct typed recovery
(`crates/osprey-codegen/src/lib.rs:492-543` and
`tests/regressions/basics/types/any_type_comprehensive.test.osp:8-17`). They do
not cover forwarding a borrowed `any`, discarding/observing it while still
erased, or carrying it through an effect operand.

**Resolved — the convention is reverted.** Both reproductions were confirmed
first: `v=` with 0 live objects, and `42` with 1 live three-byte object. The
diagnosis is exactly right, and it generalises past the two cases named here —
`fn intish() -> any = 7` recovered as a `string` would have entered `7` in the
ledger and later freed it, so the rule was not merely unbalanced but memory-
unsafe.

No rule at the cast can be sound, because the lowered type of an erased word is
the same `LType::I64` as every `int` and every borrowed `any` parameter, and
that is the one distinction the rule needs. Closing it properly means making
`any` distinguishable from `int` after lowering — the ABI change this review's
own recommendation calls for — not another cast-site patch. Until then the
erasure carries no ownership in either direction, which restores `origin/main`'s
behaviour on both programs above and leaves `origin/main`'s own defect standing:
returning a heap value AS `any` still drops its referent. That defect is
recorded under [TYPE-ANY] in `docs/specs/0004-TypeSystem.md`, with this
repair documented as attempted and wrong so it is not tried a third time.

What replaces the reverted tests is coverage of the case that regressed, which
had none: `forward`/`forwarded` in
`tests/regressions/basics/types/any_type_comprehensive.test.osp` (both flavors)
assert borrowed-`any` forwarding under every backend with the live-object
oracle armed, and
`recovering_a_pointer_from_an_erased_word_takes_no_ownership` in
`crates/osprey-codegen/src/lib.rs` fails if an owner is ever entered in the
ledger at that cast again.

### P2 — The new C coverage inventory check skips a shipped runtime unit

The completeness loop skips every source whose name starts with `test_` before
checking the threshold and exemption inventories (`Makefile:631-635`). That
also skips `compiler/runtime/test_runtime.c`, which is a real member of every
native runtime archive, not just a test harness.

The loophole is reproducible: removing `test_runtime` from `C_COV_EXEMPT` and
running the gate still reports success:

```sh
make C_COV_EXEMPT='web_runtime wasm_builtins_runtime term_runtime' \
  _coverage_check_c_runtime
```

This contradicts the new guarantee at `Makefile:605-616` that every native
runtime unit must be gated or explicitly exempted. A future attempt to gate
`test_runtime.c`, or any new production unit named `test_*`, can therefore
silently leave it outside the ratchet.

**Resolved.** The check no longer decides what ships by pattern-matching a
filename. `C_SHIPPED_UNITS` is derived from the archive object lists themselves
(`FIB_OBJ`, `HTTP_OBJ` and their GC/ARC variants), so membership — the fact the
check actually wanted — is what it tests. The negative control the review gives
now fails where it previously passed:

```text
[c] FAIL: runtime/test_runtime.c ships in a native archive but is neither gated
    in coverage-thresholds.json nor in C_COV_EXEMPT
```

`C_COV_EXEMPT` shrank to `term_runtime test_runtime` with that. `web_runtime`
and `wasm_builtins_runtime` were only ever listed because the old loop walked
`runtime/*.c`; they are absent from every native archive, so the new check does
not ask about them.

### P3 — Effect documentation still reports the fixed mailbox defects as open

The implementation and plan now mark #182 and #185 fixed, but the user-facing
effect documentation still says both are active skips:

- `tests/effects/README.md:111-131` calls #182/#185 current critical defects.
- `tests/effects/resume/README.md:22-35` says position 17 is skipped and warns
  against dynamic string answers until #185 is fixed.
- `docs/plans/README.md:22` still lists #182/#185 as showstoppers.
- Several plan references still locate `__osprey_coro_*` in
  `effects_runtime.c`, although this branch moved it to `effects_coro.c`.

These statements now contradict the passing tests and the updated spec, so a
reader cannot tell which limitations remain real.

**Resolved.** #182 and #185 are struck through and marked fixed in
`tests/effects/README.md`, matching the shape already used for #183, and the
claims that depended on them are gone: `tests/effects/resume/README.md` now
describes a 17-argument operation with every position checked and a string
continuation answer asserted to release under ARC, rather than a skip and a
warning. `docs/plans/README.md` row 0016 leaves #184 as the only remaining
showstopper. The `__osprey_coro_*` references in plans 0016 and 0026 and in the
plans index now point at `effects_coro.c`; `docs/multitarget-js-dotnet.md` lists
it alongside `effects_runtime.c`. Both claims were verified against the suites
before being written down — the 17th position and the managed answer are
assertions in `resume_error_policies.test.{osp,ospml}`, and they pass under all
three backends with the leak oracle armed.

## Verification performed

- `cargo test -p osprey-codegen` — 117 passed.
- `cargo test -p osprey-types` — 242 passed.
- `make _test_c_runtime` — all C runtime suites passed.
- `make _coverage_check_c_runtime` — configured thresholds passed; the P2
  negative-control invocation above also passed when it should have failed.
- `make _runtime_wasm` — passed.
- Paired Default/ML effect, file, error, and `any` regression suites passed
  under ARC.
- Both handler-answer mismatch fixtures produced all three expected
  diagnostics.
- The P1 programs were run on both this branch and a clean `origin/main`
  worktree to establish the behavioral regression.

---

## Soft recommendations

- It may be worth revisiting `any` as an ownership-carrying ABI rather than
  patching more individual casts. A tag/provenance bit, or an explicit
  borrow/forward/consume model for erased words, would let ARC distinguish a
  transferred pointer from a borrowed or scalar `i64`.
- Small ARC goldens for borrowed-`any` forwarding, opaque/discarded heap
  `any`, and `any` effect operands would likely prevent both halves of P1 from
  recurring. Running them with the live-object oracle is important because one
  failure is wrong output and the other is only visible as a leak.
- The C inventory check could narrowly exclude known harness source names, or
  derive production units from the archive object lists, before consulting the
  explicit exemption list.
- Updating the effect READMEs and plan index alongside the mailbox fix would
  keep the documented support boundary aligned with the tests.
