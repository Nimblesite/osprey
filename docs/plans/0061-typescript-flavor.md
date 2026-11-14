# Plan 0061 — TypeScript flavor and the Express port

Contract: [spec 0054](../specs/language/0054-TypeScriptFlavorSyntax.md)
Depends on: [spec 0023](../specs/language/0023-LanguageFlavors.md), [spec 0043](../specs/ecosystem/0043-PythonFlavorPorts.md) for the port rules, [spec 0014](../specs/runtime/0014-HTTP.md) for the HTTP server

Goal: TypeScript 7 source, opted in to Osprey, compiles natively, and an Express application written in it serves HTTP with the same wire behavior as real Express for the ported surface.

## Prerequisites in place

- Flavor dispatch and the canonical AST boundary (`src/toolchain/crates/osprey-syntax/src/lib.rs`), with three frontends to copy from: tree-sitter (Default), hand-written (ML) and a pinned third-party grammar (Python).
- Opt-in project discovery for a common extension (`[project].python_sources`) and editor opt-in that leaves other files alone.
- The native HTTP server (`httpCreateServer`, `httpListen`, `httpStopServer`) and a working HTTP framework port to follow: `src/osprey/packages/fastapi/`, with its real-loopback oracle and coverage gate.
- Nothing TypeScript-specific exists yet.

Constraints:
- Parse in process with a pinned Rust parser; no Node.js, `tsc` or JavaScript engine in the compiler or in compiled programs.
- Every accepted program must also pass the pinned `tsc` 7 against the Osprey ambient library; Osprey may reject valid TypeScript, never the reverse.
- No flavor checks below the AST boundary; equivalent multi-flavor programs emit identical LLVM IR.
- Reject unsupported constructs at the original TypeScript position; never approximate JavaScript semantics silently (`int` overflow, string length, `/` on integers).
- Keep HM inference, immutable values, exhaustive matches, `Result` failures and isolated fibers; never introduce `any`.

## Phase A — parser selection and source routing

- [ ] [FLAVOR-TS-CST] Parser spike: run `oxc_parser` (pinned) over the TypeScript 7 parser conformance corpus (`tests/cases/conformance/parser` and `tests/cases/compiler` syntax cases from the pinned TypeScript repo) and record accepted/rejected agreement with `tsc` 7. Run the same corpus through the parser of the Rust TypeScript 7 ports (oxc-project `tsrs`, pingdotgg `ts-rust`) if either publishes a pinned crate. Acceptance: a recorded agreement table; adopt `oxc_parser` unless a published port parser agrees strictly more often; record the decision and date in spec 0054.
- [ ] [FLAVOR-TS-SELECT] Add `Flavor::TypeScript`, `--flavor typescript`, the `// osprey: flavor=typescript` marker, `.ts` selection and `[project].typescript_sources` in `osprey-syntax` and `osprey-project`, rejecting `.tsx`, `.mts`, `.cts` and `.d.ts`. Acceptance: selection-precedence and conflict tests mirror the Python ones; an unlisted `.ts` file in a source root is never compiled.
- [ ] [FLAVOR-TS-CST] Frontend skeleton `src/toolchain/crates/osprey-syntax/src/typescript/` that parses, maps every syntax error to its original byte and UTF-16 position, and rejects every unsupported node with a named diagnostic. Acceptance: no panic on the conformance corpus; every rejection names its construct.

## Phase B — ambient library and the `tsc` gate

- [ ] [FLAVOR-TS-LIB] Write the Osprey ambient library (`.d.ts`, shipped with the compiler) declaring the builtin types, builtin functions and intrinsics of spec 0054, plus the generated `tsconfig.json` (`noLib`, `strict`, `noImplicitAny: false`). Generate the builtin declarations from the compiler's builtin table so they cannot drift from it. Acceptance: a test fails when a builtin is added without its declaration.
- [ ] [FLAVOR-TS-COMPLIANCE-TYPES] CI gate: run the pinned TypeScript 7 `tsc --noEmit` over every TypeScript-flavor source and accepted fixture in the repository. Acceptance: a fixture that Osprey accepts and `tsc` rejects fails CI; the gate runs in phase 1 of CI with the other static checks.

