# Plan 0007 — `select` Over Channels

**Subsystem:** syntax, AST, types, codegen, native runtime  
**Status:** reserved syntax; typed compilation rejects it  
**Spec boundary:** [CONCURRENCY-SELECT-REJECT](../specs/0011-LightweightFibersAndConcurrency.md#reserved-select-syntax-concurrency-select-reject)

## Current boundary

- Both parsers lower `select` to `Expr::Select`; each arm currently contains
  only a pattern and body, not a channel operation.
- The type checker reports `` `select` is not supported ``. The CLI therefore
  stops before code generation.
- The native runtime has no primitive that waits on multiple channel
  operations.
- `codegen::fiber::gen_select` is a legacy direct-AST path that evaluates the
  first arm. It is unreachable through typed compilation and must be deleted
  when its old codegen-only test can be replaced.

## Required implementation

1. Specify an arm model that retains each `send` or `recv` operation and any
   receive binding through syntax lowering.
2. Add a native runtime primitive for waiting on multiple buffered channel
   operations, including explicit default or timeout behavior.
3. Type-check each operation against its channel element type and merge arm
   result types; then remove `[CONCURRENCY-SELECT-REJECT]`.
4. Lower the runtime's selected arm index to control flow and delete the legacy
   first-arm function.

## Acceptance

- Default and ML programs select between at least two channel operations.
- Receive bindings have the channel element type.
- Default or timeout behavior is tested explicitly.
- Deterministic mode defines and tests tie-breaking.
- Direct AST compilation cannot silently choose the first arm.
