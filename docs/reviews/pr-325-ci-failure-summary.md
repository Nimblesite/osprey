# PR #325: CI failure summary

Log review: 31 October 2026. Scope: every workflow run since this PR opened, with job/step records for all 62 main CI runs and downloaded non-Windows failure logs. Windows remains with its assigned owner.

**At the audit snapshot: 123 workflow runs; main CI had 49 cancellations, 12 failures, one running and zero successful runs.** Cancelled runs sometimes contained genuine failures before cancellation; counts below therefore overlap.

| Failure area | What the logs show | Evidence |
| --- | --- | --- |
| Protection/configuration | Early runs stopped because required-check expectations disagreed with the repository ruleset. | [40951430304](https://github.com/Nimblesite/osprey/actions/runs/40951430304), [40963866686](https://github.com/Nimblesite/osprey/actions/runs/40963866686) |
| Packaging/build | Two successive VSIX builds lacked bundled `osprey-ui/wasi.js`; another could not compile Workbench WASM because `Nesting::lower` was unknown. Other heads failed strict lint. | [40981257660](https://github.com/Nimblesite/osprey/actions/runs/40981257660), [40983111344](https://github.com/Nimblesite/osprey/actions/runs/40983111344), [40999742311](https://github.com/Nimblesite/osprey/actions/runs/40999742311) |
| Workbench coverage | Package collection failed on six heads: 19 projects passed, Workbench collection failed and reported `0/0`, not a measured low percentage. | [40986387812](https://github.com/Nimblesite/osprey/actions/runs/40986387812), [41079268884](https://github.com/Nimblesite/osprey/actions/runs/41079268884) |
| Extension/browser | Extension coverage failed on 12 heads: repeated Workbench layout/capability mismatches, later a lesson Open-button timeout; the installed suite subsequently failed its expected module ordering. | [41016854807](https://github.com/Nimblesite/osprey/actions/runs/41016854807), [41135572472](https://github.com/Nimblesite/osprey/actions/runs/41135572472), [41154533373](https://github.com/Nimblesite/osprey/actions/runs/41154533373) |
| Compiler/test execution | Rust tests had real assertion/warning-hygiene failures; later Rust/package jobs exhausted approximately 40-minute budgets and proof checks approximately 45 minutes. | [41046357924](https://github.com/Nimblesite/osprey/actions/runs/41046357924), [41154533373](https://github.com/Nimblesite/osprey/actions/runs/41154533373) |
| Website | Missing proof-video elements, then broken specification links/anchors on a later head. | [40980023928](https://github.com/Nimblesite/osprey/actions/runs/40980023928), [41144068677](https://github.com/Nimblesite/osprey/actions/runs/41144068677) |
| Native macOS | Latest completed run passed the producer jobs, then failed three of 114 native tests: modal focus, picker width **304 > 300**, and text ink **1223.95 vs 1461.52 ±75**. | [41162597660](https://github.com/Nimblesite/osprey/actions/runs/41162597660) |
| Downstream fallout | Missing `coverage-rust` artifacts and aggregate `CI` failures followed failed/cancelled producers; these are consequences, not additional root causes. | [41154533373](https://github.com/Nimblesite/osprey/actions/runs/41154533373) |

**What prevented convergence:** pushes repeatedly superseded unfinished runs (`cancel-in-progress: true`); 25 of 61 successive run intervals were under 15 minutes. Shared-source changes also invalidated earlier local evidence, and local passes did not establish the macOS 15 result. Integration control failed to enforce one fully verified candidate before pushing.

**Why it exceeds an hour:** the last completed run took **86 minutes**: build finished at minute 14, Rust coverage at minute 43, then the coverage job performed Apple builds/tests for 28 minutes and native build/rendering checks for another 14. Native failures appeared at the end of that serial path. The replacement [41170562511](https://github.com/Nimblesite/osprey/actions/runs/41170562511) was still running at the audit snapshot; it proves no final green result yet.
