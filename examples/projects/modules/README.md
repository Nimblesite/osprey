# Talon Bank — a full Osprey web application

Talon Bank is a polished banking operations console whose application logic is
written in Osprey. The browser client is compiled to WebAssembly and owns its
model, routing, validation, updates, view tree, and effect commands. React sits
under that surface as a generic renderer; it does not contain banking logic.

The native Osprey service exposes the protected JSON API and SQLite ledger.
Its algebraic-effect boundary means neither the WebAssembly client nor the
React host can acquire database authority.

## Run the demo

Build the browser bundle after changing the client, host, or styles:

```sh
make bank-web
```

Then launch the complete app:

```sh
make bank
```

Open <http://127.0.0.1:18790>. The demo includes:

- a portfolio overview with live balances and recent activity;
- account browsing, selection, detail, and account creation;
- deposit, withdrawal, and atomic transfer workflows;
- a searchable, filterable audit ledger, including refused movements;
- client-side routes with browser back/forward support;
- loading, validation, success, refusal, and service-failure states;
- a responsive mobile navigation experience; and
- an inspectable security view explaining the capability boundaries.

The generated `src/web/bundle.ospml` is committed, so the native project still
builds without Node or a WASI SDK. `make bank-web` is the explicit regeneration
step.

## Browser architecture

```text
browser event / HTTP response
            │ flat JSON + opaque model
            ▼
 Osprey WebAssembly application
 model · update · routing · validation · view · commands
            │ one declarative JSON tree per render
            ▼
 generic React renderer + browser command host
            │ fetch only; no storage capability
            ▼
       Osprey JSON API
            │ Ledger::Store algebraic effect
            ▼
       SQLite ledger
```

The bridge is deliberately coarse. Osprey produces a complete render envelope,
and the host parses it once before React reconciles the tree. Events return as
small flat messages through the stable `osprey_web_dispatch` WebAssembly
export. `osp_alloc` provides safe host-to-Wasm string allocation. This avoids a
chatty component-level FFI and keeps React replaceable.

The reusable pieces are split clearly:

```text
client/src/
  model.ospml          opaque serializable application model
  update.ospml         event, form, route, and HTTP-response transitions
  ui.ospml             React-neutral element-tree constructors
  commands.ospml       generic HTTP, focus, and navigation commands
  bridge.ospml         the single render boundary
  pages/               complete Osprey page views

web/src/
  host.js              Wasm lifecycle and generic command interpreter
  protocol.js          safe element-tree-to-React adapter
  wasi.js              minimal browser WASI Preview 1 host
  styles.css           responsive Talon design system
```

`web/scripts/build.mjs` compiles the client for `wasm32-wasip1`, bundles React,
minifies the host and CSS, embeds the Wasm bytes, and generates the native
Osprey `Bundle` module served at `/app.js` and `/app.css`.

## Server architecture

```text
src/
  lib/sqlite.ospml       bound SQLite helpers over the C FFI
  domain/money.ospml     exact whole-cent formatting
  domain/json.ospml      central JSON encoding and scalar projection
  domain/accounts.ospml  account records, outcomes, and pure rules
  store/ledger.ospml     Store effect and transactional SQL implementation
  store/metrics.ospml    the namespace's single state owner
  api/routes.ospml       validated JSON routes and Audit effect
  web/pages.ospml        capability-free app shell and embedded assets
  web/bundle.ospml       generated React host, CSS, and Osprey Wasm client
  main.ospml             composition root and capability handlers
```

- **Storage is a capability.** `Ledger::Store` is an algebraic effect. Only the
  composition root binds it to SQLite, so an unhandled or illicit storage
  operation is a compile error.
- **Transfers are atomic.** Debit, credit, and both journal entries commit in
  one SQLite transaction.
- **Refusals are domain outcomes.** Overdraft attempts return HTTP 422 and are
  still recorded in the audit ledger.
- **Money is integral.** The full path uses cents, avoiding floating-point
  drift.
- **Input is bound and output is encoded.** SQL uses prepared parameters, JSON
  strings are escaped centrally, and the React adapter rejects executable DOM
  properties and raw HTML injection.
- **The UI has no database route.** `pages.ospml` serves static app assets and
  imports no Ledger or SQLite module. Browser data crosses only `/api/*`.

## Tests

Run the host unit tests and regenerate the bundle:

```sh
cd examples/projects/modules/web
npm ci
npm test
npm run build
```

Run the real-browser suite against the compiled Osprey server:

```sh
make bank-e2e
```

Playwright covers the public API, WebAssembly boot, bridge telemetry, SPA
navigation, account creation, money movement, refusals, filtering, responsive
layout, and injection-safe rendering.

The deterministic native tour remains byte-compared with `expectedoutput` by
`crates/osprey-cli/tests/project_e2e.rs`.

## Module-system features exercised

| Feature | Where |
|---|---|
| File-scoped and quoted namespaces | native modules and `bank/web` |
| Multi-root Osprey project | browser client plus shared domain modules |
| Signature-ascribed modules and state | `Money`, `Json`, `Metrics` |
| Exported algebraic effects | `Ledger::Store`, `Api::Audit` |
| Effect rows and exhaustive outcomes | API routes and ledger mutations |
| Private state behind an installer | request metrics |
| Stable WebAssembly host ABI | `osprey_web_dispatch`, `osp_alloc` |
| C FFI linking from a library module | SQLite adapter |

The relevant language specifications are
[`0022-WebAssemblyTarget.md`](../../../docs/specs/0022-WebAssemblyTarget.md) and
[`0025-ModulesAndNamespaces.md`](../../../docs/specs/0025-ModulesAndNamespaces.md).
