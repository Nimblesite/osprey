# CLAUDE.md
<!-- agent-pmo:74cf183 -->

Guidance for agents working in this repository.

**Osprey is in stabilization mode.** One measure outranks everything else — not new features, not new syntax, not more examples:

> **Every existing feature is stable, accurate and reliable. A program that compiles behaves exactly as documented; a program that must be rejected is rejected with a truthful error.**

Nearly-complete features get finished. Completely broken features get removed and reported, not papered over. Before adding anything new, ask whether the effort would better harden something that exists.

That doesn't mean taking shortcuts to finish features. That means doing the HARD WORK to COMPLETE FEATURES IN THE HIGHEST POSSIBLE quality level

## Tests outrank code

**A failing test that pins a compiler bug is worth more than a speculative fix.** A red test survives refactors and turns a suspicion into an enforceable contract. Code you *believe* is correct is a liability until an assertion proves it.

- Write the test before the fix. If you can only do one, write the test.
- A red test in the tree is a correct outcome. Never weaken, skip or delete a failing test, and never remove an assertion to make one pass.
- Unit tests live inside each crate. Working programs live in `tests/` and run via the differential harness (`crates/run_test_corpus.sh`) under each memory backend and again on wasm32 (`OSPREY_TARGET=wasm32`); output must match the sibling `.expectedoutput` byte-for-byte. An ML twin shares its Default twin's golden — both flavors must print identically.
- `examples/failscompilation/` holds programs the compiler must reject.
- Coverage thresholds live in `coverage-thresholds.json` and only go up.
- Expand existing examples/tests instead of adding files. Keep examples concise, mixing many language constructs per file.

## 🚨 The pipeline is not negotiable

**A gate you can turn off is not a gate.** On 2026-08-12 both branch rulesets were found `enforcement: disabled` — a 212-file PR merged past them 29 minutes after its own CI failures were filed as issues #202/#203/#204. Nothing in the tree could have noticed, because branch protection lives in GitHub's settings.

- **Never disable, weaken, bypass or narrow a CI gate to get a merge.** Not the rulesets, not a required-check list, not a job's `if:`, not a test's timeout. If a check is red, the code is wrong — fix the code.
- **"Advisory" is deleted.** Marking a job "not a required status check" removes it. If it is worth running it is worth blocking on; if it is not worth blocking on, delete the job and say so.
- **Never merge with a known failure.** An open issue describing a red check on the branch is a blocker, not a footnote. File-and-merge is the exact failure this section exists to prevent.
- The required checks are pinned in [`scripts/verify-branch-protection.mjs`](scripts/verify-branch-protection.mjs), asserted by the `changes` job on every PR. Changing the gate means changing that list and the ruleset together — the check fails until they agree.
- Adding a required check? It must skip via a **job-level `if:`**, never `on: paths:`/`paths-ignore:`. A path-filtered job never reports, and a required check that never reports blocks every merge forever.

## 🚨 Broken Code Process

Upon encountering code that fails silently:

- REPLACE the code with a panic immediately and include comments to explain WHY the code is wrong
- Write a test that fails because of the missing implementation
- Report the problem to the user immediately
- DO NOT TRY TO FIX THE CODE

Silently-wrong output is worse than a crash: a panic is found in seconds; a silent failure never is. This quarantine is the one place a panic is mandated.

## Hard rules

- ⚠️ **Never ask the user questions.** Use your judgement, record assumptions, act autonomously.
- ⚠️ **Zero duplicate code.** Edit in place, never create parallel versions. Use deslop: `find-similar` before writing code, `top-offenders` after modifying it.
- ⚠️ **No git** — and never stamp yourself as co-author — unless explicitly requested.
- ⚠️ **Token economics.** Check file size before reading, Grep over Read, smallest diff that solves the problem. Delete dead code, unused imports, stale comments.
- **No placeholders** — fix existing ones or fail with an error.
- **Files under 500 LOC, functions under 20 LOC.** Refactor when over.
- **FP style everywhere** — pure functions over OOP; name values as constants instead of scattering literals.
- **Run `make ci` routinely** — most clippy lints auto-fix; don't hand-fix them.

## Documentation

