---
name: ci-prep
description: Prepares the current branch for CI by running the exact same steps locally and fixing issues. If CI is already failing, fetches the GH Actions logs first to diagnose. Use before pushing, when CI is red, or when the user says "fix ci".
argument-hint: "[--failing] [optional job name to focus on]"
---
<!-- agent-pmo:74cf183 -->

# CI Prep

Prepare the current state for CI. If CI is already failing, fetch and analyze the logs first.

## Arguments

- `--failing` — Indicates a GitHub Actions run is already failing. When present, you MUST execute **Step 1** before doing anything else.
- Any other argument is treated as a job name to focus on (but all failures are still reported).

If `--failing` is NOT passed, skip directly to **Step 2**.

## Step 1 — Fetch failed CI logs (only when `--failing`)

You MUST do this before any other work.

```bash
BRANCH=$(git branch --show-current)
PR_JSON=$(gh pr list --head "$BRANCH" --state open --json number,title,url --limit 1)
```

If the JSON array is empty, **stop immediately**:
> No open PR found for branch `$BRANCH`. Create a PR first.

Otherwise fetch the logs:

```bash
PR_NUMBER=$(echo "$PR_JSON" | jq -r '.[0].number')
gh pr checks "$PR_NUMBER"
RUN_ID=$(gh run list --branch "$BRANCH" --limit 1 --json databaseId --jq '.[0].databaseId')
gh run view "$RUN_ID"
gh run view "$RUN_ID" --log-failed
```

Read **every line** of `--log-failed` output. For each failure note the exact file, line, and error message. If a job name argument was provided, prioritize that job but still report all failures.

## Step 2 — Analyze the CI workflow

1. Find ALL CI workflow files under `.github/workflows/` (e.g. `ci.yml`,
   `ci-windows.yml`) — not just one. Parse every job and every step in each.
2. Read each workflow file completely. Parse every job and every step.
3. The core `ci` job runs `make lint → make test → make build`, but it is NOT
   the whole story (see [MAKE-TARGETS] in REPO-STANDARDS-SPEC). At time of
   writing CI also runs, and you MUST run locally: the Shipwright manifest
   validation (`npm run test:shipwright` in `vscode-extension`), the example
   compile/run checks, the **Docker web-compiler test** (`webcompiler/` —
   `docker build` + run container + `./test.sh`), the separate `rust` job
   (`cargo fmt --all --check`, clippy, `cargo test --workspace`, the
   differential harness), and the `website` Playwright E2E suite (`npm test`
   in `website/`). Re-derive the actual list from the workflows every time —
   do not trust this list to be current.
4. Note any environment variables, matrix strategies, or conditional steps that
   affect execution — including platform-only jobs (e.g. `ci-windows.yml` runs
   on `windows-latest`; on macOS/Linux run the closest local equivalent and
   flag the gap).

**Do NOT assume the steps are correct.** Read the actual CI workflow to confirm. If extra targets beyond the 7 standard ones are found (e.g. `make fmt-check`, `make coverage-check`), flag them — they should be consolidated by the agent-pmo skill.

## Step 3 — Run each CI step locally, in order

Work through failures in this priority order:

1. **Formatting** — run auto-formatters first to clear noise
2. **Compilation errors** — must compile before lint/test
3. **Lint violations** — fix the code pattern
4. **Runtime / test failures** — fix source code to satisfy the test

For each command extracted from the CI workflow:

1. Run the command exactly as CI would run it.
2. If the step fails, **stop and fix the issues** before continuing to the next step.
3. After fixing, re-run the same step to confirm it passes.
4. Move to the next step only after the current one succeeds.

**Repo-specific context:**
- Everything runs from the repo root: `make lint` / `make test` / `make build`
  (cargo clippy + extension lint; cargo test + coverage thresholds +
  differential harness + extension tests; C runtime archives + cargo release
  build + extension).
- The compiler is the Rust workspace (`crates/`, binary `target/release/osprey`);
  the C runtime archives are built by the internal `make _runtime` helper.
- The differential harness is `zsh crates/run_test_corpus.sh [default|gc|arc]` —
  expect `TEST_CORPUS_FAIL=0` and `TEST_CORPUS_GOLDEN_FAIL=0
  TEST_CORPUS_GOLDEN_MISSING=0` at or above the golden floor the script prints
  (160 natively). `OSPREY_TARGET=wasm32 zsh crates/run_test_corpus.sh` runs the
  same corpus through the wasm32 backend under Node's WASI (103 compared, 57
  named skips). The must-reject ratchet moved into `cargo test` — it is the
  `every ill-formed program must be rejected` assertion in
  `crates/osprey-cli/tests/examples_compile.rs`.
- The Docker web-compiler test needs a running daemon. If `docker info` fails,
  start it (`open -a Docker` on macOS) and wait for readiness before running —
  do NOT treat an unavailable daemon as licence to skip the check.

### Hard constraints

- **NEVER modify test files** — fix the source code, not the tests
- **NEVER add suppressions** (`// nolint`, `// eslint-disable`)
- **NEVER use `any` in TypeScript** to silence type errors
- **NEVER delete or ignore failing tests**
- **NEVER remove assertions**

If stuck on the same failure after 5 attempts, ask the user for help.

## Step 4 — Report

- List every step that was run and its result (pass/fail/fixed).
- If any step could not be fixed, report what failed and why.
- Confirm whether the branch is ready to push.

## Step 5 — Commit/Push (only when `--failing`)

Once all CI steps pass locally:

1. Commit, but DO NOT MARK THE COMMIT WITH YOU AS AN AUTHOR!!! Only the user authors the commit!
2. Push
3. Monitor until completion or failure
4. Upon failure, go back to Step 1

## Rules

- **Always read the CI workflow first.** Never assume what commands CI runs.
- Do not push if any step fails (unless `--failing` and all steps now pass)
- Fix issues found in each step before moving to the next
- **Never skip steps or suppress errors.** RUN EVERY CHECK CI RUNS.
- **Run EVERY actual check locally — no exceptions for "heavy" ones.** This
  explicitly includes Docker / service-container tests (e.g. the web-compiler
  Docker test), end-to-end suites (Playwright), coverage-gated tests, and the
  format check. "It builds a container" / "it's slow" is NOT a reason to skip —
  start the daemon and run it. If a check genuinely cannot run on this host
  (e.g. a Windows-only job on macOS), say so explicitly in the report and run
  the closest local equivalent; do not silently drop it.
- The ONLY lines you may skip are pure CI plumbing that have no check semantics
  and no local equivalent: `actions/checkout`, toolchain/`setup-node` install
  actions, dependency cache restore/save, and artifact upload/download. Skipping
  these is fine because they assert nothing about the code — never extend this to
  a build/lint/test/validation step.

## Success criteria

- Every command that CI runs has been executed locally and passed
- All fixes are applied to the working tree
- The CI passes successfully (if you are correcting an existing failure)
