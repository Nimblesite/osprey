# Spec enforcement and coverage review

Date: 25 October 2026. Verdict: **CHANGES REQUIRED — closure is not proved.**

Base: `40115ff09310c7f688638999351ad1845b8795c6`. Reviewed snapshot: `c6a12df3c9134d3c76f77d19da7a6450fe595e48` on `codex/private-history`: 1,140 changed files, 111,461 additions and 11,709 deletions. Other agents continued editing during this review; later commits and uncommitted work are excluded. Source links below are pinned to the reviewed commit.

This replaces the previous closure diary with a fresh review of the diff. The review is static: no builds, test suites, CI jobs or live ruleset checks were started for it. Earlier execution artifacts are identified separately. The review concentrates on compiler semantics and ownership, new Hunt code, package enforcement, coverage/oracle guards, CI wiring and completion claims. It is not an assertion that every changed line or every spec has been exhaustively verified.

External pi reviewers used `zai/glm-5.3` for compiler changes and `deepseek/deepseek-v4-pro` for package/CI checks. They received separate lanes and candidate leads. The parent verified accepted findings against the pinned source, corrected inaccurate path calculations and line references, and excluded an unproved factory-selector concern. Their reports are legwork, not closure approval.

## Actionable findings

### [P1] Split Python version floors with integer division

