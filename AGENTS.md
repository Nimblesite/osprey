# CLAUDE.md

**Osprey is in stabilization mode.** Finish and polish existing features and make the tests prove they are complete. Nothing new until what exists is hardened.

## ⚠️ Invariants
- **Subagents are illegal.**
- **Linked worktrees are illegal.** Work only in the primary checkout; never create or recreate a linked worktree.
- **External agents (pi) use only these models:** kimi-k3.1-pro-highspeed or kimi-k3.1-pro-highspeed (K3.1) (`moonshotai/kimi-k3`) -> GLM 5.3 (`zai/glm-5.3`, `zai/glm-5.3-flash`), DeepSeek V4.1-Flash-1017 and their flash variants. No other model, ever. If credit is out, ***tell the user*** but switch to a model that does have credit. No excuses about not enout credit.
- ***You are responsible** for the work of the pi agents. You must review and fix all work that they create. Broken code created by pi is considered broken code created by you.
- **Never ask the user questions.** Use your judgement, record assumptions, act autonomously.
- **Answer punctually, not at length.** A few sentences or a short list; no essays, recaps or restating the question. Mark the one point the user must not miss with an emoji — nothing else gets one.
- **Zero duplicate code.** Edit in place, never create parallel versions. Use deslop: `find-similar` before writing code, `top-offenders` after modifying it.
- **Avoid git** other than basic commit/push. Never stamp yourself as co-author.
- **Git hooks are illegal**. Delete them
- **Token economics.** Check file size before reading, Grep over Read, smallest diff that solves the problem. Delete dead code, unused imports and stale comments.
- **Feature entry points belong in these instructions.** Every new CLI feature must add its canonical example/test path, exact runnable command and expected result under Commands. Answer usage questions from that entry first. When demonstrating findings, start with a verified finding example; label clean controls and engine limitations explicitly.
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
- **Package ownership and anti-name-sitting:** [spec 0045](docs/specs/ecosystem/0045-OrganizationPackageGovernance.md) requires organization-only `@org/package` identities, one package administrator and multiple explicit permission holders; leaf names are reusable across orgs. [DataProvider migration](packages/manager/supabase/migrations/schema.yaml) and [typeDiagram](schemas/package-registry.td) are canonical. `./target/release/osprey packages/manager/fixture-check --run` expects five passing audit groups; `./target/release/osprey test packages/manager/tests --filter "schema"` exercises model/parity fixtures; `./target/release/osprey test packages/manager/tests --filter "fixtures: names"` accepts duplicate leaf names across orgs and rejects unscoped names. Restore the pinned migration tool with `dotnet tool restore`; after provisioning the private `registry` schema, run `dotnet tool run DataProviderMigrate migrate --schema packages/manager/supabase/migrations/schema.yaml --provider postgres --output "$REGISTRY_MIGRATION_CONNECTION"`. Use a passfile/provider secret mechanism, never a password in that logged argument. Expected final result is eleven FORCE-RLS/default-deny tables and zero operations on rerun; currently **blocked** by DataProvider 0.9.11-beta inspecting only `public` during custom-schema verification. Retain that failure; never skip verification or open runtime policies to hide it.
- **Makefile** from the repo root: `make ci` (lint + test + build), `make test`, `make build`, `make fmt`, `make run FILE=<path>`. The compiler binary lands at `target/release/osprey`.
- **Package licenses:** `node scripts/verify-package-licenses.mjs` verifies the 18 package bundles against [`packages/license-audit.json`](packages/license-audit.json), preserving the NIMBLESITE PTY LTD, Australia MIT grant alongside upstream terms. `node --test scripts/test-package-licenses.mjs` expects five passing guard regressions. `target/release/osprey test packages/manager/cli/tests --filter "publish: licenses survive publication"` and `target/release/osprey test packages/manager/cli/tests --filter "fetch: licenses and attribution are preserved"` run the canonical publication/install regressions in [`packages/manager/cli/tests/src/publish_tests.osp`](packages/manager/cli/tests/src/publish_tests.osp) and [`packages/manager/cli/tests/src/fetch_tests.osp`](packages/manager/cli/tests/src/fetch_tests.osp); expect all selected tests to pass, including rejection of altered attribution. `publish --folder` includes root legal files and the complete `LICENSES/` directory automatically. See [`packages/README.md`](packages/README.md).
- **Package coverage:** `make package-coverage` collects, then individually gates all 18 packages against `projects.packages` in [`coverage-thresholds.json`](coverage-thresholds.json) — a 90% floor per package, no averaging. Collection reruns each [`packages/<name>/tests`](packages/) suite with `osprey test --coverage-json` and writes `target/coverage/packages/<name>.json` plus a source fingerprint, suite log and `summary.json`; every package at or above its threshold prints `OK`. `make _test_packages` collects and checks; `make _coverage_check_packages` checks the collected reports without re-running suites; both honour `BIN` and `COVERAGE_THRESHOLDS_FILE`. `make test` and CI run both: collection in the package source coverage job (uploaded as `coverage-packages`, `if-no-files-found: error`), enforcement in the coverage job. Focused single-package run: `python3 scripts/package_coverage.py --collect --package testing`.
- **Record measured package coverage:** `python3 scripts/package_coverage.py --record-measurements` verifies existing reports and writes their actual counts into `package_measurements` in [`coverage-thresholds.json`](coverage-thresholds.json), separately from `projects.packages` minimums. Add `--collect --compiler target/ci/osprey` to regenerate reports first. Recording requires the full inventory and cannot combine with `--package`. Expected result: verified counts, `null` for unavailable or stale evidence, unchanged thresholds, and a nonzero exit if a gate or snapshot write fails. Canonical recorder regressions are in [`scripts/test-package-coverage.py`](scripts/test-package-coverage.py); run `python3 scripts/test-package-coverage.py`.
- **Hunt — verified crash demo:** `./target/release/osprey hunt tests/hunt/crash.osp --input-bytes 1 --max-time 10`. This deliberately planted crash calls C `abort()` only for input `"9"`. Hunt discovers that input and reports `abort crash.osp:4`, `reproduced (SIGABRT)`. Source: [`tests/hunt/crash.osp`](tests/hunt/crash.osp); expected finding: [`tests/hunt/crash.expected`](tests/hunt/crash.expected). This is the first example to give anyone asking to see bug detection. HTML and JSON are saved under `target/hunt/<program-and-run-id>/`; their paths are printed. `--out target/hunt-demo` keeps both reports and raw engine artifacts in a predictable directory. `--no-html` / `--no-json` disable saved files independently.
- **Hunt — controls and limitations:** [`tests/hunt/clean.osp`](tests/hunt/clean.osp) and the fixed [`tests/hunt/record_update_list.osp`](tests/hunt/record_update_list.osp) expect no findings. [`tests/hunt/process.osp`](tests/hunt/process.osp) demonstrates an unsupported engine operation, not bug detection; never offer it as the bug-finding demo. Hunt needs KLEE and `compiler/bin/libosprey_runtime_klee.bc`; `make hunt FILE=tests/hunt/crash.osp HUNT_ARGS="--input-bytes 1 --max-time 10"` builds prerequisites before running.
- **Hunt — exhaustive qualification and crash proof:** `python3 scripts/hunt_sample.py --out target/my-hunt-sample` inventories runnable apps and hard-kills each invocation after ten seconds. Review `index.html` and `quick-corpus.txt`; only fully completed, finding-free runs qualify. `make hunt-crashes` asserts fifteen deliberately planted input cases through real KLEE and native replay; CI requires every expected class, source location, input and signal. Four explored paths with three normal exits and one detected crash is successful detection, not a missed test.
- **Python application ports:** [`examples/python/ports/README.md`](examples/python/ports/README.md) documents the micrograd classifier, FAISS vector search and Google ROUGE text evaluation workflows. `make python-app-ports` builds and checks all three under default, GC and ARC memory; expect nine `PASS` lines, exact goldens and zero live objects under ARC. Focused command: `python3 scripts/test-python-app-ports.py --app classifier --memory default`; runner guards: `python3 scripts/test-python-app-ports-contracts.py` (five passing tests).
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
- The target HTTP/WebSocket contract is `Result<T, Error>`, but legacy handle and status builtins still return `int` with negative error codes. `httpGetResponse`, `httpResponseBody`, `httpResponseHeader` and `httpResponseFree` return `Result`; `httpResponseStatus` returns `int`. Follow the actual builtin signature and check its failure form; do not claim uniform Result coverage.
- Fibers are isolated — message passing, no shared memory.
- Effects provide capability-based security; file, HTTP and process sandboxing is configurable.
