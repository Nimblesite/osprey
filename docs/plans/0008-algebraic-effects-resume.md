# Plan 0008 — Effect `resume` / Continuations

**Subsystem:** `crates/osprey-syntax`, `crates/osprey-ast`, `crates/osprey-types`,
`crates/osprey-codegen`, `compiler/runtime`
**Status:** Single-shot deep `resume` landed — thread-as-continuation (Option B).
**Two open items** (multi-shot rejection, `make ci` note) are now tracked by
the umbrella effects roadmap [plan 0016](0016-algebraic-effects-and-handlers.md)
§Phase A. This plan documents the *single-shot resume* work specifically;
0016 covers the path to a complete effect system (multi-shot rejection,
first-class handler values, effect-row polymorphism, wasm effects).
**Spec:** [0017-AlgebraicEffects.md](../specs/0017-AlgebraicEffects.md)

## Summary

Effects are real and compile-time safe: declarations, `perform`, `handle … in`,
effect annotations, and unhandled-effect rejection all work. A handler arm's
value now becomes the `perform`'s result and the performer continues past the
`perform` site — the common **single-shot tail-resume** — and handlers may own
mutable state (`[EFFECTS-HANDLER-STATE]`, see below), so the `State` effect is
fully usable today. Explicit single-shot deep `resume` now captures the
continuation as a value so an arm can run code *after* resuming or resume in a
non-tail position. Multi-shot resume remains a follow-up.

## Update — handler-owned state landed

`[EFFECTS-HANDLER-STATE]` Handler arms can read and write a `mut` captured from
the enclosing scope; such a `mut` is promoted to a shared heap cell and the
handler stack carries a per-region environment (`__osprey_handler_push` gained an
`env` pointer; `__osprey_handler_lookup_env` resolves it). This delivers the
canonical State-effect pattern (handler owns the cell, effectful code stays
pure) without general continuations, because value-substitution *is* tail-resume.
Implemented in [crates/osprey-codegen/src/effects.rs](../../crates/osprey-codegen/src/effects.rs)
(`capture_list`/`build_env`/`reload_env`), the cell read/write in
[expr.rs](../../crates/osprey-codegen/src/expr.rs) and
[lower.rs](../../crates/osprey-codegen/src/lower.rs), and
[compiler/runtime/effects_runtime.c](../../compiler/runtime/effects_runtime.c).
Reference app: `examples/tested/effects/http_state_levels.osp`.

## Update — single-shot explicit resume landed

`[EFFECTS-RESUME]` Resuming handler regions now use the thread-as-continuation
runtime in [compiler/runtime/effects_runtime.c](../../compiler/runtime/effects_runtime.c):
the body runs on a pthread with an inherited handler stack, `perform` suspends
through generated trampolines, and `resume(v)` resumes the body until completion
or the next performed operation. Codegen keeps non-resuming handlers on the
existing direct-call path and emits the coroutine path only for arms that mention
`resume`. Covered by the CLI regression test
`explicit_resume_runs_the_performer_continuation`.

## Evidence

- Spec status: explicit single-shot deep `resume` is executable —
  [0017-AlgebraicEffects.md](../specs/0017-AlgebraicEffects.md) §Status.
- Codegen gate: resuming regions emit body thunks, suspend trampolines, and a
  host-side drive/dispatch function; non-resuming regions keep the ordinary
  handler-function path — [crates/osprey-codegen/src/effects.rs](../../crates/osprey-codegen/src/effects.rs).

## What works today

- Effect declarations, `perform X.op(args)`, `handle X arm… in body`.
- Dynamic handler stack with push/pop/lookup, snapshot/restore across fiber
  boundaries — [compiler/runtime/effects_runtime.c](../../compiler/runtime/effects_runtime.c),
  [crates/osprey-codegen/src/effects.rs](../../crates/osprey-codegen/src/effects.rs).
- Compile-time unhandled-effect checking —
  [crates/osprey-types/src/check.rs](../../crates/osprey-types/src/check.rs).
- Single-shot deep explicit `resume` in handler arms —
  [crates/osprey-codegen/src/effects.rs](../../crates/osprey-codegen/src/effects.rs),
  [compiler/runtime/effects_runtime.c](../../compiler/runtime/effects_runtime.c).
- Working example —
  [examples/tested/effects/algebraic_effects_comprehensive.osp](../../examples/tested/effects/algebraic_effects_comprehensive.osp).

## Where it stops

Multi-shot resume (resuming the same continuation more than once) remains a
follow-up. The landed implementation is single-shot and stackful; a live pthread
stack is not cloned.

## Chosen design — thread-as-continuation (Option B)

The runtime is already thread-based (fibers are pthreads,
[fiber_runtime.c](../../compiler/runtime/fiber_runtime.c)) and already
snapshots/restores the handler stack across threads
([effects_runtime.c](../../compiler/runtime/effects_runtime.c)). There is no
`ucontext`/`setjmp` in the tree, so a suspended **thread** is the continuation —
no stack-segment copying, no CPS pass. This also makes single-shot fall out for
free: a live pthread stack cannot be cloned, so multi-shot is naturally excluded
(and rejected with a diagnostic).

