# Stateful HTTP — state at every level of the API, with algebraic effects

Task server and terminal client using value effects for storage, metrics, persistence, and UI state. Each layer installs a block-scoped `handle`; application functions request operations with `perform`.

| File | What it is |
|------|------------|
| [`server.osp`](server.osp) | A live HTTP task server. `Db` (application data), `Metrics` (server runtime) and `Persist` (on-disk snapshot) are three effects, each owning its mutable state in the handler installed for that layer. Routes request effects without choosing their implementation. |
| [`tui.osp`](tui.osp)       | A colored, keyboard-driven HTTP **client** for the server. Its `Model` effect owns the client-side view state (cached list, cursor, flash, add-counter); every keystroke mutates it by `perform`ing. |

A self-contained, deterministic version (server + scripted client in one
process, exercised by CI) lives at
[`../../tests/regressions/effects/http_state_levels.test.osp`](../../tests/regressions/effects/http_state_levels.test.osp).

## Run it

```bash
osprey examples/statefulhttp/server.osp --run    # terminal 1
osprey examples/statefulhttp/tui.osp --run        # terminal 2
```

TUI keys: `j`/`k` move · `a` add a task · `r` refresh · `q` quit.

## Why this is the state showcase

Handler-owned state (`[EFFECTS-HANDLER-STATE]`) means a `mut` an effect handler
captures becomes a shared cell the handler owns. The effects reach the handler
**across the C HTTP-callback boundary** (the server runs the effectful request
handler and its `perform`s resolve to the server's handlers) and **across fiber
boundaries** (an effect performed in a `spawn`ed fiber is handled in the
spawner). Swap the handlers in `main` and the same routes/UI run against a mock
— the logic above never changes. See
[`docs/specs/0017-AlgebraicEffects.md`](../../docs/specs/0017-AlgebraicEffects.md).
