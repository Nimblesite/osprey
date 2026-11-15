# Plan 0061 — TypeScript flavor and the Express port

Contract: [spec 0054](../specs/language/0054-TypeScriptFlavorSyntax.md)
Depends on: [spec 0023](../specs/language/0023-LanguageFlavors.md), [spec 0043](../specs/ecosystem/0043-PythonFlavorPorts.md) for the port rules, [spec 0014](../specs/runtime/0014-HTTP.md) for the HTTP server

Goal: TypeScript 7 source, opted in to Osprey, compiles natively, and an Express application written in it serves HTTP with the same wire behavior as real Express for the ported surface.

## Prerequisites in place

- Flavor dispatch and the canonical AST boundary (`src/toolchain/crates/osprey-syntax/src/lib.rs`), with three frontends to copy from: tree-sitter (Default), hand-written (ML) and a pinned third-party grammar (Python).
- Opt-in project discovery for a common extension (`[project].python_sources`) and editor opt-in that leaves other files alone.
- The native HTTP server (`httpCreateServer`, `httpListen`, `httpStopServer`) and a working HTTP framework port to follow: `src/osprey/packages/fastapi/`, with its real-loopback oracle and coverage gate.
- The TypeScript frontend, selection and the first slice of the Express port exist (2026-11-15); each item below says what is done and what is open.

Constraints:
- Parse in process with a pinned Rust parser; no Node.js, `tsc` or JavaScript engine in the compiler or in compiled programs.
- `tsc` is never part of the build: the compiler parses and lowers the source itself. A `tsc` check exists only as an optional lint ([FLAVOR-TS-LINT]).
- No flavor checks below the AST boundary; equivalent multi-flavor programs emit identical LLVM IR.
- Reject unsupported constructs at the original TypeScript position; never approximate JavaScript semantics silently (`int` overflow, string length, `/` on integers).
- Keep HM inference, immutable values, exhaustive matches, `Result` failures and isolated fibers; never introduce `any` implicitly.

## Phase A — parser selection and source routing

- [ ] [FLAVOR-TS-CST] Parser spike: run `oxc_parser` (pinned) over the TypeScript 7 parser conformance corpus (`tests/cases/conformance/parser` and `tests/cases/compiler` syntax cases from the pinned TypeScript repo) and record accepted/rejected agreement with `tsc` 7. Run the same corpus through the parser of the Rust TypeScript 7 ports (oxc-project `tsrs`, pingdotgg `ts-rust`) if either publishes a pinned crate. Acceptance: a recorded agreement table; adopt `oxc_parser` unless a published port parser agrees strictly more often; record the decision and date in spec 0054.
- [x] [FLAVOR-TS-SELECT] Add `Flavor::TypeScript`, `--flavor typescript`, the `// osprey: flavor=typescript` marker, `.ts` selection and `[project].typescript_sources` in `osprey-syntax` and `osprey-project`, rejecting `.tsx`, `.mts`, `.cts` and `.d.ts`. Acceptance: selection-precedence and conflict tests mirror the Python ones; an unlisted `.ts` file in a source root is never compiled. Done: `osprey-syntax/tests/typescript.rs` and `typescript_sources_are_opt_in_and_never_declaration_files` in `osprey-project`.
- [ ] [FLAVOR-TS-CST] Frontend skeleton `src/toolchain/crates/osprey-syntax/src/typescript/` that parses, maps every syntax error to its original byte and UTF-16 position, and rejects every unsupported node with a named diagnostic. Acceptance: no panic on the conformance corpus; every rejection names its construct. Done: the frontend on `oxc_parser` 0.158.0, with 55 rejections asserted at their line and byte column. Open: the conformance-corpus run, UTF-16 positions, and a nesting-depth bound like the other frontends' `MAX_SYNTAX_DEPTH`.

## Phase B — optional `tsc` lint

- [x] [FLAVOR-TS-LINT] An optional compatibility lint: `make typescript-lint` runs the pinned `tsc` 7 (`scripts/typescript/`) over every TypeScript-flavor source against `scripts/typescript/osprey.d.ts`, with builtin declarations translated from `osprey --hover`. The compiler contains no TypeScript tooling and never runs it. Acceptance: it rejects a known-bad program, and passes on the Express port and examples (3 sources, 2026-11-15). CI may run it beside the other linters; it is not a build or test prerequisite.

## Phase C — core lowering

