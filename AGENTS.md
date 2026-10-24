# CLAUDE.md

**Osprey is in stabilization mode.** Finish and polish existing features and make the tests prove they are complete. Nothing new until what exists is hardened.

## ⚠️ Invariants
- **Subagents are illegal.**
- **Linked worktrees are illegal.** Work only in the primary checkout; never create or recreate a linked worktree.
- **External agents (pi) use only these models:** Kimi K3 (`moonshotai/kimi-k3`), GLM 5.3 (`zai/glm-5.3`, `zai/glm-5.3-flash`), DeepSeek V4.1 (`deepseek/deepseek-v4-pro`, `deepseek/deepseek-flash`) and their flash variants. No other model, ever. If credit is out, ***tell the user***.
- ***You are responsible** for the work of the pi agents. You must review and fix all work that they create. Broken code created by pi is considered broken code created by you.
- **Never ask the user questions.** Use your judgement, record assumptions, act autonomously.
- **Answer punctually, not at length.** A few sentences or a short list; no essays, recaps or restating the question. Mark the one point the user must not miss with an emoji — nothing else gets one.
- **Zero duplicate code.** Edit in place, never create parallel versions. Use deslop: `find-similar` before writing code, `top-offenders` after modifying it.
- **No git** — and never stamp yourself as co-author — unless explicitly requested.
- **Token economics.** Check file size before reading, Grep over Read, smallest diff that solves the problem. Delete dead code, unused imports and stale comments.
- **Never disable, weaken, bypass or narrow a CI gate to get a merge.** Not the rulesets, not a required-check list, not a job's `if:`, not a test's timeout. If a check is red, the code is wrong — fix the code.

## Testing
**A failing test that pins a compiler bug is worth more than a speculative fix.** Code you *believe* is correct is a liability until an assertion proves it.

- Write the test before the fix. If you can only do one, write the test.
- A red test in the tree is a correct outcome. Never weaken, skip or delete a failing test, and never remove an assertion to make one pass.
- Unit tests live inside each crate. Programs in `tests/` run via the differential harness (`crates/run_test_corpus.sh`) under each memory backend and on wasm32 (`OSPREY_TARGET=wasm32`); output must match the sibling `.expectedoutput` byte-for-byte, and an ML twin shares its Default twin's golden.
- `examples/failscompilation/` holds programs the compiler must reject.
- Coverage thresholds live in `coverage-thresholds.json` and only go up.
- Expand existing examples and tests instead of adding files; keep each concise, mixing many language constructs.

## Broken code
On encountering code that fails silently:

- Replace it with a panic and a comment explaining why the code is wrong.
- Write a test that fails because of the missing implementation.
- Report the problem to the user. Do not try to fix the code.

A panic is found in seconds; a silent failure never is. This quarantine is the one place a panic is mandated.

## CI gates
- **Protect the Dependabot auto-merge process.** `dependabot-upgrades` is permanent staging, never a disposable feature branch. Never delete it during merged-branch cleanup, rename it, protect it against the sweep bot, retarget routine updates to main, disable the sweep or consolidation PR, or add CI to staging. Preserve `.github/dependabot.yml` and `.github/workflows/dependabot-automerge.yml`; follow AgentPMO/Shipwright and the Deslop implementation. Updates accumulate automatically in staging and reach main through a fully tested consolidation PR. Any process change requires explicit user authorization.
- **Every release must pass a fresh dependency vulnerability audit of its exact SHA.** Every publishing/deployment job must depend on the unconditional `audit` job, including website-only releases and prereleases. Never skip, suppress, downgrade, or bypass this gate. Scanner/network failures fail the release. `make audit` is the shared local/release check.
- **Never merge with a known failure.** Adjust github settings to prevent merges to main that are red
- **Required checks are pinned** in [`scripts/verify-branch-protection.mjs`](scripts/verify-branch-protection.mjs) and asserted by the `changes` job on every PR. Changing the gate means changing that list and the ruleset together.
- **A new required check skips via a job-level `if:`**, never `on: paths:`/`paths-ignore:`. A path-filtered job never reports, which blocks every merge forever.

