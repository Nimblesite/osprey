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
- **Protect the Dependabot auto-merge process.** `dependabot-upgrades` is permanent staging, never a disposable feature branch. Never delete it during merged-branch cleanup, rename it, protect it against the sweep bot, retarget routine updates to main, disable the sweep or consolidation PR, or add CI to staging. Preserve `.github/dependabot.yml` and `.github/workflows/dependabot-automerge.yml`; follow AgentPMO/Shipwright and the Deslop implementation. Updates accumulate automatically in staging and reach main through a fully tested consolidation PR. Any process change requires explicit user authorization.
- **Every release must pass a fresh dependency vulnerability audit of its exact SHA.** Every publishing/deployment job must depend on the unconditional `audit` job, including website-only releases and prereleases. Never skip, suppress, downgrade, or bypass this gate. Scanner/network failures fail the release. `make audit` is the shared local/release check.
- **Never merge with a known failure.** Adjust github settings to prevent merges to main that are red
- **One required check: the root `ci` job, named `CI`.** It is GitHub's standard root job: the last job in `ci.yml`, `if: always()`, with `needs:` naming every other job in `ci.yml`, and it fails when any of them failed or was cancelled. Every other workflow that runs on a PR (`ci-windows.yml`, `gpui.yml`, `winui.yml`) is called from `ci.yml` with `uses:`, never triggered by `pull_request` itself. A new job goes into the root job's `needs:`; [`scripts/ci-scope.test.mjs`](scripts/ci-scope.test.mjs) fails when one is missing. The ruleset requires `CI` and nothing else: never list jobs in it, and never replace the root job with a script that polls the API. [`scripts/verify-branch-protection.mjs`](scripts/verify-branch-protection.mjs) asserts this from the `changes` job on every PR.
- **A new required check skips via a job-level `if:`**, never `on: paths:`/`paths-ignore:`. A path-filtered job never reports, which blocks every merge forever.

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
