# Plan 0016 — Algebraic Effects Roadmap: Resume, Handler Values, Multi-Shot

**Subsystem:** `crates/osprey-syntax` (both flavors), `crates/osprey-ast`,
`crates/osprey-types`, `crates/osprey-codegen`, `compiler/runtime`
**Spec:** [0017 — Algebraic Effects](../specs/0017-AlgebraicEffects.md)
([EFFECTS-RESUME], [EFFECTS-HANDLER-STATE], [EFFECTS-GENERIC-*]),
[0023 — Language Flavors](../specs/0023-LanguageFlavors.md)
([FLAVOR-HANDLER-VALUE])
**Status:** Shipped: effect declarations, `perform`, lexical handlers,
handler-owned mutable state, generic effects, single-shot deep `resume`, and
runtime rejection of a second resume. Static operation inference, propagation,
handler discharge, entry-point enforcement, generic-instantiation matching and
recursive handler-arm rejection are also shipped. Missing: first-class handler
values; resuming effects on wasm; a multi-shot-capable runtime; and a
first-class open-row representation in Hindley–Milner function types. This plan
supersedes retired plan 0008 and absorbs the handler-value work sketched in
[plan 0013](0013-ml-flavor-frontend.md) Phase 0
and the effect-row-polymorphism gap flagged in
[plan 0015](0015-generics-and-variance.md). Open critical correctness defects:
resumable argument transport after position 16
([#182](https://github.com/Nimblesite/osprey/issues/182)), direct-handler
`Result` transport ([#183](https://github.com/Nimblesite/osprey/issues/183)),
and effect loss through one curried ML lowering path
([#184](https://github.com/Nimblesite/osprey/issues/184)), plus managed string
continuation answers leaking under ARC
([#185](https://github.com/Nimblesite/osprey/issues/185)).

## Summary

Osprey supports `effect` declarations, `perform`, and `handle … in` in both
Default and ML syntax, plus effect annotations and handler-owned `mut`
state, **generic effects**
(`effect State<T>` with per-site instantiation), and **single-shot deep
`resume`** (thread-as-continuation), and **multi-shot rejection** (a second
resume on a consumed continuation now aborts — Phase A, done), plus static
effect discharge for inferred and explicitly declared operations (Phase C,
done for the current language surface). Not implemented: a
**multi-shot-capable runtime**, **first-class handler values**
(`handler E { … }` and multi-install `handle a b do body`), generalized open
row variables as part of HM function types, and **resuming effects on the wasm
target** (the continuation runtime is native-only; direct substitution handlers
work on WebAssembly).

## What works today (file:line evidence)

- Declarations, `perform X.op(args)`, `handle X arm… in body`, effect
  annotations, and operation signature checking —
  `crates/osprey-types/src/check.rs`.
- **Static discharge** — `crates/osprey-types/src/effect_rows.rs` infers
  operation requirements, propagates them to named and higher-order call sites,
  removes only the covered operation at the matching generic instantiation,
  rejects recursive handler-arm re-entry, and requires an empty entry row.
  Explicit rows are checked contracts, not handlers. The runtime null-lookup
  guard in `crates/osprey-codegen/src/effects.rs` remains a defensive backstop.
- **Direct value substitution**: a non-resuming arm's value becomes
  the `perform`'s result; handlers may own `mut` state
  ([EFFECTS-HANDLER-STATE], `capture_list`/`build_env`/`reload_env` in
  `effects.rs`). Reference: `tests/regressions/effects/http_state_levels.test.osp`.
- **Single-shot deep `resume`**: an arm that mentions `resume` runs the body
  on a pthread (`__osprey_coro_*`, `effects_runtime.c`), suspends at each
  `perform`, and `resume(v)` drives it to completion or the next operation.
  Reference: `tests/effects/resume/`, whose paired assertion suites cover value
  rewrite, LIFO audit, early-exit abort, outer-handler bridge, and unit markers.
- **Generic effects**: one declaration, many instantiations, boxed erased
  ABI, instantiation-mangled runtime keys ([EFFECTS-GENERIC-*], plan 0015).

## Where it stops (each with a repro)

1. ~~**Multi-shot resume is silently wrong, not rejected.**~~ **FIXED (Phase A).**
   ```osprey
   effect Choose { pick: fn() -> bool }
   handle Choose
       pick => { let a = resume(true)  let b = resume(false)  a + b }
   in both()
   ```
   The thread-as-continuation runtime cannot resume a consumed pthread. This
   previously made the second `resume` a **no-op** that returned a wrong answer
   with **exit 0** — no diagnostic. It now aborts with `fatal: continuation
   already resumed (multi-shot resume is not supported)` and a nonzero exit
   (runtime guard in `__osprey_coro_resume`). A multi-shot-*capable* runtime
   (stack copying or CPS) is still out of scope — see Risks.

1b. ~~**Concurrent fiber performs into one resuming handler were silently
   wrong.**~~ **FIXED ([EFFECTS-FIBER-PERFORM]).** Two fibers spawned inside a
   handled body, each performing into the same resuming handler, shared one
   op/args/resume_value channel with no ownership: a second perform overwrote
   the first's arguments and both consumed the same resume value —
   nondeterministic wrong answers with exit 0 (audit repro: expected `r=3`,
   observed `r=4` on 4 of 5 runs). Each perform now claims the channel
   exclusively for its full ping-pong (`in_flight` in
   `compiler/runtime/effects_runtime.c` `__osprey_coro_suspend`); queued
   performs are dispatched by the existing drive-loop re-entry. Locked by
   `tests/regressions/effects/fiber_effects.{osp,ospml}` §(3) — deterministic
   `race-free sum 30`.

1c. ~~**`resume` inside a lambda in an arm: checker accepted, codegen
   rejected.**~~ **FIXED.** The checker's `resume_ctx` is now cleared across
   `Expr::Lambda` boundaries (matching codegen): a lambda body runs when
   *called*, not where it is written, so the arm's continuation is not live
   inside it. Now a type error (`` `resume` is only valid inside a handler
   arm ``); pinned by `examples/failscompilation/resume_in_arm_lambda.ospo`.

1d. **Resuming operation arguments after position 16 become zero.** The
   compiler accepts the operation, but the native continuation mailbox copies
   only 16 arguments and `__osprey_coro_arg` returns zero for later positions.
   The process exits successfully with corrupted data. Tracked as critical
   [issue #182](https://github.com/Nimblesite/osprey/issues/182); the paired
   `resume_error_policies.test.{osp,ospml}` suites prove positions 1–16 and keep
   position 17 as an explicit known-failure skip.

1e. **Direct handlers corrupt whole `Result<T, E>` operation values.** Both
   `Success` and `Error` values reach the caller as pointer-like integers that
   vary between runs, with exit 0. Explicit-resume transport is a separate path
   and passes. Tracked as critical
   [issue #183](https://github.com/Nimblesite/osprey/issues/183) by paired
   conditional regressions in `tests/effects/errors/`.

1f. **One curried ML lowering path silently drops performed effects.** An
   unannotated four-argument curried function can run under a handler without
   delivering its operations; the equivalent flat parameter form works.
   Tracked as critical
   [issue #184](https://github.com/Nimblesite/osprey/issues/184). Paired golden
   examples use the verified flat form until the curried path is repaired.

1g. **A dynamic string continuation answer leaks under ARC.** A one-operation
   handler that resumes with a string and returns a computed string produces
   correct output but leaves one managed object live at exit. Repetition and
   both syntax flavors reproduce it. Tracked as critical
   [issue #185](https://github.com/Nimblesite/osprey/issues/185); paired tests
   keep the unsafe shape as a known-failure skip while other string operation
   paths still run under every memory mode.

2. **First-class handler values do not parse.**
   ```osprey-ml
   db = handler Log
       info m => print m
   handle db do body
   ```
   errors `unexpected token Reserved("handler")` — `handler`/`do` are
   reserved-for-future tokens (`ml/token.rs:128`), and the shared-core
   `Expr::HandlerValue`/`Expr::Install` nodes do not exist. `Expr::Handler`
   fuses construction and installation, so a handler cannot be bound,
   returned, passed, or multi-installed.

3. ~~**Handler/row instantiation mismatch is a runtime abort, not a type
   error.**~~ **FIXED (Phase C).** The static discharge pass carries the
   resolved effect arguments with every operation requirement. A
   `Stash<string>` handler therefore cannot remove `Stash<int>.take`, and the
   remaining requirement makes compilation fail at program entry. The
   instantiation-mangled runtime lookup remains a backstop for invalid IR or a
   compiler defect, not the expected rejection mechanism. This is implemented
   without yet adding independently quantified open rows to `Type::Fun`.

4. **No explicit resume on wasm.** `__osprey_coro_*` is native-only
   ([WASM-TARGET-EFFECTS]); resuming effects link-fail and are SKIP-classed
   by `diff_wasm_examples.sh`. Direct value-substitution handlers compile to
   WebAssembly because they do not use the continuation runtime.

## Phasing

### Phase A — Reject multi-shot resume with a clear diagnostic — ✅ (done)

Closed the silent-incorrectness hole. The thread-as-continuation model is
inherently single-shot (a live pthread stack cannot be cloned), so the
second resume aborts with a diagnostic.

- [x] Detect a second `resume` on the same continuation. `__osprey_coro_resume`
      now aborts with `fatal: continuation already resumed (multi-shot resume
      is not supported)` and a nonzero exit when the coro is already done (the
      continuation was consumed). The legitimate drive→resume→drive re-entry
      leaves the coro *suspended*, not done, so it does not trip the guard.
      (`compiler/runtime/effects_runtime.c`.)
- [ ] *(Optional, deferred.)* A **compile-time** diagnostic where statically
      obvious — an arm that `resume`s on two always-executed control-flow paths
      — could report the error before runtime. Not implemented: the
      runtime guard is sound and total, and the static analysis (distinguishing
      always-both from mutually-exclusive match arms) is a nontrivial follow-up.
- [x] `examples/failscompilation/multishot_resume_rejected.ospo`: a
      double-`resume` arm rejected (nonzero exit) with the clear fatal message;
      single-shot limitation documented in 0017 §Status.
- [x] Flipped plan 0008's open TODO `Reject multi-shot resume with a clear
      diagnostic`.

### Phase B — First-class handler values + multi-install — ⬜

The [FLAVOR-HANDLER-VALUE] shared-core addition is flavor-neutral and unblocks
plan 0013 Phase 0.

- [ ] **AST**: add `Expr::HandlerValue { effect, arms }` and
      `Expr::Install { handlers: Vec<Expr>, body }`. Make the existing
      `Expr::Handler { effect, arms, body }` desugar to
      `Install { [HandlerValue { … }], body }` so every current program
      compiles unchanged.
- [ ] **Types**: a `Handler E` type in `osprey-types`; check that a handler
      value covers exactly its effect's operations; type-check `Install`
      handler lists and reject duplicate installed handlers for one effect.
- [ ] **State**: preserve handler-owned `mut` on the handler *value* (the
      cell must survive being bound/returned), extending
      [EFFECTS-HANDLER-STATE] from the fused form to the value form.
- [ ] **Codegen**: a runtime representation for a handler value (its arm fn
      pointers + captured env, as a heap value); lower `Install` of N values
      to N nested `__osprey_handler_push`/`pop`; preserve behaviour across the
      C HTTP-callback and fiber boundaries; keep `resume` working through an
      installed handler value.
- [ ] **Default surface**: `let h = handler E { … }` value form; multi-install
      `handle h1 h2 in { body }`; grammar + regen + lowering.
- [ ] **ML surface**: un-reserve `handler`/`do` (`ml/token.rs`), parse
      `handler E` → `HandlerValue` and `handle a b do body` → `Install`
      (`ml/parser.rs`, `ml/lower.rs`); the ONE remaining ML lowering arm.
- [ ] **Tests**: handler value bound / returned / passed to a HOF; state
      isolation vs sharing across installs; multi-install; **byte-identical**
      to the fused form for every existing effect example (both flavors,
      shared goldens, cross-flavor IR equivalence).

### Phase C — Static effect safety across the handler/row seam — ✅

The concrete safety goal is shipped for the current language surface. The
checker performs a least-fixed-point operation-summary analysis over named
functions and first-class callbacks. Requirements carry effect name, operation
name and resolved generic arguments; handlers remove only exact covered
requirements. This makes the plan-0015 §3 mismatch, direct unhandled performs,
transitive inferred calls, partially handled effects and effectful implicit
`main` bodies compile errors.

- [x] **Inference and propagation**: infer operations from bodies without
      requiring return/effect annotations; propagate through named calls,
      callback parameters and callable returns.
- [x] **Exact discharge**: subtract only handler arms that cover the same
      effect operation and resolved generic instantiation. Complementary nested
      partial handlers compose without hiding uncovered operations.
- [x] **Entry proof**: require the implicit `main` or executable top level to
      have no remaining concrete operation requirements.
- [x] **Checked explicit contracts**: a non-empty `!E` row constrains what a
      function may require, but never acts as a handler at its call site.
- [x] **Recursive re-entry rejection**: reject an arm that performs its own
      active operation at the same resolved instantiation instead of allowing
      runtime recursion/hang; preserve routing to outer handlers for different
      operations or instantiations.
- [x] **Runtime backstop**: retain instantiation-mangled null-lookup guards for
      defense in depth; a correctly checked program must not rely on them.
- [x] **Regression matrix**: checker tests cover inferred/transitive calls,
      higher-order callbacks, escaped lambdas, implicit `main`, partial and
      nested handlers, generic mismatches, explicit contracts and self-handler
      re-entry.

This phase does **not** claim that `Type::Fun` now stores a general row variable.
The shipped analysis is closed-program and operation-summary based. A future
surface feature that exposes independently quantified open rows (for example,
an explicit `!e` variable in a public higher-order signature) still requires a
row representation and HM generalization rules.

### Phase D — Effects on wasm — ⬜ (target parity)

- [ ] A wasm-viable continuation strategy for resuming handlers: either
      compile resuming handlers via a CPS transform (no native stack switch),
      or adopt the wasm stack-switching proposal when toolchain support lands.
      Direct substitution already works on wasm (no coroutine); only the resuming path is
      native-only.
- [ ] Un-SKIP the resuming effect examples in `diff_wasm_examples.sh` once the
      path exists; byte-identical output to native.

### Phase E — Ergonomics and diagnostics — ⬜

- [ ] Better unhandled-effect diagnostics: name the missing effect + operation
      + the nearest enclosing `handle` and what it does handle.
- [ ] Effect-operation completion / signature help in the LSP (ties into plan
      0013 Phase 6).
- [ ] Consider `finally`/return clauses on handlers (run on normal completion
      of the handled body) if a concrete need appears — spec first.

## Dependencies & sequencing

```
A (reject multi-shot)      ✅ DONE — closed the silent-correctness bug
B (handler values)         independent of A; unblocks plan 0013 Phase 0
C (static discharge)        ✅ DONE — closes plan 0015 §3 for current syntax
D (wasm effects)           independent; target parity
E (ergonomics)             after B (handler values change diagnostics surface)
```

A and C are done. B enables handler values and completes plan 0013 Phase 0.
The runtime guard is now defense in depth rather than normal effect checking.

## Risks

- **Multi-shot requires a different runtime model.** Thread-as-continuation
  cannot be multi-shot; genuine multi-shot needs
  stack copying or a CPS/segmented-stack rewrite. Phase A commits to
  rejecting it cleanly; multi-shot semantics require that rewrite, which is out
  of scope here.
- **Handler values × the C boundaries.** The HTTP callback and fiber
  snapshot/restore paths assume the current push/pop discipline; a heap
  handler value that outlives a `handle` region must not dangle its captured
  env. Cover with state-isolation tests across those boundaries.
- **Future open rows × principal types.** If independently quantified effect
  row variables are added to `Type::Fun`, they must not break HM principality.
  Specify the row-unification discipline and lock it with inference tests.
- **Byte-exact backstop.** Every phase must keep all existing effect examples
  byte-identical across both flavors (cross-flavor IR equivalence + shared
  goldens), and the `FC_EXPECTED_ESCAPES` ratchet honest.

## TODO (roll-up)

- [x] **Fiber-perform race** — concurrent performs into one resuming handler
      serialized per-coro (`in_flight`, [EFFECTS-FIBER-PERFORM]); was
      nondeterministic silent wrong answers. Locked in
      `fiber_effects.{osp,ospml}`.
- [x] **Lambda-resume checker/codegen split** — `resume` inside an arm lambda
      is now a type error (`resume_ctx` cleared across `Expr::Lambda`);
      pinned by `resume_in_arm_lambda.ospo`.
- [x] **Phase A** — reject multi-shot resume (runtime guard +
      failscompilation + 0017 §Status). *Done.* (Optional static-detection
      refinement deferred; the runtime guard is sound and total.)
- [ ] **Critical #182** — preserve every accepted resumable operation argument
      or reject arities above a documented limit before code generation.
- [ ] **Critical #183** — preserve complete `Result<T, E>` values through the
      direct handler ABI in both flavors and all memory modes.
- [ ] **Critical #184** — keep effectful curried ML functions behaviorally
      equivalent to their flat parameter form.
- [ ] **Critical #185** — release managed continuation answers exactly once
      under ARC, including nested and repeated string-valued resumptions.
- [ ] **Phase B** — first-class handler values + multi-install (AST, types,
      state, codegen, both surfaces, tests). *Unblocks plan 0013 Phase 0.*
- [x] **Phase C** — static inferred-operation propagation, exact handler
      discharge, entry proof, generic seam rejection and self-handler
      rejection. *Closes plan 0015 §3 for the current language surface.*
- [ ] **Phase D** — resuming effects on wasm (CPS or stack-switching).
- [ ] **Phase E** — diagnostics, LSP effect completion, optional handler
      return clauses.
