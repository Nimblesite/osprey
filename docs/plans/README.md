# Implementation Plans — Unfinished Compiler Features

Each plan below targets a feature that is **partially finished**: the compiler
already handles some cases but bails (or no-ops) on the rest. Plans are ordered
roughly by leverage (how much else they unblock) balanced against scope. Every
plan ends with a TODO checklist.

| # | Plan | Subsystem | Status today | Scope |
|---|------|-----------|--------------|-------|
| [0002](0002-codegen-generic-function-values.md) | Generic functions & lambdas as values | codegen | Concrete capture-free/capturing lambdas work; generic ones bail | Medium |
| [0004](0004-collection-stdlib-completion.md) | Collection / map stdlib surface | stdlib | `listXxx`/`mapXxx` implemented; spec bare names + a few ops missing | Low–Medium |
| [0005](0005-runtime-result-bridge.md) | HTTP/WebSocket `Result` bridge | runtime | Functions work but return raw `int64_t`, not `Result<T, string>` | Medium |
| [0007](0007-fiber-select.md) | `select` over channels | runtime | Parser + types work; codegen takes first arm; no runtime multiplexing | Medium |
| [0008](0008-algebraic-effects-resume.md) | Effect `resume` / continuations | effects | Handlers work as value substitution; no `resume` | High |
| [0009](0009-lsp-context-and-cross-file.md) | LSP context-awareness & cross-file | lsp | Variable hover (type+docs) landed; completion/sig-help still identifier-only, single-file | Medium |
| [0010](0010-cross-language-benchmark-suite.md) | Cross-language benchmark suite | benchmarks | 18 cases × 5 langs shipped; `intDiv` added; feature-blocked classics (arrays, float) pending | Low–High |
| [0012](0012-osprey-debugger.md) | Modern Osprey debugger | compiler/editor/runtime | Spec written; Phase 1 source line debugging in progress | High |
| [0013](0013-ml-flavor-frontend.md) | ML flavor frontend (layout syntax, curry-by-default) | frontend/types/codegen/tooling | Specs written ([0023](../specs/0023-LanguageFlavors.md)/[0024](../specs/0024-MLFlavorSyntax.md)); no ML frontend yet | High |
| [0014](0014-modules-and-namespaces.md) | Modules, namespaces & multi-file apps | frontend/resolver/types/codegen/lsp | Spec written ([0025](../specs/0025-ModulesAndNamespaces.md)); parser has only early `import`/`module` grouping | High |
| [0015](0015-generics-and-variance.md) | Generics with `in`/`out` variance & generic effects | frontend/types/codegen (both flavors) | Implemented: fn/type/effect type params, variance checking, per-site effect instantiation | High |

These were surfaced from `CodegenError::unsupported(...)` call sites, the
`## Status` sections of the language specs (`docs/specs/`), and runtime `TODO`
markers. Features that are **entirely** unstarted (memory management /
[0018](../specs/0018-MemoryManagement.md), fiber-isolated modules
[0011](../specs/0011-LightweightFibersAndConcurrency.md) §modules) are out of
scope here per the "prefer partially-finished" directive, and are noted only for
context.

> Note: evidence line numbers were verified against HEAD `4fbc2183`. A concurrent
> commit refactored capture analysis into `crates/osprey-codegen/src/freevars.rs`;
> a couple of cited lines may drift by ±1 — anchor on the function/message names.