- [x] [FLAVOR-TS-FUNCTIONS] Functions, arrow functions, generics, explicit type application, `const`/`let` with assignment analysis, tail-only `return`, and rejection of non-`Unit` expression statements. Done: reassigned `let` and parameters rebind as receiver statements do, with compound assignment; guard clauses, the program entry ([FLAVOR-TS-ENTRY]) and receiver statements ([FLAVOR-TS-RECEIVER]) were added.
- [x] [FLAVOR-TS-TYPES] Interfaces and object types to records, function-valued fields, numeric aliases to sized types, `any` and `unknown` to Osprey `any`; rejection of every type form spec 0054 lists.
- [x] [FLAVOR-TS-UNIONS] `satisfies` and annotation contexts for object literals, `kind`-discriminated unions to sum types, `Result`/`Success`/`Error`, `switch` on `kind` to exhaustive `match`.
- [ ] [FLAVOR-TS-EXPRESSIONS] Operators, template literals, `for … of`, spread update, list `map`/`filter`/`reduce`; binary64-only `/`, `intDiv`, remainder sign tests against JavaScript; string `.length` rejected until a UTF-16 count builtin exists. Done: operators, template literals, `for … of`, spread update, `console.log`, `new Map()`, `map`/`filter`/`reduce` as the list operations, and `.length` as `listLength`, so a string's `.length` fails to type-check. Open: the JavaScript remainder tests, which the TypeScript corpus twins carry.
- Acceptance for Phase C (open, except the first clause, which is done): positive and rejection cases in `src/toolchain/crates/osprey-syntax/tests/typescript.rs`; four-flavor programs in `src/osprey/tests/` share Default goldens under GC, ARC and wasm32; `cross_flavor_ir_equiv.rs` proves identical IR for flat-call quadruples.

## Phase D — effects, handlers, modules and fibers

- [ ] [FLAVOR-TS-EFFECTS] (done so far: `Requires<T, […]>` on a field or method signature) `Effect`/`StaticEffect` interfaces, `Control`/`Once`/`Many`/`Replayable`, `perform<E>().op(…)`, `handler<E>`, `handle<E>`, `resume`, `Requires<T, […]>`. Acceptance: TypeScript twins of the staged-effect and handler tests produce the Default twins' goldens and diagnostics.
- [ ] [FLAVOR-TS-INTEROP] (done so far: imports, exports, directives, package default imports and renamed exports) ES imports and exports over namespace/module directives, package default imports, fibers. Acceptance: mixed projects import between all four flavors; the multi-flavor project-graph IR gate grows to cover TypeScript.

## Phase E — tooling

- [ ] [FLAVOR-TS-VERIFICATION] LSP opt-in for TypeScript-flavor documents beside the editor's TypeScript service (VS Code language configuration in `src/toolchain/vscode-extension/`), test discovery, docs examples, corpus backends, web compiler selection, CI and release change classification. Acceptance: an ordinary `.ts` file in the workspace is never claimed; an opted-in file gets Osprey diagnostics and `tsc` diagnostics together; `make ci` passes.

## Phase F — Express port

- [x] [TS-PORT-EXPRESS-LAYOUT] Create `src/osprey/packages/express/` with README (pinned Express version, API map, divergences), `oracle/` running real Express on Node.js, and the suite project. Done: the package, README, license notice, oracle, and `tests/`, a TypeScript-flavor suite in the package matrix that serves the routes example over loopback under GC and ARC and checks each reply against the Express recording.
- [ ] [TS-PORT-EXPRESS-SURFACE] [TS-PORT-EXPRESS-HANDLERS] Port `express()`, routing methods, path parameters, query, headers, `res.status`/`set`/`json`/`send`, middleware with `next()`, `express.Router()` mounting, error middleware and typed JSON body decoding. Done: `express()`, `get`/`post`/`put`/`delete`, `:name` matching, `res.status`/`set`/`send`/`json`, `req.params`, `req.query`, `app.use(path, handler)` with `req.next()`, error handlers through `req.fail` and `app.onError`, and the final handler. Express's own `examples/web-service` is ported as `examples/web-service`. Open: `req.get`, `Router`, `app.use` without a path and JSON request bodies.
- [x] [TS-PORT-EXPRESS-SERVE] `app.listen` over `httpCreateServer`/`httpListen`, holding the process open through `httpKeepAlive` ([HTTP-SERVER-KEEPALIVE]) and returning a `Server` value, because Express code discards the result and Osprey rejects a discarded `Result`.
- [ ] [TS-PORT-EXPRESS-ORACLE] Real-loopback wire comparison against pinned Express for the corpus spec 0054 lists, under GC and ARC, with a coverage threshold entry in `src/osprey/tests/coverage-thresholds.json`. Acceptance: `make` target runs the suite; every reply matches Express or is asserted as a listed divergence. Done: `make -C src/osprey/packages express-e2e` compares 112 replies from three examples with Express 5.3.0 under GC and ARC, including the status line's reason phrase; the package suite is gated at 96 percent in `coverage-thresholds.json`. Open: the corpus for the surface not yet ported.
- [x] [TS-PORT-EXPRESS] Example application: a TypeScript-flavor HTTP service using the port, built and served natively, with an end-to-end test that drives it over HTTP. `examples/hello-world/index.ts` is the Express README example, byte for byte; `examples/routes/index.ts` is an original multi-route service.

## Phase G — later

- [ ] [FLAVOR-TS-UNIONS] Flavor-neutral contextual record typing in the shared checker, so an object literal needs no `satisfies` where the expected type is known; it must also have a Default spelling and must not branch on flavor.
- [x] [TS-PORT-EXPRESS-BENCH] Throughput and latency measurement against Node.js Express with matched requests, recorded hardware and repeated trials, as the FastAPI benchmark does. Done: `make -C src/osprey/packages express-bench`; the table is in the package README. Under ARC throughput is within 8 percent of Express either way across runs, in about a fiftieth of the memory; GC answers 18 to 24 percent fewer requests per second. Parallel handlers wait on [#397](https://github.com/Nimblesite/osprey/issues/397).
