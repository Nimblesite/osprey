# Handler-values branch review

Reviewed on 2026-09-20: `codex/handler-values` at `0a92010a7ad7efcebff23d1055b37c4711f80c98`, against `main` / `origin/main` at `152be13fc0a9ec2e511d2f75771d61ebe0a9a346`.

**Verdict: CI is green and I found no weakening of CI gates or existing corpus assertions. There is one confirmed defect in the new static/dynamic composition support. Several older compiler limitations also survive behind the new handler syntax.** Fix the composition defect before presenting static and runtime handlers as interchangeable. This is a working breaking prototype, not completion of the effects specification.

## What changed in the language

- **Handlers become ordinary callable values:** `let h = handler E { ... }`, then `h(work)`. A block statement `handle E { ... }` handles everything following it in that block. Old `handle ... in/do ...` forms and file-scope installation are rejected. Existing applications need migration.
- **Declarations determine control flow.** A plain operation returns its replacement value and the computation continues. A `control` operation permits `resume`; returning without resuming abandons the computation. An unreachable `resume` no longer changes an arm's meaning. This fixes a real semantic inconsistency on `main`.
- **Composition and answers change:** arms run outside their own activation, so same-operation requests forward outward. Deep resumption reinstalls the suspended scopes. A `return` clause transforms normal completion and the answer returned by `resume`; an abandoning control arm bypasses that transformation.
- **Static interpretation is broader and more thoroughly checked.** Ordinary value effects can be selected statically. Arm types, captures and runtime requirements are checked before erasure. Explicit generic instantiations work for dynamic effects too.
- **Runtime lookup changes:** operation names become compiler-assigned integer IDs with an indexed active-handler table. This removes string searching from lookup; it does not remove continuation allocation or make every effect operation constant-cost. Native continuations still use pthreads.
- **Documentation is consolidated:** the effects spec and plan 0016 own the contract and implementation status. Across tracked specs and plans, text shrank by 2,274 lines. Open effect rows in function types, named instances, masking, owned/stored continuations, multi-shot resumption and finalization remain unfinished. Wasm still rejects dynamic control operations.

## New defect

### P2 — An inner partial runtime handler hides unrelated outer static arms

**Location:** [lower_static.rs](../crates/osprey-ast/src/lower_static.rs), lines 138–151, particularly the effect-wide filter at line 148.

```osprey
effect Pair { first: fn() -> int second: fn() -> int }
let result = {
    handle static Pair { first => 1 second => 40 }
    handle Pair { first => 2 }
    (perform Pair.first() + perform Pair.second()) ?: 0
}
print(result)
```

Both flavors reject this with `unhandled effect operations at program entry: Pair.second`. Changing only the outer `handle static` to `handle` runs and prints `42`.