- Spec IDs are hierarchical descriptive slugs — `[GROUP-TOPIC]` or `[GROUP-TOPIC-DETAIL]`, never numbered. Code implementing a spec section references its ID in a comment (`// Implements [PARSER-EFFECTS-HANDLE]`) so grep finds spec → code → tests. Code, specs and tests must agree; where they don't, fix the stale source.
- Let prose wrap naturally — no line endings for forced wrapping.
- ⚠️ **ASCII-art diagrams are illegal** — [typeDiagram](https://typediagram.dev/docs/language-reference.html) for data types, mermaid for everything else.
- Before touching the README, website, specs, docs, examples, release notes or user-facing comments, read [`docs/messaging.md`](docs/messaging.md).

## Osprey style

- **FP constructs**: immutable types, expressions over statements, algebraic effects for abstraction, ML-style minimal brackets. The best function is a single pure expression. Avoid consecutive statements and assignments, even when they add clarity.
- **Lean on type inference — redundant annotations are defects.** Osprey is Hindley-Milner: every type the compiler can infer must be left off.
  - Never annotate function parameters, return types or lambda parameters when inferable: `fn add(a, b) = a + b`, not `fn add(a: int, b: int) = a + b`; `|x| => x * 2`, not `|x: int| => x * 2`.
  - Keep an annotation only when the compiler cannot infer the type: an empty literal with no context (`let xs: List<int> = []`), an `extern`/ambiguous return, or an unconstrained polymorphic type variable. A return annotation never turns `Result<T, E>` into `T` — handle failure with `match` or `?:`.
  - If removing an annotation still compiles with identical output, it was redundant — remove it. Applies to every `.osp` you touch: `tests/regressions/`, `benchmarks/`, docs and website snippets.
- **No consecutive print calls** — consolidate into one interpolated string.

## Rust

- **Panics are illegal** outside the broken-code quarantine. Return `Result<T, E>`.
- **`unwrap()` and similar are illegal.** Use pattern matching.

## Commands

- Use the Makefile from the repo root: `make ci` (lint + test + build), `make test`, `make build`, `make fmt`, `make run FILE=<path>`. The compiler binary lands at `target/release/osprey`.
- **VSCode extension**: `cd vscode-extension && npm install && npm run compile`; test with `npm test`.
- **Website**: `cd website && npm install && npm run dev` (or `npm run build`). **CSS hard budget 1.8k LOC**; blogs/specs/docs are prose and share the `prose` CSS name prefix; **zero Tailwind**.
- **WebCompiler**: `cd webcompiler && npm install && npm start`.
- **Never commit generated files.** `website/src/spec/*.md` (from `docs/specs/`) and `website/src/assets/vendor/` are gitignored build output regenerated by `npm run build` — edit the source, never the copy.

## Architecture

- `crates/` — the compiler pipeline: tree-sitter parse (`osprey-syntax`, grammar in `tree-sitter-osprey/`) → AST (`osprey-ast`) → Hindley-Milner inference (`osprey-types`) → LLVM IR (`osprey-codegen`) → CLI (`osprey-cli`, the `osprey` binary); `osprey-runtime-sys` links the C runtime.
- `compiler/runtime/` — pure-C runtime (fibers, HTTP/WebSocket, system ops), compiled with hardening flags (`-D_FORTIFY_SOURCE=2`, `-fstack-protector-strong`), all warnings as errors. Performance-critical code stays C.
- `vscode-extension/` (TypeScript), `website/` (11ty), `webcompiler/` (Node service), `homebrew-package/`.

Key invariants:

- Effects are declared with `effect` and discharged with `handle...in`. The compiler rejects a program that performs an effect no handler discharges — `unhandled effect operations at program entry: E.op; add a matching handle` — reaching through helpers, lambdas passed to HOFs, and fibers (`crates/osprey-types/src/effect_rows.rs`). The remaining limit is representational: closed-program operation summaries, not an effect-row variable in `Type::Fun`.
- Pattern matching is mandatory for `any` types and union types.
- All HTTP/WebSocket operations return `Result<T, String>`.
- Fibers are isolated — message passing, no shared memory.
- Effects provide capability-based security; file, HTTP and process sandboxing is configurable.

Workflow for a language change: grammar in `tree-sitter-osprey/` → `osprey-syntax`/`osprey-ast` → type rules in `osprey-types` → `osprey-codegen` → C runtime functions if needed → regression program in `tests/regressions/` + rejection case in `examples/failscompilation/`.
