# Osprey → WebAssembly

Two examples compiled to `wasm32-wasip1` and run under `wasmtime`, Node's WASI,
and in the browser. See
[`docs/specs/0022-WebAssemblyTarget.md`](../../docs/specs/0022-WebAssemblyTarget.md)
for the design.

- **`hello.osp`** — a one-screen language tour (algebraic effects, union types +
  exhaustive matching, HM inference, pipelines, persistent List/Map).
- **`studio.osp` + `studio.ospml`** — **Osprey Data Studio**, the app behind
  [`index.html`](index.html): two cooperating WebAssembly engines in your
  browser. Shipped in **both flavors** — Default (brace) and ML (layout) — that
  emit a **byte-identical** manifest.

## Osprey Data Studio — Osprey × SQLite, both on WebAssembly

A real, sophisticated single-page app that does not just print to the console:

1. **Osprey is the brain.** `studio.osp` models a sales dataset with a `Sale`
   record and `Region`/`Grade` union types, then replays each row through an
   **algebraic-effect handler** (`effect Db`). That one handler *is* the database
   writer and the accountant: it appends an `INSERT` to a seed script and folds
   the row into running totals in the same pass. The module prints a delimited
   **manifest** — schema (DDL), seed (DML), named analytics queries, and Osprey's
   own **gold-standard answers** — over the WASI `fd_write` shim.
2. **SQLite is the engine.** The page loads [`sql.js`](https://sql.js.org)
   (SQLite compiled to WebAssembly) from a CDN, executes Osprey's DDL + seed into
   a live in-browser database, and runs the analytics queries.
3. **The page reconciles them.** Every gold-standard metric Osprey computed in
   pure functions is checked against the same number computed by SQLite over the
   relational data. They agree — two independent WebAssembly engines, one truth.

On top of that: a **live SQL console** (run arbitrary SQL against the seeded
database, ⌘/Ctrl-Enter to execute), result tables for every shipped query, the
**syntax-highlighted source** for both flavors, and the raw manifest.

The headline is the **flavor toggle**: switch between the `studio.osp` (Default)
and `studio.ospml` (ML) WebAssembly builds and the dashboard is identical, because
both lower to the same canonical AST and emit the same bytes
([`docs/specs/0023-LanguageFlavors.md`](../../docs/specs/0023-LanguageFlavors.md),
[`0024-MLFlavorSyntax.md`](../../docs/specs/0024-MLFlavorSyntax.md)).

```sh
make wasm-serve     # build both flavors + sql-driven page, serve, open the browser
```

Then use the flavor toggle, the SQL console, and ↻ Re-run. No bundler, no npm in
the page — only `sql.js` from a CDN (offline, Osprey's metrics + manifest still
render; the live-SQL parts need the CDN).

## Prerequisites

- `clang` with the wasm32 backend (any recent LLVM)
- `wasm-ld` — from LLVM's `lld` (`brew install lld`, or `apt-get install lld`)
- A WASI sysroot — `brew install wasi-libc`, or the
  [wasi-sdk](https://github.com/WebAssembly/wasi-sdk). Override the autodetected
  location with `OSPREY_WASI_SYSROOT=/path/to/wasi-sysroot`.
- `node` (for the smoke tests) and, optionally, `wasmtime` (for `--run`).

## Build & run

From the repo root:

```sh
# One target builds everything ready to go: the wasm runtime archive, the hello
# example, BOTH Studio flavors, validation, and WASI/browser-shim smoke-runs
# (both Studio flavors must match one byte-identical golden, studio.expectedoutput).
make wasm
```

Or drive the compiler directly:

```sh
# the language tour
osprey examples/wasm/hello.osp --target=wasm32 --compile -o examples/wasm/build/hello.wasm
osprey examples/wasm/hello.osp --target=wasm32 --run        # uses wasmtime under the hood

# Osprey Data Studio, both flavors — identical manifest from different syntax
osprey examples/wasm/studio.osp   --target=wasm32 --compile -o examples/wasm/build/studio.osp.wasm
osprey examples/wasm/studio.ospml --target=wasm32 --compile -o examples/wasm/build/studio.ospml.wasm
osprey examples/wasm/studio.osp   --run    # the manifest, on the console
diff <(osprey examples/wasm/studio.osp --run) <(osprey examples/wasm/studio.ospml --run) && echo "byte-identical"
```

## In the browser

`index.html` dynamic-imports the shared WASI shim (`wasi-shim.mjs`, `fd_write` →
page + console) and loads SQLite (`sql.js`) on demand. Serve over HTTP (browsers
block `fetch()` of `.wasm` and ES modules from `file://`):

```sh
make wasm-serve                          # build + serve + open the browser
# …or by hand:
cd examples/wasm && python3 -m http.server 8080   # then visit http://localhost:8080/
```

From the devtools console you can drive it directly:

```js
await osprey.loadFlavor("ospml")     // re-run the dashboard from the ML build
osprey.query("SELECT * FROM sales")  // run SQL against the live SQLite database
osprey.state                         // parsed manifest, metrics, db handle
```

## What works / what doesn't

The target is `wasm32-wasip1`; libc (malloc, `sprintf`, `printf`/`puts` → WASI
`fd_write`) comes from wasi-libc, so the portable runtime subset — allocator,
strings, lists, maps, JSON, substituting effects — runs unchanged. Fibers/`spawn`,
explicit effect resumption, HTTP/WebSocket, process APIs, terminal APIs, and
general FFI are rejected by the compiler before LLVM emission. The supplied
browser bridge remains available. File and input access depends on the WASI
host's capabilities.
