# CLAUDE.md

**Osprey is in stabilization mode.** Finish and polish existing features and make the tests prove they are complete. Nothing new until what exists is hardened.

Clean up disk space routinely. If you put temporary files on the hard drive, you need to delete them afterwards. Disk space is your responsibility

🛑 NO FURTHER WORK ON THE DEFAULT OSPREY MEMORY MANAGEMENT APPROACH. WE DON'T NEED TO TEST OR BENCH THE DEFAULT APPROACH. WE ONLY PURSUE GC AND ARC. DO NOT WORK ON THE DEFAULT MEMORY MANAGEMENT APPROACH. WHILE OTHER APPROACHES WILL COME IN FUTURE, GETTING OSPREY APPS TO WORK WITH ANYHING OTHER THAN GC OR ARC IS REDUNDANT. DEFAULT CAN BE SLOWLY REMOVED FROM TESTS, THE BENCH ETC. MEMORY MANAGEMENT REMAINS PLUGGABLE 

## ⚠️ Invariants
- **Subagents are illegal.**
- **Linked worktrees are illegal.** Work only in the primary checkout; never create or recreate a linked worktree.
- **External agents (pi) use only these models, by these exact pi ids:** kimi k3.1-pro-highspeed (`kimi-coding/k3.1-pro-highspeed`), GLM 5.3 (`zai/glm-5.3`, `zai/glm-5.3-flash`), DeepSeek V4.1 Flash (`deepseek/deepseek-flash`). No other model, ever. Check readiness with `pi auth check --model <id>` before launching. If credit is out, ***tell the user*** but switch to one of these that does have credit. No excuses about not enough credit.
- ***You are responsible** for the work of the pi agents. You must review and fix all work that they create. Broken code created by pi is considered broken code created by you.
- **Never ask the user questions.** Use your judgement, record assumptions, act autonomously.
- **Answer punctually, not at length.** A few sentences or a short list; no essays, recaps or restating the question. Mark the one point the user must not miss with an emoji — nothing else gets one.
- **Zero duplicate code.** Edit in place, never create parallel versions. Use deslop: `find-similar` before writing code, `top-offenders` after modifying it.
- **Avoid git** other than basic commit/push. Never stamp yourself as co-author.
- **Git hooks are illegal**. Delete them
- **Token economics.** Check file size before reading, Grep over Read, smallest diff that solves the problem. Delete dead code, unused imports and stale comments.
- **Feature entry points are commented make targets.** Answer usage questions from `make help` first. When demonstrating findings, start with a verified finding example; label clean controls and engine limitations explicitly.
- **Never disable, weaken, bypass or narrow a CI gate to get a merge.** Not the rulesets, not a required-check list, not a job's `if:`, not a test's timeout. If a check is red, the code is wrong — fix the code.

## App Logic

- **Accuracy above all else**
- False negatives or positives are considered showstoppers. Upon encountering a false negative or positive, you must 🛑 stop and tell the user what is wrong
- A screen that lists items but doesn't have any ability to report security warnings for those items is considered the worst kind of false negative.
- If functionality is incomplete and doesn't report security warnings, hide the entire screen

## Testing
**A failing test that pins a compiler bug is worth more than a speculative fix.** Code you *believe* is correct is a liability until an assertion proves it.

- Write the test before the fix. If you can only do one, write the test.
- A red test in the tree is a correct outcome. Never weaken, skip or delete a failing test, and never remove an assertion to make one pass.
- Unit tests live inside each crate. Programs in `tests/` run via the differential harness (`crates/run_test_corpus.sh`) under each memory backend and on wasm32 (`OSPREY_TARGET=wasm32`); output must match the sibling `.expectedoutput` byte-for-byte, and an ML twin shares its Default twin's golden.
- `examples/failscompilation/` holds programs the compiler must reject.
- Coverage thresholds live in `coverage-thresholds.json` and only go up.
- Expand existing examples and tests instead of adding files; keep each concise, mixing many language constructs.

## 🛑 Panics in Osprey are illegal
- **Osprey has no panic, abort, exit or crash, and never will.** Every failure is a typed value: `Result<T, E>` or an effect discharged by a handler ([FAILURE-EXPLICIT]). This applies to the language, the standard library, `packages/`, examples, tests and generated code.
- **Never add a panic builtin**, or any builtin, prelude function or runtime entry point that ends the program instead of returning an error.
- **Never reach a panic through FFI.** No `extern`/`@extern` declaration may bind a C function that panics, aborts or exits (`osprey_*_panic`, `abort`, `exit`, ...). Never use one to dodge a `Result`.
- **Never fake a value to avoid an error.** No zero, empty or placeholder value stands in for a failure; return the error.
- Removing an existing Osprey panic is always in scope. Proposing one is a defect.

## Broken code
On encountering code that fails silently:

