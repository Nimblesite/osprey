# Plan 0007 — `select` Over Channels

**Subsystem:** syntax, AST, types, codegen, native runtime  
**Status:** implemented on native; awaiting `make ci`  
**Contract:** [CONCURRENCY-SELECT](../specs/language/0011-LightweightFibersAndConcurrency.md#choosing-among-channel-operations-concurrency-select)

## Required implementation

1. Specify an arm model that retains each `send` or `recv` operation and any
   receive binding through syntax lowering.
2. Add a native runtime primitive for waiting on multiple buffered channel
   operations, including explicit default or timeout behavior.
3. Type-check each operation against its channel element type and merge arm
   result types; then remove `[CONCURRENCY-SELECT-REJECT]`.
4. Lower the runtime's selected arm index to control flow, replacing the current
   loud-failure `gen_select` implementation.

## Acceptance

- Default and ML programs select between at least two channel operations.
- Receive bindings have the channel element type.
- Default or timeout behavior is tested explicitly.
- Deterministic mode defines and tests tie-breaking.
- Direct AST compilation cannot silently choose the first arm.

## TODO

- [x] Direct AST compilation cannot silently choose the first arm: arms now hold their
      operations, and the runtime reports the arm it performed (`select_calls_the_runtime_and_switches_on_the_arm_it_performed`).
- [x] Arm model retains each `send` / `recv` operation and any receive binding through
      lowering in both flavors: `Expr::Select { operations, arms }`, with every walker
      visiting the operands.
- [x] Native runtime primitive `channel_select` waits on multiple buffered channel
      operations, with explicit default and timeout behavior.
- [x] Each operation is type-checked against its channel's element type, arm results
      merge, and `[CONCURRENCY-SELECT-REJECT]` is replaced by
      [`[CONCURRENCY-SELECT]`](../specs/language/0011-LightweightFibersAndConcurrency.md).
- [x] The runtime's selected arm lowers to control flow in `gen_select`.
- [x] Deterministic-mode tie-breaking defined and tested
      (`tests/regressions/fiber/fiber_determinism.test.osp`, golden included).
- [x] Assertion suites in both flavors select between at least two channel operations,
      with receive bindings at the element type and explicit default and timeout cases
      (`tests/regressions/fiber/channel_select.test.osp` and its ML twin, identical IR).
- [ ] `make ci` green.
