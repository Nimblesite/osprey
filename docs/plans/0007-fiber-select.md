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
- `codegen::fiber::gen_select` **used to** evaluate the first arm on the
  direct-AST path — unreachable through typed compilation, but a
  plausible-looking wrong answer for anything that bypassed the checker. It now
  returns the same `` `select` is not supported `` error the checker does, pinned
  by `direct_ast_select_is_rejected_instead_of_choosing_the_first_arm`
  (`osprey-codegen/src/lib.rs`). The function stays as the loud-failure arm and
  is replaced — not deleted — when arm lowering lands.

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

## TODO

- [x] Direct AST compilation cannot silently choose the first arm —
      `codegen::fiber::gen_select` returns the checker's own
      `` `select` is not supported `` error instead of lowering `arms[0].body`.
      Pinned by `direct_ast_select_is_rejected_instead_of_choosing_the_first_arm`
      (`osprey-codegen/src/lib.rs`); the surface-syntax rejection stays pinned by
      `crates/osprey-cli/tests/examples_compile.rs` and the `osprey-types` unit
      test at `expr.rs`.
- [ ] Arm model that retains each `send` / `recv` operation and any receive
      binding through lowering in **both** flavors — today each arm carries only
      a pattern and a body (`Expr::Select { arms: Vec<MatchArm> }`,
      `osprey-ast/src/lib.rs`), so the channel operation is not representable and
      every downstream walker (`freevars`, `effects`, `effect_rows`, `analysis`,
      `rewrite`, `span`) needs the new shape.
- [ ] Native runtime primitive that waits on multiple buffered channel
      operations, with explicit default / timeout behavior — `fiber_runtime.c`
      has `channel_send` / `channel_recv` only, both single-channel and blocking.
- [ ] Type-check each operation against its channel element type, merge arm
      result types, then remove `[CONCURRENCY-SELECT-REJECT]` from
      `osprey-types/src/expr.rs` and from
      [spec 0011](../specs/0011-LightweightFibersAndConcurrency.md).
- [ ] Lower the runtime's selected-arm index to control flow, replacing the
      loud-failure `gen_select` above.
- [ ] Deterministic-mode tie-breaking defined and tested
      ([CONCURRENCY-DETERMINISTIC] — the golden, not just the assertions, is the
      oracle here).
- [ ] Assertion suites in both flavors selecting between ≥ 2 channel operations,
      with receive bindings at the channel element type and an explicit
      default/timeout case; goldens byte-identical across flavors and across all
      three memory backends.
- [ ] `make ci` green.
