# Plan 0016 — Algebraic Effects Roadmap: Resume, Handler Values, Multi-Shot

**Subsystem:** `crates/osprey-syntax` (both flavors), `crates/osprey-ast`,
`crates/osprey-types`, `crates/osprey-codegen`, `compiler/runtime`
**Spec:** [0017 — Algebraic Effects](../specs/0017-AlgebraicEffects.md)
([EFFECTS-RESUME], [EFFECTS-HANDLER-STATE], [EFFECTS-GENERIC-*]),
[0023 — Language Flavors](../specs/0023-LanguageFlavors.md)
([FLAVOR-HANDLER-VALUE])
**Status:** Effects are usable and compile-time safe today for the common
cases; this plan is the roadmap from there to a **complete** effect system.
It supersedes the narrower [plan 0008](0008-algebraic-effects-resume.md)
(single-shot resume, which landed) as the umbrella tracker, and absorbs the
handler-value work sketched in [plan 0013](0013-ml-flavor-frontend.md) Phase 0
and the effect-row-polymorphism gap flagged in
[plan 0015](0015-generics-and-variance.md).

## Summary

Osprey's effects work: `effect` declarations, `perform`, `handle … in`
(Default) / `handle … in`/`… do` (ML), effect annotations, compile-time
unhandled-effect rejection, handler-owned `mut` state, **generic effects**
(`effect State<T>` with per-site instantiation), and **single-shot deep
`resume`** (thread-as-continuation), and **multi-shot rejection** (a second
resume on a consumed continuation now aborts loudly — Phase A, done). What is
NOT complete: a **multi-shot-capable runtime**, **first-class handler values**
(`handler E { … }` and multi-install `handle a b do body`), **static effect
safety across the handler/row instantiation seam** (currently a runtime
abort, not a compile error), and **effects on the wasm target** (the
continuation runtime is native-only). This plan sequences those to done.

## What works today (file:line evidence)

- Declarations, `perform X.op(args)`, `handle X arm… in body`, effect
  annotations, unhandled-effect checking —
  `crates/osprey-types/src/check.rs`, `crates/osprey-codegen/src/effects.rs`,
  `compiler/runtime/effects_runtime.c`.
- **Tail-resume** by value substitution: a non-resuming arm's value becomes
  the `perform`'s result — the cheap default; handlers may own `mut` state
  ([EFFECTS-HANDLER-STATE], `capture_list`/`build_env`/`reload_env` in
  `effects.rs`). Reference: `examples/tested/effects/http_state_levels.osp`.
- **Single-shot deep `resume`**: an arm that mentions `resume` runs the body
  on a pthread (`__osprey_coro_*`, `effects_runtime.c`), suspends at each
  `perform`, and `resume(v)` drives it to completion or the next operation.
  Reference: `examples/tested/effects/resume_value_rewrite.osp` and the
  `resume_*` family (LIFO audit, early-exit abort, outer-handler bridge,
  unit markers).
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

3. **Handler/row instantiation mismatch is a runtime abort, not a type
   error.** A `!Stash<int>` function called under a `handle Stash` whose arms
   pin `Stash<string>` type-checks (the two instantiate independently across
   the `Type::Fun` boundary, which carries no effect row) and aborts at
   runtime with `unhandled effect: Stash$int.take`. Sound, but the effect
   system's promise is *compile-time* safety. (Flagged in plan 0015 §3.)

4. **No effects on wasm.** `__osprey_coro_*` is native-only
   ([WASM-TARGET-EFFECTS]); resuming effects link-fail and are SKIP-classed
   by `diff_wasm_examples.sh`.

## Phasing

### Phase A — Reject multi-shot resume with a clear diagnostic — ✅ (done)

Closed the silent-incorrectness hole. The thread-as-continuation model is
inherently single-shot (a live pthread stack cannot be cloned), so the
near-term behavior is a **loud rejection**, not a wrong answer.

- [x] Detect a second `resume` on the same continuation. `__osprey_coro_resume`
      now aborts with `fatal: continuation already resumed (multi-shot resume
      is not supported)` and a nonzero exit when the coro is already done (the
      continuation was consumed). Runtime-side, always correct — the legitimate
      drive→resume→drive re-entry leaves the coro *suspended*, not done, so it
      never trips the guard. (`compiler/runtime/effects_runtime.c`.)
- [ ] *(Optional, deferred.)* A **compile-time** diagnostic where statically
      obvious — an arm that `resume`s on two always-executed control-flow paths
      — would beat the runtime guard for those cases. Not implemented: the
      runtime guard is sound and total, and the static analysis (distinguishing
      always-both from mutually-exclusive match arms) is a nontrivial follow-up.
- [x] `examples/failscompilation/multishot_resume_rejected.ospo`: a
      double-`resume` arm rejected (nonzero exit) with the clear fatal message;
      single-shot limitation documented in 0017 §Status.
