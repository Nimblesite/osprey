# Implementation Plans — Unfinished Compiler Features

Each plan below targets a feature at some point on the road to done: most are
**partially finished** (the compiler handles some cases but bails or no-ops on
the rest) and a few are **mostly done** with a clearly-scoped remainder. A plan
is **retired** (struck through, file deleted) once every checklist item is
ticked *and* named tests prove it; its residue, if any, moves to a successor
plan or a spec's §Status. Plans are ordered roughly by leverage (how much else they unblock) balanced
against scope. Every plan ends with a TODO checklist, and each carries a
**§What is left** section that spells out the remaining work with concrete
repros.

| # | Plan | Subsystem | Status today | Scope |
|---|------|-----------|--------------|-------|
| [0002](0002-codegen-generic-function-values.md) | Generic functions & lambdas as values | codegen | Slot-driven specialization, let-alias and the emit-once specialisation cache landed; **only** a returned still-generic lambda bails (needs per-instantiation cells) | Low (remainder) |
| [0004](0004-collection-stdlib-completion.md) | Collection / map stdlib surface | stdlib/types | `listXxx`/`mapXxx` implemented; bare `length`/`isEmpty` miscompile **fixed** (receiver-directed dispatch). Remaining bare names (`get`/`contains`/`reverse`/`indexOf`) are blocked on overload resolution: `TypeEnv` is one-scheme-per-name and the callee is instantiated before argument 0 is inferred, so it needs a candidate registry + inference reordering + deferred resolution, and costs HM principality until qualified types exist | High |
| [0005](0005-runtime-result-bridge.md) | HTTP/WebSocket `Result` bridge | runtime | Functions work but return raw `int64_t`, not `Result<T, string>` | Medium |
| [0007](0007-fiber-select.md) | `select` over channels | runtime | Parser + types work; codegen takes first arm; no runtime multiplexing | Medium |
| ~~0008~~ | Effect `resume` / continuations | effects | **Done — plan retired.** Single-shot deep `resume` runs on the thread-as-continuation runtime (`__osprey_coro_*`, `effects_runtime.c`); multi-shot aborts with `fatal: continuation already resumed`. Proven by `explicit_resume_runs_the_performer_continuation` (cli_e2e), the `effects_resume_*` differential twins, and `multishot_resume_rejected.ospo`. A multi-shot-*capable* runtime, handler values, and effect rows live in [plan 0016](0016-algebraic-effects-and-handlers.md) | — |
| [0009](0009-lsp-context-and-cross-file.md) | LSP context-awareness & cross-file | lsp | Variable hover (type+docs) landed; completion/sig-help still identifier-only, single-file | Medium |
| [0010](0010-cross-language-benchmark-suite.md) | Cross-language benchmark suite | benchmarks | 22 cases × 7 langs (+ wasm and ARC/GC backend columns); `intDiv` added; `-O2` + `@osp_alloc` landed and both reclaiming backends ship — `binarytrees` 633 MB → **2.97 MB** under `--memory=arc` (and faster), 18.5 MB under `--memory=gc`, so it now trails **only on the default backend** since both are opt-in. Left: wire `_conformance-gc`/`_conformance-arc` into CI, actually enforce the `ARC_LEAKY=0` bar, refresh stale README figures; feature-blocked classics (arrays, float) pending | Low–High |
| ~~0011~~ | Reclaiming memory backends (tracing GC + ARC) | codegen/runtime | **Done — plan retired.** Both backends ship (`--memory=gc`, `--memory=arc`) and clear the [MEM-BACKENDS] bar: byte-identical on all 148 differential examples and zero leaked language values under ARC. The remaining roadmap (precise Cheney oracle, `--static-memory`, Perceus borrow inference / drop specialization / reuse) now lives in [spec 0018](../specs/0018-MemoryManagement.md) | — |
| [0012](0012-osprey-debugger.md) | Modern Osprey debugger | compiler/editor/runtime | Spec written; Phase 1 source line debugging in progress | High |
| [0013](0013-ml-flavor-frontend.md) | ML flavor frontend (layout syntax, curry-by-default) | frontend/types/codegen/tooling | Frontend shipped (68 `.ospml` twins, VSIX, equivalence tests, 5 ML must-reject fixtures); LSP now answers in the **authoring** flavor on one `[FLAVOR-SELECT]` chain (`[LSP-FLAVOR-RENDER]`, spec 0020) and a marker/extension conflict is a diagnostic, not a silent guess; only handler *values* + the optional `osprey convert` remain | Mostly done |
| [0014](0014-modules-and-namespaces.md) | Modules, namespaces & multi-file apps | frontend/resolver/types/codegen/lsp | Initial Default + ML project compiler and project-aware diagnostics are live; opaque manifest aliases, cross-file navigation, and interface checking remain | High |
| [0015](0015-generics-and-variance.md) | Generics with `in`/`out` variance & generic effects | frontend/types/codegen (both flavors) | Core + generic-fn-values landed; turbofish + static seam remain | Mostly done |
| [0016](0016-algebraic-effects-and-handlers.md) | Algebraic effects roadmap (resume/handler-values/multi-shot) | effects/types/codegen/runtime | Tail + single-shot resume + generic effects + multi-shot rejection + fiber-perform race fix + lambda-resume type error done; handler values, effect rows (incl. static unhandled-effect checks), wasm effects remain | High |
| [0019](0019-ml-elegance.md) | ML flavor elegance (inline unions, equational clauses, ML `?:`, positional payloads, plain-`int` arithmetic) | frontend/types/codegen (both flavors) | Specs 0024/0013/0004/0003/0023 carry the target surface; **no compiler work started**. Phases 1–2 take binarytrees 22 → 6 lines | Medium–High |
| [0020](0020-package-manager.md) | Source-derived package registry and manager | package core/CLI/API/WASM web/trust plane | Specs 0029–0032 and a 66-source research corpus are complete; **no implementation started** | Very High |

These were surfaced from `CodegenError::unsupported(...)` call sites, the
`## Status` sections of the language specs (`docs/specs/`), and runtime `TODO`
markers.

> Note: evidence line numbers may drift by a few lines as the code moves —
> anchor on function and diagnostic-message names, which the plans cite
> alongside line numbers for exactly this reason.
