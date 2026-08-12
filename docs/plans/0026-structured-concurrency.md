# Plan 0026 — Structured Concurrency: Cancellation and Turn Isolation

**Subsystem:** `tree-sitter-osprey` + `crates/osprey-syntax` (both flavors) +
`crates/osprey-ast` + `crates/osprey-types` (effect rows, turn graph) +
`crates/osprey-codegen` + native runtime (`fiber_runtime.c`,
`effects_runtime.c`)
**Status:** design spec written; **no implementation started**
**Spec:** [0036-StructuredConcurrency.md](../specs/0036-StructuredConcurrency.md)

## Summary

One mechanism carries the whole design: a suspended fiber is a continuation a
handler holds, so cancellation is *declining to resume* plus finalizers, and
serialization is *resuming one performer at a time*. Both halves grow from
shipped behavior — the thread-as-continuation resume runtime and the
[EFFECTS-FIBER-PERFORM](../specs/0017-AlgebraicEffects.md#resuming-handlers)
round-trip serialization — rather than adding a parallel subsystem.

## Existing seams this builds on

- `effects_runtime.c` / `__osprey_coro_*`: the continuation representation to
  drop instead of resume; its per-handler serialization is the proto-turn.
- `crates/osprey-types/src/effect_rows.rs`: the closed-program fixed point
  that already knows which operations a body can reach — reused twice, for
  [CANCEL-POINTS] (suspension visibility) and [SERIAL-REENTRANCY] (turn-graph
  cycles).
- `fiber_runtime.c`: channel wait queues that [CANCEL-KILLSAFE] must make
  dequeue-safe; the completed-result roots that scope teardown generalizes.
- [Plan 0007](0007-fiber-select.md): the multi-channel wait primitive `race`
  and `within` are specified over.
- ARC leak discipline (`make test` zero-leak gate): a dropped continuation
  must release its captures — same family as
  [issue #185](https://github.com/Nimblesite/osprey/issues/185).

## Phases

1. **Scopes** — `scope` in both grammars, `Expr::Scope`, child registry in the
   runtime, normal-exit wait, root-scope compatibility (no corpus program
   changes meaning). [CANCEL-SCOPE]
2. **Cancellation core** — `cancel(fiber)`, delivery only at suspension
   points, continuation drop + unwinding, cancelled completion state,
   `join`/`Outcome<T>`, await-edge propagation, entry-diagnostic exit.
   [CANCEL-REQUEST] [CANCEL-POINTS] [CANCEL-DELIVERY] [CANCEL-JOIN]
3. **Finalizers** — `finally` arm in both flavors, innermost-first ordering,
   one-shot shielding. [CANCEL-FINALLY]
4. **Kill-safety** — channel wait-queue removal without element loss, turn
   completion under performer cancellation, ARC/GC release of dropped
   continuations. [CANCEL-KILLSAFE]
5. **Deadlines and races** — `within`, `race`; blocked on plan 0007's runtime
   multiplexing primitive; deterministic tie-breaking shared with it.
   [CANCEL-DEADLINE]
6. **Turn isolation** — name the shipped serialization as turns; flat-combining
   turn queue with single-CAS fast path; deterministic-mode total turn order.
   [SERIAL-TURN] [SERIAL-CONTENTION]
7. **Static reentrancy** — turn graph over effect-row summaries, cycle
   rejection with a named-cycle diagnostic, `reentrant` surface (Unit-only,
   enqueue-as-new-turn). [SERIAL-REENTRANCY]
8. **`atomic` / `retry`** — last; multi-region turn acquisition in canonical
   order first, optimistic logging later; row-checked isolation (no
   suspending or outward operations inside the block). [SERIAL-ATOMIC]

## Interactions and risks

- **WebAssembly:** everything here rides the pthread/continuation runtime, so
  it is native-only at first — the same boundary resume already has
  ([0022](../specs/0022-WebAssemblyTarget.md)). Spec 0036 claims must stay
  qualified until a wasm story exists.
- **Deterministic mode** is sequential; `cancel` before the victim's driving
  `await` must still dequeue it, and turn order must be spawn order, or
  goldens diverge between modes.
- **Staged effects** ([0035](../specs/0035-StagedEffects.md)): a static
  region has no runtime turns; the turn graph must be built after static
  discharge so erased handlers never appear in it.
- **Memory backends:** the differential harness runs every corpus program
  under all three backends; dropped continuations are a new ARC leak surface.

## TODO

- [ ] `scope` parsed in both flavors to one `Expr::Scope`; spawn attaches to
      the innermost scope; normal exit waits for children; root scope
      preserves every existing golden byte-for-byte.
- [ ] `cancel`, cancelled completion, `join` returning `Outcome<T>`, await
      propagation; assertion suites in both flavors under all three memory
      backends, ARC zero-leak on dropped continuations.
- [ ] Must-reject fixtures: cancellation "caught" surface does not exist
      (no pattern can match it), `reentrant` op with a non-`Unit` result,
      turn-graph cycle (direct and through a helper lambda), suspending
      operation inside `atomic`, `retry` outside `atomic`.
- [ ] `finally` in both flavors; ordering and shielding pinned by a test that
      cancels a fiber suspended two regions deep.
- [ ] Kill-safe channels: cancel a blocked sender and receiver; element count
      proven conserved in the golden.
- [ ] `within` / `race` after plan 0007's primitive lands; deterministic
      tie-break golden shared with that plan.
- [ ] Flat-combining turn queue benchmarked against the current round-trip
      serialization in `benchmarks/` before it replaces anything.
- [ ] Turn-graph cycle rejection wired into `effect_rows.rs` with a
      named-cycle diagnostic; Orleans-style runtime timeout explicitly *not*
      added.
- [ ] `atomic`/`retry` with row-checked isolation; a blocking-condition test
      (bounded-buffer via `retry`) in both flavors.
- [ ] Spec 0036 sections updated from *normative target* to shipped as each
      phase lands; spec 0011's [CONCURRENCY-CANCEL-DESIGN] pointer replaced
      by real semantics; `[EFFECTS-FIBER-PERFORM]` cross-reference kept
      truthful.
- [ ] `make ci` green.
