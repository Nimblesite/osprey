# Why PR #436 keeps failing CI

PR #436 merges `fixes2` into `main`. Five CI runs have gone red or been cancelled since 01:43 UTC on 27 Nov 2026. Each red run had a different set of failures, because commits kept landing on the branch faster than CI could check them.

## Root causes

1. **No one could run `make ci` locally.** The coordinated push rule ([CI-PUSH-LOCK]) needs a clean checkout. Six or more agents share one working tree, and 30–125 files were always uncommitted. `claude-pr436-ci` pushed `ce845c359` with no local `make ci`, so hosted CI became the first full check of the branch.
2. **The push lock was broken twice.**
   - `1edfd5597` committed another agent's staged rename and pushed a half-finished tree while `PUSH-fixes2` was held.
   - `claude-lint-levels` pushed `00a33ddc1` while `claude-pr436-ci` held the lock, which cancelled run 45934521288.
   - Each push without the lock cancels the run in progress. That throws away about 30 minutes of results and loses the evidence of what was failing.
3. **Whole-index commits picked up other agents' half-done work.**
   - `git commit -a` / `add -A` style commits (`e8197a51c Fixes`, `27410fb72 Fixes`, `1edfd5597`) shipped other agents' uncommitted, unchecked work.
   - That work included 305 `@@TODO@@` placeholder outputs and 49 synth doc examples that did not compile.
4. **Lint sweeps changed what programs do without running the suites that check it.**
   - `27410fb72`, and later `09da547e0`, turned repeated string literals into top-level `let`s in corpus programs.
   - The ownership checker refuses returning or passing a top-level string. This made 5 programs × 2 flavours fail under ownership on native, macOS and Android, twice.
   - The sweeps were only checked under GC.
5. **A rule changed without its tests.** `04f101a2b` made `unexampled-export` an error (deny) in packages. It did not update `warnings.rs`'s level test or `live_profiler_lsp`'s editor test.
6. **Mistakes by `claude-pr436-ci`.**
   - `dba6dfe78` (#370) used `signalflow::App` from a file that the `daw-render` project's roots don't include, so `daw-render` stopped compiling.
   - The fix for that put a `kindOf` call in `native/src`. That folder is compiled whole into wasm32 web apps, and wasm32 refuses directory builtins, so the Talon web build failed.
   - The #402 positions changed compiler error text that a blog post's example quotes; another agent updated the post.
   - A stale warning table in `project_tests.rs` tripped clippy's `too_many_lines`.
7. **A golden depended on the platform.** The math Chebyshev example printed a raw `cos (2 acos 0.5)`. macOS libm gives `-0.49999999999999983` and Linux gives `-0.5000000000000002`.
8. **Slow feedback.** A full CI run takes 45–60 minutes. With ~40 commits an hour landing, each run tests a tree that is already out of date.

## Status

Every failure in runs 45929780472 and 45936341867 has a fix committed locally and checked under the same mode that failed:

| Failure | Fix | Local evidence |
|---|---|---|
| Synth doc examples (354) | `a3adddc8e`, `a4e9d64af` | 582/582 under GC and ARC |
| Ownership corpus (5 programs) | `99d9557a5`, `938c8c001` | `make _conformance-ownership`: 0 failures |
| Lint-level Rust test | `fd543e4c4` | 13/13 warnings tests pass |
| Chebyshev golden | `4d910c82a` | 17/17 math doctests under GC and ARC |
| macOS and Android jobs | the same corpus fixes | — |
| Website `/docs/releases/` timeout | not a code fault: 7/7 pass locally | flaky page-settle bound, ci-reliability item 44 |

**Next:** push these fixes as one batch after the 5 Apple and proof-kernel jobs that are still running finish, then merge once the root `CI` check is green.

## Rules from now on

- Only the holder of `PUSH-fixes2` pushes. Everyone else commits with `git commit -- <own paths>` and sends the holder the SHA.
- Never `add -A`, `add .` or `commit -a`. Check `git diff --cached --stat` before every commit.
- A lint sweep runs the gate it could affect before committing: `make _conformance-ownership`, the package's `--doctests`, and `make _check_programs`.
- A rule change updates its tests in the same commit.
- No placeholder outputs. Goldens print values that are the same on every platform.
