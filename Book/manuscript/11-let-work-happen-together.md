# Chapter 11 — Let work happen together

**Chapter outline.** The full lesson and Flight Log checkpoint are still to be written. The support notes below describe the current implementation rather than the planned concurrency design.

## Reader outcome

Spawn native computations, communicate through typed channels, and collect their results without introducing a separate async function kind. Keep application coordination explicit instead of relying on shared writable state.

## Flight Log state

Independent entries are processed concurrently and return their values through explicit communication.

## Core sections

1. Concurrency is work making progress together
2. A current native fiber uses an operating-system thread
3. Spawn work without coloring every caller
4. Send values instead of sharing writable memory
5. Receive, await, and yield deliberately
6. Keep failures and effects visible across boundaries
7. Explicit completion, native availability, and the limits of today's lifetime model

## Current implementation notes

- A normal native fiber is backed one-to-one by a pthread. Do not describe the current runtime as a pool of lightweight tasks multiplexed onto fewer threads, or promise that spawning thousands of fibers is inexpensive.
- `spawn expression` returns `Fiber<T>`; `await` blocks and returns the complete `T`, including a `Result` when the computation produces one. There is no separate `async` function declaration. Awaiting the same completed fiber again returns the same result value.
- `Channel(capacity)` requires positive capacity. `send` blocks while its buffer is full and returns `Unit`; `recv` blocks while it is empty and returns the channel's element type. One channel has one inferred element type. `select` is reserved syntax that the current checker rejects.
- Managed immutable values may be co-owned across fiber threads. “Send values” does not mean that every value is copied into an independent heap. The runtime handles ownership; this chapter must not teach shared-cell mutation as a coordination technique.
- In normal execution, awaiting tasks in a chosen order gives a chosen order for collecting their results. It does not guarantee the order in which the workers run or print. The runtime's optional deterministic mode queues work and executes it sequentially when awaited; it does not simulate every possible interleaving.
- Native `yield` offers the current thread's remaining time slice and returns its value. Native `sleep` blocks that thread. Neither operation provides the cancellation behavior described in the future structured-concurrency design.
- Lexical `scope`, cancellation, `join` with `Outcome`, deadlines, and races remain planned work in plan 0026. The chapter must explicitly await its finite workers; it cannot promise that a lexical scope cancels them. WebAssembly excludes the native fiber/channel runtime.

## Compiler-feedback exercise

Send an integer and then a string through the same channel. The current compiler rejects the program with `type mismatch: cannot unify int with string`. Repair the channel contract rather than erasing its type. A separate exercise can leave an effect requested by a worker uncovered and follow the compiler's required-handler diagnostic.

## Flight Log checkpoint

Process independent entries in finite fibers, await each result, and print the collected values in a defined order from the parent. Avoid worker output when claiming deterministic stdout. Test channel transfer and effect handling separately from assumptions about scheduling.

## Planned visuals

- Worker computations and explicit communication
- Typed channel handoff
- Spawn, completion, and explicit await

## Source map

- Current contract: `docs/specs/0011-LightweightFibersAndConcurrency.md`, especially `[CONCURRENCY-SPAWN-AWAIT]`, `[CONCURRENCY-CHANNEL]`, `[CONCURRENCY-DETERMINISTIC]`, and `[CONCURRENCY-CANCEL-DESIGN]`.
- Ownership: `docs/specs/0018-MemoryManagement.md`, `[MEM-FIBER-ISOLATION]`.
- Executable evidence: `tests/regressions/fiber/fiber_determinism.test.osp`, `tests/regressions/basics/types/channel_element_type_in_generic_field.test.osp`, and `tests/regressions/effects/fiber_effects.test.osp`.
- Future design, not shipped API: `docs/specs/0036-StructuredConcurrency.md` and `docs/plans/0026-structured-concurrency.md`.
