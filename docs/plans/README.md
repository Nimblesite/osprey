# Implementation Plans — Unfinished Compiler Features

Each plan tracks unfinished compiler work. A plan is **retired** (struck
through, file deleted) once every checklist item is complete and named tests
prove it; any remaining work moves to a successor plan or a spec's §Status.
Plans are ordered by dependency impact and scope. Each plan ends with a TODO
checklist and records remaining work with concrete repros.

| # | Plan | Subsystem | Status today | Scope |
|---|------|-----------|--------------|-------|
| [0002](0002-codegen-generic-function-values.md) | Generic functions & lambdas as values | codegen | Slot-driven specialization, let-alias and the emit-once specialisation cache landed; **only** a returned still-generic lambda bails (needs per-instantiation cells) | Low (remainder) |
| [0004](0004-collection-stdlib-completion.md) | Collection / map stdlib surface | stdlib/types | `listXxx`/`mapXxx` implemented; bare `length`/`isEmpty` miscompile **fixed** (receiver-directed dispatch). Remaining bare names (`get`/`contains`/`reverse`/`indexOf`) are blocked on overload resolution: `TypeEnv` is one-scheme-per-name and the callee is instantiated before argument 0 is inferred, so it needs a candidate registry + inference reordering + deferred resolution, and costs HM principality until qualified types exist | High |
| [0007](0007-fiber-select.md) | `select` over channels | syntax/runtime | Syntax is reserved; type checking rejects it; no runtime multiplexing | Medium |
| ~~0008~~ | Effect `resume` / continuations | effects | **Done — plan retired.** Single-shot deep `resume` runs on the thread-as-continuation runtime (`__osprey_coro_*`, `effects_runtime.c`); multi-shot aborts with `fatal: continuation already resumed`. Proven by `explicit_resume_runs_the_performer_continuation` (cli_e2e), the paired `tests/effects/resume/` assertion suites, and `multishot_resume_rejected.ospo`. A multi-shot-*capable* runtime, handler values, and effect rows live in [plan 0016](0016-algebraic-effects-and-handlers.md) | — |
| ~~0009~~ | LSP context-awareness & cross-file | lsp | **Done — plan retired.** Completion is filtered by cursor position (`[LSP-COMPLETION-CONTEXT]`, `[LSP-COMPLETION-MEMBER]`), hover covers parameters and written type names (`[LSP-HOVER-WRITTEN]`), signature help triggers on the callee name, and every feature resolves across the project through the compiler's own loader (`[LSP-WORKSPACE]`). 115 `osprey-lsp` tests. The one deliberate remainder — the type of an arbitrary *sub-expression* — needs an expression-keyed table in `osprey-types` and is recorded in [spec 0020](../specs/0020-LanguageServerAndEditors.md) `[LSP-HOVER-WRITTEN]` | — |
| [0010](0010-cross-language-benchmark-suite.md) | Cross-language benchmark suite | benchmarks | 22 cases × 7 langs (+ wasm and ARC/GC backend columns); `intDiv` added; `-O2` + `@osp_alloc` landed and both reclaiming backends ship. `binarytrees`: default 633 MB, `--memory=arc` 2.97 MB, `--memory=gc` 18.5 MB; alternate backends remain opt-in. `make test` replays the differential harness under both backends, and the ARC pass requires `ARC_LEAKY=0`. Left: refresh stale README figures and add blocked benchmark families | Low–High |
| ~~0011~~ | Reclaiming memory backends (tracing GC + ARC) | codegen/runtime | **Done — plan retired.** Both native backends ship (`--memory=gc`, `--memory=arc`). `make test` requires backend-neutral output across the current differential corpus and zero live ARC values at each example's exit. The shipped contracts are documented in [spec 0018](../specs/0018-MemoryManagement.md) | — |
| [0012](0012-osprey-debugger.md) | Modern Osprey debugger | compiler/editor/runtime | Spec written; Phase 1 source line debugging in progress | High |
| [0013](0013-ml-flavor-frontend.md) | ML flavor frontend (layout syntax, curry-by-default) | frontend/types/codegen/tooling | Frontend shipped (68 `.ospml` twins, VSIX, equivalence tests, 5 ML must-reject fixtures); LSP now answers in the **authoring** flavor on one `[FLAVOR-SELECT]` chain (`[LSP-FLAVOR-RENDER]`, spec 0020) and a marker/extension conflict is a diagnostic, not a silent guess; only handler *values* + the optional `osprey convert` remain | Mostly done |
| [0014](0014-modules-and-namespaces.md) | Modules, namespaces & multi-file apps | frontend/resolver/types/codegen/lsp | Default + ML project compilation, project-aware diagnostics, and cross-file resolution are implemented; opaque manifest aliases, separate importer checking against signatures, and an incremental LSP project graph remain | High |
| [0015](0015-generics-and-variance.md) | Generics with `in`/`out` variance & generic effects | frontend/types/codegen (both flavors) | Core + generic-fn-values landed; turbofish + static seam remain | Mostly done |
| [0016](0016-algebraic-effects-and-handlers.md) | Algebraic effects roadmap (resume/handler-values/multi-shot) | effects/types/codegen/runtime | Tail + single-shot resume + generic effects + multi-shot rejection + fiber-perform race fix + lambda-resume type error done; handler values, effect rows (incl. static unhandled-effect checks), wasm effects remain | High |
| [0019](0019-ml-elegance.md) | ML flavor elegance (inline unions, equational clauses, ML `?:`, positional payloads; historical plain-arithmetic phase superseded by `[ARITH-CHECKED]`) | frontend/types/codegen (both flavors) | Syntax phases shipped. Phase 2's silent-wrap decision is historical only; checked-`Result` arithmetic is the shipped contract and the corpus is migrated. | Medium |
| [0020](0020-package-manager.md) | Source-derived package registry and manager | package core/CLI/API/WASM web/trust plane | Specs 0029–0032 and a 66-source research corpus are complete; **no implementation started** | Very High |

These were surfaced from `CodegenError::unsupported(...)` call sites, the
`## Status` sections of the language specs (`docs/specs/`), and runtime `TODO`
markers.

> Note: evidence line numbers may drift as code moves. Use the cited function
> and diagnostic-message names as stable anchors.
