---
name: release-review
description: Adversarial review of the current branch (or main) against the last Osprey release, hunting regressions, memory-safety holes and weakened tests. Files each confirmed released bug as a GitHub issue. Use when the user says "release review", "adversarial review", "regression review", "review against the release", or before cutting a release.
---

# Release Review

Find what this branch breaks that the last release got right. You own every verdict.

## Scope

- Release: `gh release view --json tagName -q .tagName` (else `git describe --tags --abbrev=0`)
- Diff: `git fetch --tags && git diff <tag>...HEAD --stat`; read every changed file in full, old beside new (`git show <tag>:<path>`)
- Rank by blast radius: C runtime (`src/toolchain/compiler/runtime/`), codegen and the wasm32 backend, type inference, parser and grammar, CLI, stdlib and `src/osprey/packages/`, LSP and the VS Code extension, CI and release workflows

## Hunt

- 🔒 Memory safety first ([AGENTS.md](../../../AGENTS.md)): bounds, lifetimes, GC vs ARC ownership, fibers, FFI and WASI capabilities, untrusted input in HTTP/WebSocket
- Every removed guard, branch, fallback, error path or default is a suspect
- Osprey must never panic, abort or exit ([FAILURE-EXPLICIT]); fake zero/empty values for errors are findings
- [COMPILER-RED-FAILS]: a program with errors must never build, emit or run
- Same program, different output under GC, ARC or `OSPREY_TARGET=wasm32` is a finding
- Trace each change to its callers: blocking work on the LSP thread, missing timeouts, unbounded parallelism, stale caches, Windows paths
- Check wiring end to end: Makefile target → CI job → required `CI` check

## Test weakening

Each of these is a finding, never a judgment call:

- Deleted or skipped tests, dropped assertions, fewer input permutations, loosened goldens (`.expectedoutput`)
- `coverage-thresholds.json` lowered, CI timeouts raised, gates or job `if:` relaxed, `failscompilation/` programs removed
- Tests that re-implement the code they test, or assert less than their name claims

Hunt them with `git diff <tag>...HEAD --diff-filter=D --stat` and a grep of removed `assert`/`expect` lines.

## Legwork

- Up to two `pi` sessions (`zai/glm-5.4` or `kimi-coding/k3.1-pro`, per [AGENTS.md](../../../AGENTS.md)), each one bounded slice: a file set, the diff range, concrete questions; register each on TMC
- Kill a stalled pi; never wait on it. Its output is leads: confirm every claim in code yourself
- ❌ subagents, ❌ linked worktrees

## Tests

- A review, not CI prep: ❌ `make test`, `make ci`, full corpus or VS Code suites, ❌ triggering GitHub Actions
- Run one targeted test only to confirm a suspect: `cargo test --manifest-path src/toolchain/Cargo.toml --test <file> <name>`
- Prove a bug with a new failing test when cheap; never weaken an existing one

## Verdict

- Confirmed = you traced the mechanism and can state input → wrong output; drop anything unconfirmed
- Label each: regression vs the release, or a new bug on the branch

## Report

- Bug in the released tag → `gh issue list --search` for duplicates, then `gh issue create --label bug` (add `critical` for memory safety): title, tag, file:line, repro, expected vs actual
- Branch-only bug → failing test or message to the branch owner on TMC, no issue
- Reply with confirmed findings, severity first, and exactly what you did and did not review
