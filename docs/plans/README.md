# Implementation Plans — Unfinished Compiler Features

Each plan below targets a feature at some point on the road to done: most are
**partially finished** (the compiler handles some cases but bails or no-ops on
the rest), a few are **mostly done** with a clearly-scoped remainder, and one
(0008 single-shot resume) is complete and folded into a successor roadmap.
Plans are ordered roughly by leverage (how much else they unblock) balanced
against scope. Every plan ends with a TODO checklist, and each carries a
**§What is left** section that spells out the remaining work with concrete
repros.

| # | Plan | Subsystem | Status today | Scope |
|---|------|-----------|--------------|-------|
| [0002](0002-codegen-generic-function-values.md) | Generic functions & lambdas as values | codegen | Slot-driven specialization + let-alias landed; only a returned still-generic lambda bails | Low (remainder) |
| [0004](0004-collection-stdlib-completion.md) | Collection / map stdlib surface | stdlib | `listXxx`/`mapXxx` implemented; spec bare names + a few ops missing | Low–Medium |
| [0005](0005-runtime-result-bridge.md) | HTTP/WebSocket `Result` bridge | runtime | Functions work but return raw `int64_t`, not `Result<T, string>` | Medium |
| [0007](0007-fiber-select.md) | `select` over channels | runtime | Parser + types work; codegen takes first arm; no runtime multiplexing | Medium |
| [0008](0008-algebraic-effects-resume.md) | Effect `resume` / continuations | effects | Single-shot deep `resume` landed; multi-shot now rejected loudly at runtime | Done |
| [0009](0009-lsp-context-and-cross-file.md) | LSP context-awareness & cross-file | lsp | Variable hover (type+docs) landed; completion/sig-help still identifier-only, single-file | Medium |
| [0010](0010-cross-language-benchmark-suite.md) | Cross-language benchmark suite | benchmarks | 22 cases × 5 langs shipped; `intDiv` added; feature-blocked classics (arrays, float) pending | Low–High |
| [0011](0011-arc-gc-implementation.md) | Reclaiming memory backends (tracing GC + ARC) | codegen/runtime | Phase 1 conservative GC shipped (`--memory=gc`); Perceus ARC + Cheney + static-mode not started | High |
| [0012](0012-osprey-debugger.md) | Modern Osprey debugger | compiler/editor/runtime | Spec written; Phase 1 source line debugging in progress | High |
| [0013](0013-ml-flavor-frontend.md) | ML flavor frontend (layout syntax, curry-by-default) | frontend/types/codegen/tooling | Frontend shipped (68 `.ospml` twins, VSIX, equivalence tests); handler *values* + ML must-reject remain | Mostly done |
| [0014](0014-modules-and-namespaces.md) | Modules, namespaces & multi-file apps | frontend/resolver/types/codegen/lsp | Initial Default + ML project compiler is live; opaque manifest aliases and cross-file LSP/interface checking remain | High |
| [0015](0015-generics-and-variance.md) | Generics with `in`/`out` variance & generic effects | frontend/types/codegen (both flavors) | Core + generic-fn-values landed; turbofish + static seam remain | Mostly done |
| [0016](0016-algebraic-effects-and-handlers.md) | Algebraic effects roadmap (resume/handler-values/multi-shot) | effects/types/codegen/runtime | Tail + single-shot resume + generic effects + multi-shot rejection + fiber-perform race fix + lambda-resume type error done; handler values, effect rows (incl. static unhandled-effect checks), wasm effects remain | High |

These were surfaced from `CodegenError::unsupported(...)` call sites, the
`## Status` sections of the language specs (`docs/specs/`), and runtime `TODO`
markers.

> Note: evidence line numbers may drift by a few lines as the code moves —
> anchor on function and diagnostic-message names, which the plans cite
> alongside line numbers for exactly this reason.
