# AGENTS.md

**Osprey is in stabilization mode.** Finish and harden existing features until tests prove them complete; nothing new until then.

## Workflow
- **No subagents, no linked worktrees, no git hooks.** Work in the primary checkout; delete any hooks. Git is basic commit/push only.
- **pi agents use only these exact ids:** `kimi-coding/k3.1-pro-highspeed`, `zai/glm-5.3`, `zai/glm-5.3-flash`, `deepseek/deepseek-flash`. Run `pi auth check --model <id>` first; if one is out of credit, tell the user and switch to another.
- **Token economics.** Check file size before reading, grep before reading, smallest diff that solves the problem. Use deslop `find-similar` before writing and `top-offenders` after.
- **Feature entry points are commented make targets** — answer usage questions from `make help`. When demonstrating findings, lead with a verified finding; label clean controls and engine limitations.

## Correctness
- **Accuracy above all.** A false positive or negative is a showstopper: 🛑 stop and tell the user. A screen that lists items without reporting their security warnings is the worst false negative; hide incomplete screens entirely.
- **Test before fix.** A failing test that pins a bug beats a speculative fix; if you can only do one, write the test. A red test is a correct outcome: never weaken, skip or delete one, or drop an assertion.
- **Osprey never panics, aborts or exits.** Every failure is a `Result<T, E>` or a handled effect ([FAILURE-EXPLICIT]) — in the language, stdlib, `packages/`, examples, tests and generated code. No panicking builtin, prelude or runtime entry point; no `extern` binding to `abort`, `exit` or `osprey_*_panic`; no fake zero/empty value standing in for an error. Removing a panic is always in scope.
- **Silent failures:** in Osprey, make them typed errors; in Rust/C compiler and runtime code, quarantine with a commented panic that never surfaces in Osprey. Add a failing test and report it.

## Tests
- **Prefer end-to-end tests** that drive real use cases; write unit tests (inside each crate) only when isolation is needed.
- `tests/` programs run through `crates/run_test_corpus.sh` under GC, ARC and wasm32 (`OSPREY_TARGET=wasm32`), matching the sibling `.expectedoutput` byte-for-byte; an ML twin shares its Default twin's golden.
- `examples/failscompilation/` holds programs the compiler must reject.
- Expand existing tests and examples rather than adding files; mix many constructs per program.
- `coverage-thresholds.json` only goes up.

## CI
CI has three phases, each done once. Do not mix up the phases.
1. **Build and check** — lint, format check, static analysis; cache the build for later phases.
2. **Test** — run every test once, with coverage.
3. **Coverage check** — gate the collected reports against `coverage-thresholds.json`; run no tests.

- **Never weaken a gate or merge red** — not rulesets, job `if:`s or timeouts; fix the code. The only required check is the root `CI` job in `ci.yml` (`if: always()`, `needs:` every job). Other PR workflows are called via `uses:`. Skip jobs with job-level `if:`, never `paths:` filters.
- **Releases depend on `make audit`** of their exact SHA; scanner failures fail the release.
- **`dependabot-upgrades` is permanent staging** — never delete, retarget or change its process without user authorization.

## Code
- **Files under 500 LOC, functions under 20 LOC.** FP everywhere: pure functions, named constants, no placeholders.
- **Rust:** no `panic!`/`unwrap()` outside the quarantine; return `Result` and match. Run `make ci` routinely and let clippy auto-fix.
- **Osprey:** immutable types, expressions over statements, effects for abstraction, ML-style minimal brackets. The best function is one pure expression. One interpolated print, never consecutive prints.
- **Osprey: lean on type inference — redundant annotations are defects.** Osprey is Hindley-Milner: every type the compiler can infer must be left off.
  - Never annotate parameters, return types or lambda parameters when inferable: `fn add(a, b) = a + b`, not `fn add(a: int, b: int) = a + b`; `|x| => x * 2`, not `|x: int| => x * 2`.
  - Keep an annotation only when the compiler cannot infer the type: an empty literal with no context (`let xs: List<int> = []`), an `extern`/ambiguous return, or an unconstrained polymorphic type variable. A return annotation never turns `Result<T, E>` into `T` — handle failure with `match` or `?:`.
  - If removing an annotation still compiles with identical output, remove it. Applies to every `.osp` you touch, including `benchmarks/`, docs and website snippets.

```ospml
    // Wrong: the signature is inferable, so it must be omitted.
    escape : string -> string
    escape s = escapeControls (replace s "\"" "\\\"" ?: "") 1
```

- **Website:** CSS budget 1.8k LOC, `prose`-prefixed classes, zero Tailwind.

## Documentation
- **Spec IDs are hierarchical slugs** (`[GROUP-TOPIC-DETAIL]`), never numbers, cited in code (`// Implements [PARSER-EFFECTS-HANDLE]`) and tests. Where they disagree, fix the stale source.
- Prose wraps naturally. No ASCII art: [typeDiagram](https://typediagram.dev/docs/language-reference.html) for data types, mermaid otherwise.
- Read [`docs/messaging.md`](docs/messaging.md) before touching user-facing text.

## Commands
Run `make help` and read the [`Makefile`](Makefile) first; a new feature adds its command and expected result there. Skills live in `.agents/skills/<name>/SKILL.md` (then `node scripts/link-skills.mjs`). Never commit generated files (`website/src/spec/*.md`, `website/src/assets/vendor/`); edit `docs/specs/`.

## Architecture
- `crates/` — the compiler pipeline: tree-sitter parse (`osprey-syntax`, grammar in `tree-sitter-osprey/`) → AST (`osprey-ast`) → Hindley-Milner inference (`osprey-types`) → LLVM IR (`osprey-codegen`) → CLI (`osprey-cli`, the `osprey` binary); `osprey-runtime-sys` links the C runtime.
- `compiler/runtime/` — pure-C runtime (fibers, HTTP/WebSocket, system ops), compiled with hardening flags (`-D_FORTIFY_SOURCE=2`, `-fstack-protector-strong`), all warnings as errors. Performance-critical code stays C.
- A language change follows that same path, then lands a regression program in `tests/regressions/` plus a rejection case in `examples/failscompilation/`.
- **Memory management is pluggable, but only GC and ARC are supported.** Never build, test or bench the default backend; remove it from tests and benches as you touch them.
- **Package manager** backend is Osprey + Supabase, never Rust. Prefer SQL triggers and functions; put APIs that must sit beside Supabase on GCP.
