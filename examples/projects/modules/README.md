# Talon Bank — the Osprey modules showcase

A complete, layered banking application in ML-flavor Osprey. One
`osprey.toml` project, eight source files, every module-system feature doing
real work: a SQLite library over the C FFI (with a bound prepared-statement
cursor API), a double-entry journal, a storage capability as an algebraic
effect, a JSON API over the data (`/api/accounts`, `/api/activity`, deposits,
withdrawals, transfers, overdraft refusals), and a server-driven web
dashboard — stat tiles, accounts, a color-coded activity feed — that is
*provably* unable to touch the database.

Spec: [docs/specs/0025-ModulesAndNamespaces.md](../../../docs/specs/0025-ModulesAndNamespaces.md) ·
Plan: [docs/plans/0014-modules-and-namespaces.md](../../../docs/plans/0014-modules-and-namespaces.md)

## Run it

```sh
target/release/osprey examples/projects/modules --run
```

Output is deterministic and byte-compared against `expectedoutput` by
`crates/osprey-cli/tests/project_e2e.rs`.

## Architecture

```
src/
  lib/sqlite.ospml       module Sqlite        SQLite library: C FFI externs + typed helpers
  domain/money.ospml     module Money         pure cents arithmetic  (signature-ascribed)
  domain/accounts.ospml  module Accounts      Account record, Outcome union, pure rules
  store/ledger.ospml     module Ledger        effect Store (capability) + SQL implementations
  store/metrics.ospml    state Metrics        the namespace's ONE state module (request counter)
  api/routes.ospml       module Api           JSON routes + effect Audit; performs Store
  web/pages.ospml        module Pages         server-driven HTML; sees ONLY bank::Api
  main.ospml             (entry)              composition root: binds every capability
```

The dependency arrows only point down, and the module system enforces them:

```
web ("bank/web")  ──imports──▶  api  ──performs──▶  Ledger::Store  ──SQL──▶  Sqlite ──▶ libsqlite3
                                 │
                                 └──performs──▶  Api::Audit  (bound to console in main)
```

- **The UI never talks to the database.** `web/pages.ospml` imports only
  `bank::Api`. There is no `Ledger` or `Sqlite` path in scope, so a database
  call from the UI is not merely bad style — it does not resolve.
- **Storage is a capability.** `Ledger::Store` is an algebraic effect. The api
  layer `perform`s it; only `main.ospml` decides what a `debit` physically is.
  An unhandled operation is a compile error. Swapping SQLite for an in-memory
  fake is a main-only edit.
- **Refusal is a domain outcome, not an error.** `debit` returns the
  `Outcome` union (`Landed`/`Refused`); every caller pattern-matches it
  exhaustively, and an overdraft surfaces as HTTP 422 with a reason.
- **Double-entry journal.** Every movement — refusals included — lands as a
  bound-parameter row in `entries`; a transfer writes both sides. The activity
  feed is an audit ledger, not a best-effort log.
- **One state owner.** `state Metrics` owns the only mutable cell in the app.
  The cell is readable and writable exclusively inside its own effect's
  handler arms, and that region exists only while the exported installer
  (`track`) is running.

## Module-system features exercised

| Feature | Where |
|---|---|
| File-scoped `namespace` | every file (`namespace bank`) |
| Quoted namespace + alias import | `web/pages.ospml` (`namespace "bank/web"`), `import "bank/web" as web` in main |
| Cross-file namespace merging | `Account`/`Outcome` declared in domain, used in api with no import |
| `module` with `export` markers | Sqlite, Accounts, Ledger, Api, Pages |
| Signature-ascribed module (no redundant exports) | `module Money : MoneyApi` |
| Signature-ascribed **state** module | `state Metrics : MetricsApi` |
| Effect declared in a signature | `MetricsApi.MetricsFx` |
| Effect exported from a module | `Ledger::Store`, `Api::Audit` |
| Effect rows on module exports | `route : … ! [Ledger::Store, Audit]` |
| Handler installer + private cells | `Metrics::track` |
| Qualified paths in perform/handle | `perform Ledger::Store.debit`, `handle Api::Audit` |
| Member imports | `import bank::Sqlite`, etc. |
| Multi-file project manifest | `osprey.toml` (`source_roots`, `default_namespace`, `entry`) |
| `@link` directive from a non-entry file | `lib/sqlite.ospml` links `sqlite3` |

## Browser end-to-end tests (Playwright)

```sh
cd examples/projects/modules/e2e
npm install && npx playwright install chromium
npx playwright test
```

The suite boots the real compiled binary (the `/tmp/talon_bank.hold` marker
file pauses the demo just before shutdown so the server stays up), then
drives Chromium through the dashboard and the JSON API: seeded balances,
styled rendering, an overdraft refusal (422), and the server-driven proof
that an API mutation appears on the next rendered page.

## Known compiler defects encountered (worked around here, not fixed)

Found while building this app; each reproduces in a few lines. The module
system itself needed **zero** workarounds.

1. **`deleteFile` is a phantom builtin** — typed in
   `crates/osprey-types/src/builtins.rs` but absent from the runtime, so any
   use fails at link time. Workaround: idempotent `DROP TABLE IF EXISTS`.
2. **`Result` loses its variant tag through effect `resume`** — a handler arm
   returning `Error(…)` for an op typed `… => Result<T, E>` matches
   `Success` at the perform site with a garbage payload. User-defined unions
   round-trip correctly, which is why `debit` returns the domain `Outcome`
   union instead.
3. **`HttpResponse` field readback corrupts** — constructing an
   `HttpResponse` and then reading a string field back (e.g. `.partialBody`)
   yields garbage or a segfault. Custom records are unaffected. Workaround:
   `Api::accountsJson` hands the web layer the JSON string directly.
4. **Undocumented HTTP contract** — the `headers` string of a server
   `HttpResponse` is spliced verbatim into the wire response, so every header
   line must be CRLF-terminated (`"Content-Type: text/html; charset=utf-8\r\n"`),
   or browsers receive a mangled header block and fall back to text/plain.