- [x] Flipped plan 0008's open TODO `Reject multi-shot resume with a clear
      diagnostic`.

### Phase B — First-class handler values + multi-install — ⬜ (the big feature)

The [FLAVOR-HANDLER-VALUE] shared-core addition. Flavor-neutral; unblocks the
last ML gap (plan 0013 Phase 0) and the richer Default surface.

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

### Phase C — Static effect safety across the handler/row seam — ⬜ (effect-row polymorphism)

Turns the plan-0015 §3 runtime abort into a compile error. This is
effect-row polymorphism — the largest type-system piece.

- [ ] **Effect rows on function types**: add an effect-row component to
      `Type::Fun` (a set of instantiated `EffectRef`s, plus a row variable
      `!E` for polymorphism), so a call carries the callee's declared row into
      the caller's inference.
- [ ] **Row unification**: at a `handle` site, unify the handler's
      instantiation with the rows of functions invoked in the handled body;
      an instantiation mismatch (`Stash<int>` handler vs `Stash<string>`
      callee row) becomes a type error at the call, with both spans.
- [ ] **Row polymorphism**: infer and generalize row variables so a
      higher-order function (`fn run<E>(f: () -> a !E) -> a !E`) threads its
      callee's effects. Keep principal types (bounded by the same
      HM discipline generics use).
- [ ] **Downgrade the runtime guard**: once the seam is statically proven,
      the instantiation-mangled-key null-guard abort ([EFFECTS-GENERIC-RUNTIME])
      becomes a belt-and-braces backstop rather than the primary safety net;
      keep it, but no correct program should reach it.
- [ ] **Tests**: failscompilation cases for the cross-function instantiation
      mismatch (currently a runtime abort); positive cases for row-polymorphic
      HOFs over effects.

### Phase D — Effects on wasm — ⬜ (target parity)

- [ ] A wasm-viable continuation strategy for resuming handlers: either
      compile resuming handlers via a CPS transform (no native stack switch),
      or adopt the wasm stack-switching proposal when toolchain support lands.
      Tail-resume already works on wasm (no coro); only the resuming path is
      native-only.
- [ ] Un-SKIP the resuming effect examples in `diff_wasm_examples.sh` once the
      path exists; byte-identical output to native.

### Phase E — Ergonomics & polish — ⬜ (nice-to-haves)

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
C (effect-row polymorphism) independent of A/B; closes plan 0015 §3
D (wasm effects)           independent; target parity
E (polish)                 after B (handler values change diagnostics surface)
```

A is done (it removed a silent-correctness footgun for a few lines of runtime
code). B is now the highest-value remaining work: it unlocks a whole class of
programs and finishes the ML flavor. C is the deepest (a type-system feature)
but the least user-visible (the runtime already fails safe).

## Risks

- **Multi-shot is not a "later flag" — it is a different runtime model.**
  Thread-as-continuation cannot be multi-shot; genuine multi-shot needs
  stack copying or a CPS/segmented-stack rewrite. Phase A commits to
  rejecting it cleanly; do not promise multi-shot semantics without that
  rewrite (which is out of scope here).
- **Handler values × the C boundaries.** The HTTP callback and fiber
  snapshot/restore paths assume the current push/pop discipline; a heap
  handler value that outlives a `handle` region must not dangle its captured
  env. Cover with state-isolation tests across those boundaries.
- **Effect-row polymorphism × principal types.** Adding rows to `Type::Fun`
  must not break HM principality; follow a published row-typing discipline
  (e.g. Leijen's scoped labels / Koka's row polymorphism) rather than an
  ad-hoc scheme.
- **Byte-exact backstop.** Every phase must keep all existing effect examples
  byte-identical across both flavors (cross-flavor IR equivalence + shared
  goldens), and the `FC_EXPECTED_ESCAPES` ratchet honest.

## TODO (roll-up)

- [x] **Phase A** — reject multi-shot resume (runtime guard +
      failscompilation + 0017 §Status). *Done.* (Optional static-detection
      refinement deferred; the runtime guard is sound and total.)
- [ ] **Phase B** — first-class handler values + multi-install (AST, types,
      state, codegen, both surfaces, tests). *Unblocks plan 0013 Phase 0.*
- [ ] **Phase C** — effect-row polymorphism on `Type::Fun`; static seam
      safety. *Closes plan 0015 §3.*
- [ ] **Phase D** — resuming effects on wasm (CPS or stack-switching).
- [ ] **Phase E** — diagnostics, LSP effect completion, optional handler
      return clauses.

## References

- Plotkin, Pretnar. *Handling Algebraic Effects.* LMCS 2013.
- Leijen. *Type Directed Compilation of Row-Typed Algebraic Effects.* POPL 2017.
- Leijen. *Extensible Records with Scoped Labels.* TFP 2005 (row typing).
- Kiselyov, Sivaramakrishnan. *Eff Directly in OCaml.* ML Workshop 2016
  (one-shot continuations via threads — the model in use today).
