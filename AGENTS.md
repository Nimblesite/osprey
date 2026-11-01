# AGENTS.md

**Stabilization mode:** harden existing features until tests prove them complete. ❌ new features.

## Workflow
- ❌ subagents, linked worktrees, git hooks (delete them). Primary checkout only; git = commit/push.
- **Offload legwork (repetitive, simple work) to pi agents** — exact ids only, in this precedence: `kimi-coding/k3.1-pro-highspeed` → `zai/glm-5.3` → `zai/glm-5.3-flash` → `deepseek/deepseek-flash`. `pi auth check --model <id>` first; out of credit → tell user, fall to the next. You own their output: review and fix it.
- **Tokens:** check file size before reading, grep first, smallest diff.
- **Entry points = commented make targets.** Answer usage questions from `make help`. Demo findings with a verified finding first; label clean controls and engine limitations.

## Correctness
- **Accuracy above all.** False positive/negative = 🛑 stop and tell user. Listing items without their security warnings is the worst false negative → hide incomplete screens.
- **Test before fix.** If only one, write the test. A red test is correct: ❌ weaken, skip, delete or drop assertions.
- 🛑 **Osprey never panics, aborts or exits.** Every failure is `Result<T, E>` or a handled effect ([FAILURE-EXPLICIT]), everywhere: language, stdlib, `packages/`, examples, tests, generated code. ❌ panicking builtins/prelude/runtime entry points, ❌ `extern` to `abort`/`exit`/`osprey_*_panic`, ❌ fake zero/empty values for errors. Removing a panic is always in scope.
- **Silent failures:** Osprey → typed error. Rust/C compiler/runtime → commented panic as quarantine, never surfacing in Osprey. Add a failing test; report it.

## Tests
- ✅ E2E tests driving real use cases. Unit tests (in-crate) only when isolation is needed.
- **Loaded tests:** many input permutations, many user interactions per test, many assertions per interaction.
- `tests/` programs run via `crates/run_test_corpus.sh` under GC, ARC, wasm32 (`OSPREY_TARGET=wasm32`); output matches sibling `.expectedoutput` byte-for-byte; ML twin shares the Default twin's golden.
- `examples/failscompilation/` = programs the compiler must reject.
- Expand existing tests/examples, ❌ new files; mix many constructs per program.
- 💯 coverage aim per package/project; `coverage-thresholds.json` only goes ⬆️.

## Apps
- **One folder, every app:** [`apps/Makefile`](apps/Makefile) has one target per app. `make -C apps <app>` builds whatever it needs and opens the app on the host OS. ❌ app targets in the root Makefile.
- **Every app has a working Windows version.** It uses WinUI from the latest Windows App SDK (❌ WPF, WinForms, raw Win32 windows). The GPUI examples run on the WinUI host there.
- **Sandboxed by default:** on Windows each app is its own MSIX package in an AppContainer (`uap10:TrustLevel="appContainer"`) that declares only the capabilities it uses. An app that cannot be sandboxed says why in its README.
- **Automated app tests run headless or with a window.** Both modes take screenshots and compare them with committed goldens. They are part of the required `CI` check.

## CI
Three phases, each once — ❌ mixing them:
1. **Build + check** — lint, format check, static analysis; cache the build.
2. **Test** — every test once, with coverage.
3. **Coverage check** — gate collected reports against `coverage-thresholds.json`; ❌ running tests.

- ❌ weakening gates (rulesets, job `if:`, timeouts) or merging red — fix the code. Sole required check: root `CI` job in `ci.yml` (`if: always()`, `needs:` every job). Other PR workflows via `uses:`. Skip with job-level `if:`, ❌ `paths:` filters.
- Releases depend on `make audit` of their exact SHA; scanner failure = release failure.
- `dependabot-upgrades` = permanent staging. ❌ delete, retarget or change its process without user authorization.

## Code
- 🐛 **Duplication and superfluous code are bugs** — copies, parallel versions, dead code, unused imports, stale comments. Fix on sight. Run [deslop](https://deslop.live/docs/for-ai/) routinely: `find-similar` before writing, `top-offenders` after.
- Files < 500 LOC, functions < 20 LOC. FP: pure functions, named constants, ❌ placeholders.
- **Rust:** ❌ `panic!`/`unwrap()` outside quarantine → `Result` + match. Run `make ci` often; let clippy auto-fix.
- **Osprey:** immutable types, expressions over statements, effects for abstraction, ML-style minimal brackets. Best function = one pure expression. One interpolated print, ❌ consecutive prints.
- **Osprey: redundant type annotations are defects** (Hindley-Milner — leave off anything inferable).
  - `fn add(a, b) = a + b`, not `fn add(a: int, b: int)`; `|x| => x * 2`, not `|x: int| => ...`.
  - ✅ annotate only the uninferable: context-free empty literal (`let xs: List<int> = []`), `extern`/ambiguous return, unconstrained type variable. Annotation never turns `Result<T, E>` into `T` — use `match` or `?:`.
  - Still compiles with identical output without it → remove it. Applies to every `.osp` you touch, incl. `benchmarks/`, docs, website snippets.

```ospml
    // ❌ inferable signature — omit it
    escape : string -> string
    escape s = escapeControls (replace s "\"" "\\\"" ?: "") 1
```

- **Osprey: JSON and database access stay at the edges** — decode into typed records in one boundary module (via `@osprey/json`'s `Doc`) or behind an effect; ❌ `Json` values, `JsonInt`/`jsonIndex` matching or SQL in business logic.
- **Website:** CSS ≤ 1.8k LOC, `prose`-prefixed classes, ❌ Tailwind.

## Documentation
- **Spec IDs:** hierarchical slugs (`[GROUP-TOPIC-DETAIL]`), ❌ numbers; cite in code (`// Implements [PARSER-EFFECTS-HANDLE]`) and tests. On disagreement, fix the stale source.
- Prose wraps naturally. ❌ ASCII art: [typeDiagram](https://typediagram.dev/docs/language-reference.html) for data types, mermaid otherwise.
- Read [`docs/messaging.md`](docs/messaging.md) before touching user-facing text.

## Commands
`make help` + [`Makefile`](Makefile) first; new features add their command and expected result there. Skills: `.agents/skills/<name>/SKILL.md`, then `node scripts/link-skills.mjs`. ❌ commit generated files (`website/src/spec/*.md`, `website/src/assets/vendor/`) — edit `docs/specs/`.

## Architecture
- `crates/` pipeline: tree-sitter parse (`osprey-syntax`, grammar `tree-sitter-osprey/`) → AST (`osprey-ast`) → HM inference (`osprey-types`) → LLVM IR (`osprey-codegen`) → CLI (`osprey-cli`, `osprey` binary); `osprey-runtime-sys` links the C runtime.
- `compiler/runtime/`: pure-C runtime (fibers, HTTP/WebSocket, system ops); hardened (`-D_FORTIFY_SOURCE=2`, `-fstack-protector-strong`), warnings = errors. Perf-critical code stays C.
- Language change → same path, plus a regression in `tests/regressions/` and a rejection in `examples/failscompilation/`.
- **Memory:** pluggable; GC and ARC implemented. Compile-time ownership ([plan 0045](docs/plans/0045-compile-time-ownership.md)) will replace `none` — not in active work. ❌ build, test or bench `none`; remove it from tests/benches as you touch them.
- **Package manager:** backend is Osprey + Supabase, ❌ Rust. Prefer SQL triggers/functions; APIs that must sit beside Supabase go on GCP.