Location: [python_plane.osp:133–135](https://github.com/Nimblesite/osprey/blob/c6a12df3c9134d3c76f77d19da7a6450fe595e48/packages/manager/cli/lib/python_plane.osp#L133).

`versionText` uses `${floor / 10000}.${(floor / 100) % 100}` to decode an integer such as `31200`. Osprey `/` returns a float, so this produces `3.12.12.0`, not `3.12`. This changes the generated interpreter requirement and project digest; a legitimately pinned `>=3.12` workspace then fails verification before uv runs. The same error affects nonzero patch floors. Use integer quotients and retain exact output/digest assertions for zero and nonzero patches.

The existing full manager CLI transcript contains this exact wrong output and failures in `python: lock composes and pins the uv project`, `python: verify a pinned plane`, and `python: verify findings`. Plan 0020's checked uv-integration item is therefore not sufficient evidence that the current implementation works.

### [P1] Preserve libm special cases in Hunt's power model

Location: [hunt_math.c:188–202](https://github.com/Nimblesite/osprey/blob/c6a12df3c9134d3c76f77d19da7a6450fe595e48/compiler/runtime/klee/hunt_math.c#L188).

The integer-exponent path excludes `abs(exponent) >= 2^53`, after which every negative base returns NaN. Consequently the model returns NaN for `pow(-1.0, 9007199254740992.0)`, whose result is exactly `1.0`; it also mishandles negative-infinity power cases. This is a change in branch semantics, not a small rounding difference. A crash reachable through the correct result can disappear from Hunt's explored program, and native replay cannot recover an input that was never reported.

The mock's power tests cover only a positive base. Add host/model parity assertions at the integer-representability boundary and over signed zeros, infinities and NaNs, and preserve unsupported behavior as an explicit model limitation until it is implemented correctly. The spec's “within a few ulp” claim is not established by the current tests.

### [P1] Do not recursively delete an arbitrary directory because it contains `info`

Location: [hunt/mod.rs:241–253](https://github.com/Nimblesite/osprey/blob/c6a12df3c9134d3c76f77d19da7a6450fe595e48/crates/osprey-cli/src/hunt/mod.rs#L241).

`clear_run_directory` considers any nonempty directory with an entry named `info` to be disposable KLEE output. `osprey hunt file.osp --out existing-directory` therefore deletes every file in an unrelated directory containing an ordinary `info` file or directory. Nothing verifies ownership or even that `info` is a KLEE report. The deletion happens before the new run.

Use an owned run directory or refuse to clear an unverified existing destination. The current test adds `info` to a directory containing `precious.txt` and then expects the whole directory to disappear; it does not prove the advertised protection. Add a preservation regression for unrelated contents plus an arbitrary `info` entry.

### [P1] Register comprehension bindings before lowering builtin aliases

Location: [comprehensions.rs:12–16](https://github.com/Nimblesite/osprey/blob/c6a12df3c9134d3c76f77d19da7a6450fe595e48/crates/osprey-syntax/src/python/comprehensions.rs#L12), [comprehensions.rs:144–148](https://github.com/Nimblesite/osprey/blob/c6a12df3c9134d3c76f77d19da7a6450fe595e48/crates/osprey-syntax/src/python/comprehensions.rs#L144).

The comprehension body is lowered before its targets are registered, and `for_in_clause` never calls `bind`. `call_alias` consequently treats a comprehension-local `len` as the builtin and rewrites it to `length`; an unregistered local `range` can also receive the builtin's inserted zero argument. For example, `[len(xs) for len in callbacks]` should call each callback, but its lowered body calls `length(xs)` instead. The missing binding check also bypasses the frontend's reserved lowering-target checks.

Bind each target in its proper lexical scope before lowering the clauses/body that can reference it, retaining outer-scope evaluation of the iterator expression. Add AST and behavioral regressions for shadowed `len`/`range` and nested clauses; the current tests use ordinary variable names and miss this case.

### [P2] Implement alternate octal formatting as precision, not an extra prefix

Location: [hunt_format.c:175–201](https://github.com/Nimblesite/osprey/blob/c6a12df3c9134d3c76f77d19da7a6450fe595e48/compiler/runtime/klee/hunt_format.c#L175).

`emit_unsigned` always adds an extra `0` prefix for nonzero alternate octal, independently of precision, and adds none for zero. Thus `%#.3o` of `1` produces `0001` instead of `001`, and `%#.0o` of `0` produces an empty string instead of `0`. Return counts, width padding and truncation inherit the error. These are defined formatting cases covered by the new mock's advertised printf contract.

A prior private pi differential run identified these mismatches. Parent source review confirms both control paths. That oracle labels thousands of known divergences as “pinned”; its reported “PASS” is not complete formatting parity and must not be used as closure proof. Add exact assertions for these cases and their widths/truncation without exempting mismatches. Undefined `#` with `%u` is excluded from this finding.

### [P2] Include exported Default functions in the coverage inventory

Location: [package_coverage.py:57](https://github.com/Nimblesite/osprey/blob/c6a12df3c9134d3c76f77d19da7a6450fe595e48/scripts/package_coverage.py#L57), [package_coverage.py:162–189](https://github.com/Nimblesite/osprey/blob/c6a12df3c9134d3c76f77d19da7a6450fe595e48/scripts/package_coverage.py#L162).

`DEFAULT_DEF_RE` matches only lines starting with `fn`, excluding valid `export fn` declarations. A Default source file containing only exported functions is consequently absent from `required_source_files`. If that entire file disappears from a report, another covered file can still satisfy the package threshold: the supposed completeness guard never demands the missing file. The source fingerprint binds files to the report but does not prove that the report contains their counters.

Extend discovery to exported declarations and add a missing-file regression using a real module with `export fn`. The existing Default fixture uses an unexported `fn entry`, so it cannot detect the omission. This is a gap in the newly introduced enforcement logic, not evidence that all current package reports are wrong.

## Existing deployment paths missed by the new audit gate

**[P1] Incomplete gate coverage.** [release.yml:38](https://github.com/Nimblesite/osprey/blob/c6a12df3c9134d3c76f77d19da7a6450fe595e48/.github/workflows/release.yml#L38) adds an unconditional audit and correctly makes its publishing/deployment callers depend on it. However, [deploy-pages.yml:13–25](https://github.com/Nimblesite/osprey/blob/c6a12df3c9134d3c76f77d19da7a6450fe595e48/.github/workflows/deploy-pages.yml#L13) and [deploy-webcompiler.yml:7–10](https://github.com/Nimblesite/osprey/blob/c6a12df3c9134d3c76f77d19da7a6450fe595e48/.github/workflows/deploy-webcompiler.yml#L7) retain independent `workflow_dispatch` entry points whose deployment jobs have no audit dependency. A manual deployment therefore bypasses the new mandatory fresh-audit gate.

These manual entry points predate the base; the finding is an incomplete implementation of the newly added universal release/deployment gate, not a newly introduced manual-deploy route. Enforce the audit inside each directly invokable workflow, or remove the unaudited route. Extend wiring tests beyond the job names in `release.yml`. No live GitHub configuration was inspected.

## Tests and completion claims that remain open

| Area | Source and evidence | Review conclusion |
|---|---|---|
| Hunt default completion | CI explicitly runs `make _test_hunt` and `scripts/test-hunt-default-budget.py` at 64 bytes/30 seconds. Retained `target/hunt-profiler-2026-10-25/current-red.log` reports `ValueError: incomplete exploration` for `clean`. | An actual required gate is red. Preserve its width, budget, completeness requirement and assertions. The checker fix and planted-crash successes do not close it. |
| Hunt file replay | [ktest.rs:118](https://github.com/Nimblesite/osprey/blob/c6a12df3c9134d3c76f77d19da7a6450fe595e48/crates/osprey-cli/src/hunt/ktest.rs#L118) panics for real files named `input.len`, `input.ok`, `entropy.data`, `clock.data`, or `errno.data`. `real_files_with_metadata_like_names_keep_their_replay_bytes` requires successful decoding and necessarily reaches that panic. | This is an explicit broken-code quarantine with an unresolved regression, not a completed decoder. Keep the desired test and the quarantine visible until an unambiguous decoder is proved. |
| Hunt delivery checklist | `docs/plans/0034-symbolic-execution.md` keeps Phase 3 completion open, but `docs/specs/tooling/SymbolicExecution.md:300` checks main mode as delivered; line 304 calls a budget-limited corpus “green”. | Align the spec checklist with the plan and actual retained outcomes. No-findings runs with unfinished paths do not prove corpus completion. |
| Development-overlay build test | `overlay_tests.osp:28,117–124` reads `.osprey/build/osprey.toml`, although the command uses `--project /abs` and `Build::writeDerived` writes `/abs/.osprey/build/osprey.toml`. It also expects the old absolute roots after the manifest-relative-path correction. | The retained full CLI suite fails this case. Update the lookup and assert correct path resolution and the compiler invocation; do not remove the path assertions or blame this stale fixture alone for a production path failure. |
| Manager full suites | Prior frozen-compiler evidence proves core 341/341 under Default/GC/ARC. The full CLI transcript ends `tests=83 passed=79 failed=4 skipped=0`; three failures are explained by version-floor arithmetic above and one by the stale overlay fixture. | Full manager closure is not established. Core-only results cannot stand in for the CLI, API, fixture, manifest and end-to-end suites. `registry-test` remains explicitly outside ordinary CI by maintainer decision. |
| Package expansion | At this snapshot the goldens expect Re 6125, Safetensors 173 and Testing 41 assertions. The old review cited earlier proofs for Re 6121, Safetensors 97 and Testing 37. | Do not reuse those earlier totals as proof of the expanded suites. Current source/oracle/golden fingerprints and matching backend transcripts are needed to establish execution; this static review does not assert they passed. |
| Broader delivery | Plans 0020/0031/0032/0034/0035 explicitly retain hosted trust/deployment work, Python frontend/tooling and GPT-2 edge cases, training, typed Hunt harnesses/regression generation/Windows, and profiler heap/Windows backends as open or deferred. | These are delivery targets, not regressions merely because code is absent. No blanket completed-feature claim is justified. |

The manager execution artifact was started during earlier repair work, not by this review. Its frozen compiler SHA-256 is `f882cee7494fc47abad7fcf74d76ec6045c1524185ad933adfb672a7937fd366`; transcripts, source hashes and commands are retained under `/var/folders/v3/49bmt6f55b1csb6snk8fr1mw0000gn/T/osprey-review-fixes-69ntrug7/manager-final-matrix-2/`. Its binary is not a build of every later change in the reviewed snapshot.

## Test weakening and coverage assessment

- Normalizing the old/new threshold layouts preserves all 52 preexisting project entries and their numeric floors; the snapshot has 71 entries including 18 package gates. No threshold reduction was found. Package floors remain 90% individually, with no averaging.
- Sampled assertion refactors through `crates/testkit.rs` preserve required/forbidden substring polarity; the helper still asserts every supplied needle and has regressions for each failure direction. This is not a proof that every moved assertion in the entire diff is semantically equivalent.
- The package backend matrix still discovers all shipped manifests, executes Default/GC/ARC, compares exact goldens, and requires one zero-live ARC sentinel. New coverage collection is a separate required job and feeds the required coverage job. Missing/malformed/altered evidence and oracle failures remain rejected, subject to the exported-function inventory gap above.
- The Hunt mock math/formatting tests leave real semantic gaps despite large comparison totals. New tests must cover the boundary behavior above rather than treating aggregate counts or closed-symbol checks as proof of model accuracy.
- No CI job, timeout, assertion, threshold or required-check list was changed for this review. Static wiring inspection does not establish that the live ruleset requires every new check or that branch CI passes.

Keep this review open. Closure requires the source defects and stale assertions to be resolved, the existing red Hunt/quarantine cases to pass without narrowing their contracts, and fresh evidence for the exact sources being claimed complete. Independent pi conclusions are review input; the parent must verify them and cannot substitute a clean-sounding agent summary for proof.

## Repair evidence after the reviewed snapshot

25 October 2026: the findings above describe the pinned snapshot, not the subsequently repaired source. Evidence is retained under `/var/folders/v3/49bmt6f55b1csb6snk8fr1mw0000gn/T/osprey-review-fixes-69ntrug7/`.

- Hunt power and alternate-octal defects have regression-first repairs. The extended native tests failed with 42 failures before the fixes, then passed with 690,680 checks and zero failures. All 24 Hunt script tests and 15 sample-runner tests also passed. A separate host-libm oracle passed the same 30 power boundary cases; the symbolic runtime bitcode rebuilt successfully. See `hunt-math-format-red-final.log`, `hunt-math-format-green.log`, `hunt-pow-host-oracle.log` and `hunt-math-format-source-hashes.json`. Neither sampled finite results nor these totals establish universal libm precision; that unsupported claim has been removed.
- The subsequent read-only GLM math/format review proposed two invalid findings. Binary64 rounds `0x1p53 + 1` to `0x1p53`, so its proposed odd-exponent sign counterexample is not representable. The existing do/while digit counter returns one for zero, and `%#.0o` with zero is explicitly tested. Parent verification and host results are recorded in `pi-math-format-parent-corrections.md` and `.json`. That report is not independent closure approval.
- Comprehension target binding has parser regressions and the full syntax/grammar run passed 352 tests. The CLI regression `comprehension_aliases_execute_in_their_lexical_scope` additionally executes shadowed `len`/`range`, a filter, nested clauses, repeated target names and outer builtin use under both GC and ARC, asserting exact output `7|10|3|4|6`. It passed against the Cargo-built compiler; see `comprehension-behavior.log`. Its sources use the supported iterator surface, rather than arbitrary Python iterables.
- The full manager CLI run under GC was deliberately stopped after its source fingerprint changed (`packages/manager/cli/lib/tree.osp`, one of 102 tracked inputs). That old frozen-compiler run cannot prove current source; its logs and explicit non-result receipt remain under `manager-review-fixes-matrix/`. No passing or failing current-suite verdict is claimed, and no earlier or stopped Default run substitutes for current GC/ARC evidence.
- The version-floor component regression has fresh GC and ARC execution against 21 production modules linked into the retained fixture, covering zero and nonzero patch components. Both runs pass the exact one-test TAP tally; ARC reports exactly one zero-live-object sentinel. Before/after hashes of the production sources, test origin, fixture, compiler and linked runtime archives are unchanged. See `python-floor-proof/current-gc-receipt.json`, `current-arc-receipt.json` and their stdout/stderr files. This closes the component receipt gap identified by the independent pi audit, without claiming a full manager-suite result.

- A subsequent review pass found that `osprey test <file>` inherited the caller's directory, allowing standalone file suites to litter the checkout. `standalone_suites_keep_writes_out_of_the_callers_directory` failed because `artifact.txt` appeared beside the invoked source. File suites now use automatically cleaned temporary working directories, resolving source and cache paths first; project suites preserve their existing working directory. The regression passes for successful and deliberately failing suites under GC and ARC, and all seven existing runner integration tests pass. See `osprey-root-litter-red.log`, `osprey-root-litter-green.log` and `osprey-suite-runner-regressions.log`.
- The project coverage suite exposed two stale expectations after the authorized GC/ARC migration: a removed `default` selector and an expected third executable despite testing only GC and ARC. The source-change invalidation control now starts with ARC, then runs GC; the shared-object control still requires one IR file, one object and one executable per tested backend. All 12 project coverage tests pass. No source-change, cache-identity, coverage or ARC exit assertions were removed. See `osprey-project-coverage-regressions.log` and `osprey-project-coverage-green.log`.
- Both native grammar crates are published at Git revision `865f0469acaec12ba7e448491fc9ec2cc877d57c`. A fresh external Cargo consumer fetched that exact remote revision, built both crates with tree-sitter 0.25 and parsed both syntax flavors successfully without local path dependencies. The reported Hunt formatting defects were fixed by a DeepSeek pi worker and independently checked; the full workspace formatting check passes. These results remove Deslop's dependency delivery blocker; they do not prove full branch CI. See `osprey-published-grammar-check.log`, its retained consumer manifest/lockfile and `osprey-deslop-fmt-final.log`.

## Additional open finding: precise ownership metadata on WASM

**[P1] Partial WASM ARC delivery silently releases scalar values as pointers.** The in-progress implementation's `arc_drop_word` scanned both 32-bit halves of each marked 8-byte word. A real WASI reproducer stored a managed child in one half and an ordinary scalar equal to a different live object's address in the other. Dropping the parent incorrectly changed that unrelated object's reference count from 2 to 1. A registry lookup cannot distinguish an integer from an owned pointer merely because the integer matches an allocated address.

The retained failing source is now `compiler/runtime/memory_arc_ownership_wasm_tests.c`, run by `scripts/test-wasm-arc-ownership.sh` inside the existing `_test_wasm_goldens` gate. Before quarantine, the actual 32-bit run printed `expected=2 actual=1` and exited 1. In accordance with the repository's broken-code rule, the unsound 32-bit masked drop now aborts explicitly with `WASM ARC requires precise pointer layout metadata`; the regression remains red, and native 64-bit behavior is unchanged. Strict WASI compilation and native syntax checks pass. This is an open implementation requirement, not a completed fix or an acceptable release state. See `arc32-review/`, `osprey-arc32-persistent-red.log` and `osprey-arc32-gate-quarantine-red.log` in the evidence directory.

The review remains open pending this ownership requirement and the remaining suite, gate and independent closure evidence described above.

## Current ownership verification, 26 October 2026

A fresh `scripts/test-wasm-arc-ownership.sh` run compiled the current runtime with strict warnings and executed the real wasm32 regression under WASI. It passed: the unrelated scalar target retained count `2`, and the pointer-list children had counts `1,1,2`. The temporary build directory was removed automatically. The transcript is `target/logs/ci-prep-current/wasm-arc-ownership-current.log`. This supersedes the historical red/quarantine status above for these two ownership cases; it does not prove the complete WASM corpus or branch CI. The review remains open for the remaining regressions, gates and independent closure evidence.
