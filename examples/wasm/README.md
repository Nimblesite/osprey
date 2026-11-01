# Osprey → WebAssembly

Two examples compiled to `wasm32-wasip1` and run under `wasmtime`, Node's WASI,
and in the browser. See
[`docs/specs/compiler/0022-WebAssemblyTarget.md`](../../docs/specs/compiler/0022-WebAssemblyTarget.md)
for the design.

- **`hello.osp`** — a one-screen language tour (algebraic effects, union types +
  exhaustive matching, HM inference, pipelines, persistent List/Map).
- **`studio.osp` + `studio.ospml`** — **Osprey Data Studio**, the app behind
  [`index.html`](index.html): two cooperating WebAssembly engines in your
  browser. Shipped in **both flavors** — Default (brace) and ML (layout) — that
  emit a **byte-identical** manifest.

## Osprey Data Studio — Osprey × SQLite, both on WebAssembly

1. **Osprey prepares the data.** `studio.osp` models sales with a `Sale` record and `Region`/`Grade` unions. Its `Db` effect handler builds SQL inserts and computes totals in one pass. The module emits a manifest containing the schema, seed data, analytics queries and expected results over WASI `fd_write`.
2. **SQLite executes the queries.** The page loads [`sql.js`](https://sql.js.org) from a CDN, creates an in-browser database from the schema and seed, and runs the analytics queries.
3. **The page checks the results.** It compares Osprey's computed metrics with SQLite's query results.

The page includes a SQL console (⌘/Ctrl-Enter to execute), query result tables, syntax-highlighted source for both flavors and the raw manifest.

The flavor toggle switches between `studio.osp` (Default) and `studio.ospml` (ML). Both builds emit the same manifest, so the dashboard stays the same
([`docs/specs/language/0023-LanguageFlavors.md`](../../docs/specs/language/0023-LanguageFlavors.md),
[`0024-MLFlavorSyntax.md`](../../docs/specs/language/0024-MLFlavorSyntax.md)).

```sh
make -C website wasm-serve     # build both flavors + sql-driven page, serve, open the browser
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
make -C website wasm
```

Or drive the compiler directly:

```sh
# the language tour
osprey examples/wasm/hello.osp --target=wasm32 --memory=none --compile -o examples/wasm/build/hello.wasm
osprey examples/wasm/hello.osp --target=wasm32 --memory=none --run        # uses wasmtime under the hood

# Osprey Data Studio, both flavors — identical manifest from different syntax
osprey examples/wasm/studio.osp   --target=wasm32 --memory=none --compile -o examples/wasm/build/studio.osp.wasm
osprey examples/wasm/studio.ospml --target=wasm32 --memory=none --compile -o examples/wasm/build/studio.ospml.wasm
osprey examples/wasm/studio.osp   --run    # the manifest, on the console
diff <(osprey examples/wasm/studio.osp --run) <(osprey examples/wasm/studio.ospml --run) && echo "byte-identical"
```

## In the browser

`index.html` dynamic-imports the shared WASI shim (`wasi-shim.mjs`, `fd_write` →
page + console) and loads SQLite (`sql.js`) on demand. Serve over HTTP (browsers
block `fetch()` of `.wasm` and ES modules from `file://`):

```sh
make -C website wasm-serve                          # build + serve + open the browser
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
