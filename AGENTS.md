# AGENTS.md

**Stabilization mode:** harden existing features until tests prove them complete. ❌ new features.

## Workflow
- ❌ subagents, linked worktrees, git hooks (delete them). Primary checkout only; git = commit/push.
- **pi agents — exact ids only:** `kimi-coding/k3.1-pro-highspeed`, `zai/glm-5.3`, `zai/glm-5.3-flash`, `deepseek/deepseek-flash`. `pi auth check --model <id>` first; out of credit → tell user, switch model.
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
- **Memory:** pluggable, but only GC and ARC. ❌ build, test or bench the default backend; remove it from tests/benches as you touch them.
- **Package manager:** backend is Osprey + Supabase, ❌ Rust. Prefer SQL triggers/functions; APIs that must sit beside Supabase go on GCP.
