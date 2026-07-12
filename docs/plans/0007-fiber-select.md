# Plan 0007 — `select` Over Channels

**Subsystem:** `crates/osprey-codegen` + `compiler/runtime` (with parser/types done)
**Status:** Partially implemented (front end done; codegen is a placeholder)
**Spec:** [0011-LightweightFibersAndConcurrency.md](../specs/0011-LightweightFibersAndConcurrency.md) §select

## Summary

`select { … }` parses and type-checks, but codegen unconditionally evaluates the
**first arm** — a deterministic placeholder. There is no runtime multiplexing
over multiple channel operations and no timeout arm. So `select` compiles but does
not actually select.

## What works today

- Parser recognizes `select_expression` —
  [crates/osprey-syntax/src/expr.rs](../../crates/osprey-syntax/src/expr.rs).
- Type inference over the arms.

## Where it bails

```rust
// crates/osprey-codegen/src/fiber.rs:97 (gen_select)
/// `select { … }` — take the first arm (the example's deterministic choice).
pub(crate) fn gen_select(cg: &mut Codegen, arms: &[MatchArm]) -> Result<Value> {
    match arms.first() {
        Some(arm) => gen_expr(cg, &arm.body),
        None => Ok(Value::unit()),
    }
}
```

The spec form is channel-operation-driven — e.g. `msg => recv(ch1) => …` — with an
optional timeout/`_` arm; the runtime has no `select` over multiple channels —
[0011 §select](../specs/0011-LightweightFibersAndConcurrency.md).

## Implementation plan

1. **Parse the arm shape fully.** Ensure each arm captures its *channel
   operation* (a `recv(ch)` or `send(ch, v)`) and binding, not just a body. Extend
   the AST/lowering input if the operation is currently dropped.
2. **Add a runtime multiplex primitive.** Implement `channel_select` in
   [compiler/runtime/fiber_runtime.c](../../compiler/runtime/fiber_runtime.c):
   given a set of channel ops, return the index of the first ready one (and the
   received value for `recv`). Build it on the existing channel mutex/condvar
   rather than OS `select(2)` — the channels are in-process.
3. **Support a timeout / default arm** (`_`) so `select` can be non-blocking or
   time-bounded.
4. **Codegen the dispatch.** Replace `gen_select` to: evaluate the channel handles,
   call `channel_select`, then branch to the matching arm body with its binding in
   scope. This relies on cooperative blocking (`fiber_yield`, already in
   `compiler/runtime/fiber_runtime.c` — that was plan 0006, since completed and
   retired) so a blocked `select` yields rather than spins.
5. **Preserve deterministic mode** for the differential harness (deterministic
   tie-breaking when multiple ops are ready).

## Testing

- Add a `tested/fiber` example: two channels, a producer on each, a `select` that
  consumes whichever is ready, plus a timeout arm; assert deterministic output
  under deterministic mode; add `.expectedoutput`.

## Risks / considerations

- Fairness vs. determinism: pick a deterministic tie-break for tests while keeping
  reasonable fairness in normal mode.
- The cooperative-blocking primitive a correct blocking `select` needs
  (`fiber_yield`) already exists — that was plan 0006, since completed and
  retired.

## TODO

- [ ] Ensure each `select` arm carries its channel op + binding through to codegen.
- [ ] Implement `channel_select` (first-ready over N ops) in the C runtime.
- [ ] Support a timeout / `_` default arm.
- [ ] Replace `gen_select` placeholder with real dispatch to the ready arm.
- [ ] Deterministic tie-breaking under deterministic mode.
- [ ] Add a `tested/fiber` multi-channel select example; add `.expectedoutput`.
- [ ] `make ci` green.