The rewrite removes the entire outer effect from consideration when entering a dynamic handler, even when that handler supplies only `first`. It must shadow the supplied operations while retaining outer implementations of uncovered operations. The [scope contract](specs/0017-AlgebraicEffects.md#scope-composition-and-instances) explicitly permits complementary partial handlers. Consequently, changing a provider's stage can break valid application composition.

This combination is newly supported on the branch; `main` rejected static interpretation of an ordinary dynamic effect. It is a defect in the new capability, rather than an unchanged `main` program newly failing. Existing static-selection tests cover complete handlers and separate static/dynamic calls, but miss this mixed partial nesting.

## Older defects that still affect the new interface

These were checked against an independently built source snapshot of `main`. They must not be described as newly introduced regressions.

### P1 — Abandonment leaks owned values from the suspended computation

```osprey
effect Stop { control stop: fn() -> int }
fn work(n) = {
    let text = "allocated-${n}"
    let result = perform Stop.stop()
    print(text)
    result
}
let h = handler Stop { stop => 0 }
print(h(|| => work(42)))
```

Run with `OSPREY_ARC_DEBUG=1 target/release/osprey file.osp --run --memory=arc`. Output is correctly `0`, but the process exits successfully with **one live ARC object, 13 bytes**. The suspended frame exits through `pthread_exit` in [effects_coro.c](../compiler/runtime/effects_coro.c), lines 254–261, bypassing generated cleanup for `text`. The new thread cleanup releases handler-stack bookkeeping, not all source-owned values.

On `main`, the equivalent old-form handler with `stop => match false { true => resume(0) false => 0 }` produces the same leak. The branch makes abandonment directly expressible without that unreachable branch. This confirms the resource-audit concern already recorded in plan 0016; it is an observable defect, not merely missing finalizer syntax. Add an abandonment test with a live allocation before the perform, retaining the zero-live-objects assertion.

### P2 — Inferred stateful factories pass checking but cannot be compiled

```osprey
effect Count { next: fn() -> int }
fn counter(initial) = {
    mut count = initial
    handler Count {
        next => {
            count = (count + 1) ?: count
            count
        }
    }
}
let a = counter(0)
print(a(|| => perform Count.next()))
```

Both flavors pass `--check` but `--run` fails with `a closure value with a still-generic type`. Giving `counter` the return type `fn(fn() -> int) -> int` makes it run. The existing stateful factory test in [handler_values.rs](../crates/osprey-cli/tests/handler_values.rs), line 68, supplies that annotation. It proves lifetime/state behavior for the annotated case, not unrestricted inferred factories. The equivalent lambda wrapping a handler on `main` has the same code-generation failure.

### P2 — Higher-order effect results do not reliably survive handler abstraction

```osprey
effect Read { read: fn() -> fn(int) -> int }
let h = handler Read { read => |n| => (n + 1) ?: 0 }
let action = h(|| => perform Read.read())
print(action(41))
```

Both flavors reject this as a dynamic callable whose effect provenance cannot be proven. A direct block-scoped handler around `perform Read.read()` works and prints `42`; selecting the callable handler statically does not repair it. The effect analysis loses the returned function's provenance through the handled callback, so extracting a working block into a reusable handler can stop compilation.

The equivalent named installer `fn supply(action) = handle Read ... in action()` also fails on `main`. This is an inherited higher-order analysis gap exposed by the new callable interface, distinct from the passing tests for return clauses that construct a closure themselves.

### Other inherited validation gaps

- A generic installer with `Echo<T>.echo: T -> T` and an identity arm cannot serve both integer and string callbacks: both revisions report their operations unhandled. Likewise, `handle static E<int>` does not discharge an inferred `perform E.echo(42)`, although explicitly writing `perform E<int>.echo(42)` works. Static matching still compares source spellings in [lower_static.rs](../crates/osprey-ast/src/lower_static.rs), lines 436–447. The spec's generic inference and reuse promises exceed these cases.
- Duplicate operation arms are accepted: `handler Read { read => 1 read => 2 }` checks successfully and returns `2`. `main` does the same with its old syntax. The checker validates each arm but does not reject duplicate names, contrary to `[EFFECTS-OP-TYPING]`.

## Test and gate audit

**No material weakening found in the reviewed diff.** In particular:

- `.github/`, `Makefile`, `coverage-thresholds.json`, the corpus runner, scripts and target rejection manifests are unchanged. No required check, coverage floor or test timeout was relaxed.
- The only changed existing successful-program golden is `handler_scoping.test.osp.expectedoutput`: it retains the previous output and adds a third assertion plus the updated TAP tally. Other existing success goldens are unchanged. Syntax migration can change the scope of a handler, but these output and assertion oracles remain active.
- Four removed `contains_resume` unit tests exercised the deleted mode-inference helper. Replacement runtime tests assert declaration-based value/control behavior, including identical abandonment with and without an unreachable resume. This is a deliberate contract replacement, not hiding a failure.
- Deleted rejection fixtures covered formerly unsupported callable handlers, explicit dynamic instantiation and static selection of ordinary effects. Positive coverage replaces those restrictions. Self-forwarding rejection tests were changed consistently: an outer handler permits forwarding; no outer handler still produces an unhandled-operation error.
- Editor tests retain their assertions; migrated source positions and spelling changed. The formatter's old `in`-repair test became a block-handler formatting test. Coverage's nested-expression test increased its expected instrumented lines from seven to nine.
- The omissions exposed above are **coverage gaps**, not evidence that old assertions were deleted. State-factory annotations and simple integer abandonment examples limit what the new green tests establish.

## Verification and limits

- Built the current release CLI; `cargo test --workspace` passed locally: 1,800 tests, zero failures or ignored tests.
- Built `main` separately from `git archive`, with its own runtime archives. The shared checkout and branch were unchanged; no worktree or branch was created.
- Compared all **177 unchanged tracked `.osp`/`.ospml` files under `tests/` and `examples/`** using each compiler's `--llvm`: 138 succeeded on both, 39 failed on both, with no changed exit outcome or timeout. These were standalone invocations; project files needing assembly can reject individually. This compares acceptance/code generation, not byte-for-byte IR or runtime output.
- Executed the `semantics` and `returns` comparisons using both Osprey flavors plus installed Koka, OCaml, Eff and Effekt. Both comparisons passed every language's expected output. They establish the tested basic continuation/answer semantics, not comprehensive parity.
- [PR 239](https://github.com/Nimblesite/osprey/pull/239) remains draft. At the reviewed head, all 14 reported checks succeeded, including default/GC/ARC corpus, wasm, Windows, editor tests, website tests and coverage. Full target suites were verified through CI rather than rerun locally for this review.
- Reproducer sources and local logs are in `/tmp/osprey-branch-review/`. No compiler, runtime, test, golden or CI source was changed during this review.

**Recommended follow-up:** fix operation-level shadowing first; add the exact failing compositions to both flavors. Keep the inherited ARC and higher-order failures visible, and qualify the implementation-status claims until their executable cases pass.