## Phase C — core lowering

- [ ] [FLAVOR-TS-FUNCTIONS] Functions, arrow functions, generics, explicit type application, `const`/`let` with assignment analysis, tail-only `return`, and rejection of non-`Unit` expression statements.
- [ ] [FLAVOR-TS-TYPES] Interfaces and object types to records, function-valued fields, numeric aliases to sized types, `unknown` to `any`; rejection of every type form spec 0054 lists.
- [ ] [FLAVOR-TS-UNIONS] `satisfies` and annotation contexts for object literals, `kind`-discriminated unions to sum types, `Result`/`Success`/`Error`, `switch` on `kind` to exhaustive `match`.
- [ ] [FLAVOR-TS-EXPRESSIONS] Operators, template literals, `for … of`, spread update, list `map`/`filter`/`reduce`; binary64-only `/`, `intDiv`, remainder sign tests against JavaScript; string `.length` rejected until a UTF-16 count builtin exists.
- Acceptance for Phase C: positive and rejection cases in `src/toolchain/crates/osprey-syntax/tests/typescript.rs`; four-flavor programs in `src/osprey/tests/` share Default goldens under GC, ARC and wasm32; `cross_flavor_ir_equiv.rs` proves identical IR for flat-call quadruples.

## Phase D — effects, handlers, modules and fibers

- [ ] [FLAVOR-TS-EFFECTS] `Effect`/`StaticEffect` interfaces, `Control`/`Once`/`Many`/`Replayable`, `perform<E>().op(…)`, `handler<E>`, `handle<E>`, `resume`, `Requires<T, […]>`. Acceptance: TypeScript twins of the staged-effect and handler tests produce the Default twins' goldens and diagnostics.
- [ ] [FLAVOR-TS-INTEROP] ES imports and exports over namespace/module directives, package default imports, fibers. Acceptance: mixed projects import between all four flavors; the multi-flavor project-graph IR gate grows to cover TypeScript.

## Phase E — tooling

- [ ] [FLAVOR-TS-VERIFICATION] LSP opt-in for TypeScript-flavor documents beside the editor's TypeScript service (VS Code language configuration in `src/toolchain/vscode-extension/`), test discovery, docs examples, corpus backends, web compiler selection, CI and release change classification. Acceptance: an ordinary `.ts` file in the workspace is never claimed; an opted-in file gets Osprey diagnostics and `tsc` diagnostics together; `make ci` passes.

## Phase F — Express port

- [ ] [TS-PORT-EXPRESS-LAYOUT] Create `src/osprey/packages/express/` with README (pinned Express version, API map, divergences), `oracle/` running real Express on Node.js, and the suite project.
- [ ] [TS-PORT-EXPRESS-SURFACE] [TS-PORT-EXPRESS-HANDLERS] Port `express()`, routing methods, path parameters, query, headers, `res.status`/`set`/`json`/`send`, middleware with `next()`, `express.Router()` mounting, error middleware and typed JSON body decoding.
- [ ] [TS-PORT-EXPRESS-SERVE] `app.listen` over `httpCreateServer`/`httpListen`, returning `Result`.
- [ ] [TS-PORT-EXPRESS-ORACLE] Real-loopback wire comparison against pinned Express for the corpus spec 0054 lists, under GC and ARC, with a coverage threshold entry in `src/osprey/tests/coverage-thresholds.json`. Acceptance: `make` target runs the suite; every reply matches Express or is asserted as a listed divergence.
- [ ] [TS-PORT-EXPRESS] Example application: a TypeScript-flavor HTTP service using the port, built and served natively, with an end-to-end test that drives it over HTTP.

## Phase G — later

- [ ] [FLAVOR-TS-UNIONS] Flavor-neutral contextual record typing in the shared checker, so an object literal needs no `satisfies` where the expected type is known; it must also have a Default spelling and must not branch on flavor.
- [ ] [TS-PORT-EXPRESS] Throughput and latency measurement against Node.js Express with matched requests, recorded hardware and repeated trials, as the FastAPI benchmark does. No performance claim before it.
