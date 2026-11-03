# Plan 0026 — Structured Concurrency: Cancellation and Turn Isolation

Contract: [0036-StructuredConcurrency](../specs/language/0036-StructuredConcurrency.md) [0017-AlgebraicEffects](../specs/language/0017-AlgebraicEffects.md) [0011-LightweightFibersAndConcurrency](../specs/language/0011-LightweightFibersAndConcurrency.md) [0018-MemoryManagement](../specs/compiler/0018-MemoryManagement.md)

## Constraints

- Native-only at first: the whole design rides the pthread/continuation runtime; keep spec 0036 claims qualified until a wasm story exists ([0022-WebAssemblyTarget](../specs/compiler/0022-WebAssemblyTarget.md)).
- Deterministic mode ([CONCURRENCY-DETERMINISTIC]) is sequential: `cancel` before the victim's driving `await` must still dequeue it; turn order must be spawn order.
- Staged effects: retain captured-state ownership and alias evidence through discharge; erased dispatch is absent from the runtime turn graph; residual memory access still requires the same synchronization/exclusivity proof.
- Dropped continuations are a new ARC leak surface: every corpus program runs under `gc` and `arc`; the ARC lane's zero-leak sentinel is the oracle.
- Do not add an Orleans-style runtime deadlock timeout; cycles are rejected statically.

## Prerequisites in place

- `compiler/runtime/effects_coro.c` (`__osprey_coro_*`): thread-as-continuation resume; `in_flight` is the per-handler serialization ([EFFECTS-FIBER-PERFORM]) proto-turn; `abandon_body()` already runs handler finalizers inner→outer ([MULTI-COST-ABORT]).
- Handler `finally` syntax ships in both flavors (`tree-sitter-osprey/grammar.js` `handler_finally`, `tree-sitter-osprey-ml/grammar.js`), tested in `compiler/runtime/effects_finally_tests.h`.
- `crates/osprey-types/src/effect_rows/`: closed-program fixed point reused for [CANCEL-POINTS] and [SERIAL-REENTRANCY].
- `compiler/runtime/fiber_runtime.c`: channel wait queues, completed-result roots, and the `select` primitive ([CONCURRENCY-SELECT]) with source-order deterministic tie-break.

## Phase 1 — Scopes [CANCEL-SCOPE]

- [ ] Add `scope` keyword and `scope_expression` to `tree-sitter-osprey/grammar.js` and `tree-sitter-osprey-ml/grammar.js`; lower both flavors to one `Expr::Scope` in `crates/osprey-syntax/src/default/expr.rs` and `crates/osprey-syntax/src/ml/`; add `Expr::Scope` to `enum Expr` in `crates/osprey-ast/src/lib.rs`. Check: `cargo test -p osprey-syntax` parses `scope { ... }` in both flavors to `Expr::Scope`.
- [ ] Scope registry in `compiler/runtime/fiber_runtime.c`: `spawn` attaches to the innermost scope, normal exit waits for children, and the implicit root scope wraps program entry. Check: `crates/run_test_corpus.sh gc` and `arc` reproduce every existing golden byte-for-byte.

## Phase 2 — Cancellation core [CANCEL-REQUEST] [CANCEL-POINTS] [CANCEL-DELIVERY] [CANCEL-JOIN]

- [ ] Add `cancel` and `join` keywords (both grammars), `Expr::Cancel`/`Expr::Join`, and `Outcome<T>` + `join`/`cancel` builtins in `crates/osprey-types/src/builtins.rs`. Runtime in `fiber_runtime.c`/`effects_coro.c`: cancellation is observed only at suspension points (`await`, `send`, `recv`, `sleep`, `yield`, control ops), declines to resume (drop continuation + finalizers), and completes the fiber as cancelled. Check: both-flavor corpus suite in `tests/regressions/fiber/`.
- [ ] Await propagation: `await` of a cancelled fiber unwinds the awaiter as cancelled; at program entry it runs entry finalizers and exits nonzero with a one-line diagnostic naming the spawn site. Check: must-reject fixture that no pattern can match cancellation (no "caught" surface); golden pins exit status and diagnostic.
- [ ] Dropped continuations release captures: zero ARC leak under `--memory=arc` (`[osp-arc] exit: 0 live objects` sentinel via `crates/run_test_corpus.sh arc`).

## Phase 3 — Finalizers and shielding [CANCEL-FINALLY]

- [ ] Make `cancel` run the existing handler finalizers innermost-first through `abandon_body()` in `compiler/runtime/effects_coro.c`, and add one-shot shielding so a further cancel cannot interrupt cleanup (including at its suspension points). Check: test cancelling a fiber suspended two handler regions deep records finalizer order and shielding.

## Phase 4 — Kill-safety [CANCEL-KILLSAFE]

- [ ] Channel wait-queue removal in `compiler/runtime/fiber_runtime.c`: cancelling a blocked sender or receiver removes it without consuming, duplicating, or losing an element. Check: golden proves element count conserved, under both corpus backends.
- [ ] Turn completion under performer cancellation: a turn whose arm is already running completes and the answer is discarded; a performer still queued is dequeued without the arm running.
- [ ] Dropped continuations release captures under GC and ARC ownership paths (`memory_gc.c`, `memory_arc.c`).

## Phase 5 — Deadlines and races [CANCEL-DEADLINE]

- [ ] `within(ms) { ... } -> Result<T, TimedOut>` and `race(a, b) -> T`: grammar + `Expr` variants; implement over the existing `select` primitive in `fiber_runtime.c`; reuse its source-order deterministic tie-break. Check: golden shared with `tests/regressions/fiber/channel_select.test.osp`.

## Phase 6 — Turn isolation [SERIAL-TURN] [SERIAL-CONTENTION]

- [ ] Name the shipped `in_flight` serialization as turns; implement a flat-combining turn queue with a single-CAS fast path.
- [ ] Before it replaces anything, add a `benchmarks/cases/<name>/` microbenchmark comparing the flat-combining turn queue against the current round-trip serialization.
- [ ] Deterministic mode: total turn order = spawn order of performers.

## Phase 7 — Static reentrancy [SERIAL-REENTRANCY]

- [ ] Turn graph over effect-row summaries in `crates/osprey-types/src/effect_rows/`; reject waiting cycles between distinct owners with a named-cycle diagnostic (direct cycle and cycle through a helper lambda). Check: must-reject fixtures.
- [ ] `reentrant` surface: a `reentrant` op must return `Unit`; performing it inside its own active turn enqueues a new turn. Check: must-reject fixture for a non-`Unit` `reentrant`.

## Phase 8 — atomic / retry [SERIAL-ATOMIC]

- [ ] `atomic { ... }` and `retry`: grammar + `Expr` variants; multi-region turn acquisition in canonical order (optimistic logging later); row-checked isolation rejecting `spawn`/`await`/`send`/`recv`/`sleep` and outward operations inside the block. Check: must-reject fixtures (suspending op inside `atomic`, `retry` outside `atomic`); bounded-buffer blocking-condition test via `retry` in both flavors.

## Phase 9 — Spec bookkeeping and CI

- [ ] Update `docs/specs/language/0036-StructuredConcurrency.md` sections from "normative target" to shipped as each phase lands; replace `docs/specs/language/0011-LightweightFibersAndConcurrency.md` [CONCURRENCY-CANCEL-DESIGN] with real semantics; keep [EFFECTS-FIBER-PERFORM] truthful.
- [ ] `make ci` green.