### Static gate

A `handle E arm… in body` is a **resuming region** iff any arm body contains a
`resume`. Resuming regions emit the coroutine path below; every other region
keeps the existing zero-overhead function-call path (handler = function, `ret` =
tail-resume). Detected by an AST walk over arm bodies in codegen.

### Runtime ABI (`compiler/runtime/effects_runtime.c`)

A per-region `Coro` control block carries: the captured user `env`, a turn flag +
mutex/cond pair, an operation-id + argument buffer (body→host), a `resume_value`
(host→body), the body's final result, and `done`/`abort` flags.

- `Coro *__osprey_coro_new(void *env)` — allocate.
- `void __osprey_coro_start(Coro*, i64 (*body)(void*), void *env, HandlerSnapshot*)` — spawn
  the body thread with the inherited handler stack and body environment, then
  block the host until the body first suspends or completes.
- `i64 __osprey_coro_suspend(Coro*, i64 op_id, i64 *args, i64 argc)` — **body side**, called
  by each arm's suspend-trampoline at a `perform`: publishes `(op_id,args)`, hands
  control to the host, blocks until resumed, returns `resume_value`. If the host
  set `abort`, it `pthread_exit`s (single-shot teardown).
- `i64 __osprey_coro_resume(Coro*, i64 v)` — **host side**, lowering of `resume`:
  delivers `v`, runs the body until its next suspend or completion, blocks the
  host meanwhile; returns `done ? body_result : <sentinel: body performed again>`.
- accessors `__osprey_coro_done/op/arg/result` and `__osprey_coro_abort` + free.

### Dispatch model (codegen-emitted, host thread)

The host side is ordinary host-thread call recursion; the body thread runs
straight-line and only suspends:

```
region(env):
  coro = coro_new(env); push suspend-trampolines(env=coro); snapshot = handler_snapshot()
  coro_start(coro, body_thunk, env, snapshot)
  return drive(coro)

drive(coro):                 // also re-entered after each resume that performed again
  if coro_done(coro): return coro_result(coro)
  return dispatch_arm(coro, coro_op(coro))     // an arm finishing IS the region answer

dispatch_arm(coro, op):      // switch op → __arm_E_op(env, args…); arm body may call resume
resume(v)  ⇒  r = coro_resume(coro, v); if !coro_done(coro) then drive(coro) else r
```

Tail-resume arms bottom out when the body completes (answer = `body_result`),
matching today's semantics; non-tail arms run their post-`resume` code as the
recursion unwinds (LIFO), the behaviour value-substitution can't express. An arm
that never resumes returns directly → host sets `abort`, joins the body, frees the
`Coro`.

### Phases

1. **Surface syntax + AST.** Done: `resume "(" expr? ")"` in the grammar, AST
   `Expr::Resume(Option<Box<Expr>>)`, syntax lowering.
2. **Types.** Done: bind handler-arm params from the effect's `OpType`; type `resume`'s
   argument against the operation result; reject `resume` outside a handler arm.
3. **Runtime.** Done: the `Coro` ABI above in `effects_runtime.c`.
4. **Codegen.** Done: static gate; suspend-trampolines; `body_thunk`; the host
   `drive`/`dispatch_arm`; lower `resume`.
5. **Multi-shot** follow-up.

## Testing

- CLI regression: `explicit_resume_runs_the_performer_continuation` asserts
  LIFO post-`resume` output for the Audit/pipeline example.
- `failscompilation` case: multi-shot resume rejected with a clear message (until
  implemented).

## Risks / considerations

- Highest-risk item here: continuations interact with memory management
  (currently none — [0018](../specs/0018-MemoryManagement.md)); a captured stack
  segment that is never freed compounds existing leaks. Note the dependency.
- Interaction with fibers: the handler snapshot/restore path must compose with
  captured continuations.
- Land single-shot first; keep the existing value-substitution behaviour working
  for handlers that never resume.

## TODO

- [x] Add `resume <expr>` to grammar, AST, and syntax lowering.
- [x] Bind handler-arm params from the effect `OpType`; type `resume`.
- [x] Prototype single-shot delimited continuations (stackful capture on the
      existing handler stack).
- [x] Implement the `__osprey_coro_*` continuation ABI in `effects_runtime.c`.
- [x] Codegen handler arms to capture the continuation + emit resume.
- [ ] Reject multi-shot resume with a clear diagnostic — **moved to
      [plan 0016](0016-algebraic-effects-and-handlers.md) §Phase A** (today a
      double-`resume` silently no-ops the second call and returns a wrong
      answer with exit 0; must become a loud rejection).
- [x] Add a resuming-handler CLI regression test.
- [x] Update 0017 §Status once single-shot resume lands.
- [x] `make ci` green.