## Code rules
- **Files under 500 LOC, functions under 20 LOC.** Refactor when over.
- **FP style everywhere** — pure functions over OOP; name values as constants instead of scattering literals.
- **No placeholders** — fix existing ones or fail with an error.
- **Run `make ci` routinely** — most clippy lints auto-fix; don't hand-fix them.
- **Rust: panics, `unwrap()` and similar are illegal** outside the broken-code quarantine. Return `Result<T, E>` and pattern match.

## Osprey style
- **FP constructs** — immutable types, expressions over statements, algebraic effects for abstraction, ML-style minimal brackets. The best function is a single pure expression. Avoid consecutive statements and assignments, even when they add clarity.
- **Lean on type inference — redundant annotations are defects.** Osprey is Hindley-Milner: every type the compiler can infer must be left off.
  - Never annotate parameters, return types or lambda parameters when inferable: `fn add(a, b) = a + b`, not `fn add(a: int, b: int) = a + b`; `|x| => x * 2`, not `|x: int| => x * 2`.
  - Keep an annotation only when the compiler cannot infer the type: an empty literal with no context (`let xs: List<int> = []`), an `extern`/ambiguous return, or an unconstrained polymorphic type variable. A return annotation never turns `Result<T, E>` into `T` — handle failure with `match` or `?:`.
  - If removing an annotation still compiles with identical output, remove it. Applies to every `.osp` you touch, including `benchmarks/`, docs and website snippets.
- **No consecutive print calls** — consolidate into one interpolated string.

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
- **Makefile** from the repo root: `make ci` (lint + test + build), `make test`, `make build`, `make fmt`, `make run FILE=<path>`. The compiler binary lands at `target/release/osprey`.
- **VSCode extension**: `cd vscode-extension && npm install && npm run compile`; test with `npm test`.
- **Website**: `cd website && npm install && npm run dev` (or `npm run build`). CSS hard budget 1.8k LOC; blogs, specs and docs are prose and share the `prose` CSS name prefix; zero Tailwind.
- **WebCompiler**: `cd webcompiler && npm install && npm start`.
- **Skills** live in `.agents/skills/<name>/SKILL.md`, the location every agent reads; `.claude/skills/<name>` is a link to each for Claude Code. Add a skill there, never under `.claude/`, then run `node lspkit/scripts/link-skills.mjs` (it also repairs a Windows checkout without symlinks).
- **Never commit generated files.** `website/src/spec/*.md` and `website/src/assets/vendor/` are gitignored output of `npm run build` — edit the source in `docs/specs/`, never the copy.

## Architecture
- `crates/` — the compiler pipeline: tree-sitter parse (`osprey-syntax`, grammar in `tree-sitter-osprey/`) → AST (`osprey-ast`) → Hindley-Milner inference (`osprey-types`) → LLVM IR (`osprey-codegen`) → CLI (`osprey-cli`, the `osprey` binary); `osprey-runtime-sys` links the C runtime.
- `compiler/runtime/` — pure-C runtime (fibers, HTTP/WebSocket, system ops), compiled with hardening flags (`-D_FORTIFY_SOURCE=2`, `-fstack-protector-strong`), all warnings as errors. Performance-critical code stays C.
- A language change follows that same path, then lands a regression program in `tests/regressions/` plus a rejection case in `examples/failscompilation/`.

### Package Manager
- The backend is 100% written in Osprey/Supabase - not Rust.
- CP cloud for any APIs that need to sit on Supabase, but prefer SQL triggers and functions where possible

### Language invariants
- Effects are declared with `effect` and discharged by `handler` values or rest-of-block `handle` statements. The compiler rejects a program that performs an effect no handler discharges — ``unhandled effect operations at program entry: E.op; add a matching `handle` `` — reaching through helpers, lambdas passed to HOFs, and fibers (`crates/osprey-types/src/effect_rows.rs`). The remaining limit is representational: closed-program operation summaries, not an effect-row variable in `Type::Fun`.
- Pattern matching is mandatory for `any` and union types.
- All HTTP/WebSocket operations return `Result<T, String>`.
- Fibers are isolated — message passing, no shared memory.
- Effects provide capability-based security; file, HTTP and process sandboxing is configurable.