- In **Osprey**, make the failure a typed error (`Result` or an effect) that callers must handle, never a panic.
- In **Rust or C** compiler and runtime code only, a panic with a comment explaining why the code is wrong is the quarantine; it never surfaces as an Osprey-level panic.
- Write a test that fails because of the missing implementation.
- Report the problem to the user.

## CI gates
CI has three phases, each done once:
1. **Build and check** — lint, format check, static analysis; cache the build for later phases.
2. **Test** — run every test once, with coverage.
3. **Coverage check** — gate the collected reports against `coverage-thresholds.json`; run no tests.

- **Never weaken a gate or merge red.** The only required check is the root `CI` job in `ci.yml` (`if: always()`, `needs:` every job). Other PR workflows are called via `uses:`. Skip jobs with job-level `if:`, never `paths:` filters.
- **Releases depend on `make audit`** of their exact SHA; scanner failures fail the release.
- **`dependabot-upgrades` is permanent staging** — never delete, retarget or change its process without user authorization.

## Code rules
- **Files under 500 LOC, functions under 20 LOC.** Refactor when over.
- **FP style everywhere** — pure functions over OOP; name values as constants instead of scattering literals.
- **No placeholders** — fix existing ones or fail with an error.
- **Website CSS budget 1.8k LOC**, `prose`-prefixed classes, zero Tailwind.
- **Run `make ci` routinely** — most clippy lints auto-fix; don't hand-fix them.
- **Rust: panics, `unwrap()` and similar are illegal** outside the broken-code quarantine. Return `Result<T, E>` and pattern match.
- **Osprey: FP constructs** — immutable types, expressions over statements, algebraic effects for abstraction, ML-style minimal brackets. The best function is a single pure expression. Avoid consecutive statements and assignments, even when they add clarity.
- **Osprey: lean on type inference — redundant annotations are defects.** Osprey is Hindley-Milner: every type the compiler can infer must be left off.
  - Never annotate parameters, return types or lambda parameters when inferable: `fn add(a, b) = a + b`, not `fn add(a: int, b: int) = a + b`; `|x| => x * 2`, not `|x: int| => x * 2`.
  - Keep an annotation only when the compiler cannot infer the type: an empty literal with no context (`let xs: List<int> = []`), an `extern`/ambiguous return, or an unconstrained polymorphic type variable. A return annotation never turns `Result<T, E>` into `T` — handle failure with `match` or `?:`.
  - If removing an annotation still compiles with identical output, remove it. Applies to every `.osp` you touch, including `benchmarks/`, docs and website snippets.
- **Osprey: no consecutive print calls** — consolidate into one interpolated string.

```ospml
    // Wrong: the signature is inferable, so it must be omitted.
    escape : string -> string
    escape s = escapeControls (replace s "\"" "\\\"" ?: "") 1
```

## Documentation
- **Spec IDs are hierarchical descriptive slugs** — `[GROUP-TOPIC]` or `[GROUP-TOPIC-DETAIL]`, never numbered. Implementing code cites the ID in a comment (`// Implements [PARSER-EFFECTS-HANDLE]`) so grep links spec → code → tests. Where code, specs and tests disagree, fix the stale source.
- **Let prose wrap naturally** — no forced line endings.
- **ASCII-art diagrams are illegal** — [typeDiagram](https://typediagram.dev/docs/language-reference.html) for data types, mermaid for everything else.
- **Read [`docs/messaging.md`](docs/messaging.md)** before touching the README, website, specs, docs, examples, release notes or user-facing comments.

## Commands
Every build, test and feature check is a commented make target: run `make help` and read the [`Makefile`](Makefile) before anything else. A new feature adds its canonical command and expected result there, never here. Skills live in `.agents/skills/<name>/SKILL.md`; run `node scripts/link-skills.mjs` after adding one. Never commit generated files (`website/src/spec/*.md`, `website/src/assets/vendor/`); edit `docs/specs/`.

## Architecture
- `crates/` — the compiler pipeline: tree-sitter parse (`osprey-syntax`, grammar in `tree-sitter-osprey/`) → AST (`osprey-ast`) → Hindley-Milner inference (`osprey-types`) → LLVM IR (`osprey-codegen`) → CLI (`osprey-cli`, the `osprey` binary); `osprey-runtime-sys` links the C runtime.
- `compiler/runtime/` — pure-C runtime (fibers, HTTP/WebSocket, system ops), compiled with hardening flags (`-D_FORTIFY_SOURCE=2`, `-fstack-protector-strong`), all warnings as errors. Performance-critical code stays C.
- A language change follows that same path, then lands a regression program in `tests/regressions/` plus a rejection case in `examples/failscompilation/`.

### Package Manager
- The backend is 100% written in Osprey/Supabase - not Rust.
- CP cloud for any APIs that need to sit on Supabase, but prefer SQL triggers and functions where possible
