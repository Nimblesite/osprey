# macOS Local App Sweep

Run on 2026-11-06 at `79c803505`, on an Apple Silicon Mac (Darwin 25.6.0). It
covers the macOS app suites that Linux CI exercises only in part. The VS Code
extension tests were left out because another session owns `vscode-test`. The
windowed variants (`<app>` without `-headless`) were left out so the run did
not take over the screen. `agent-live` was left out because it spends DeepSeek
tokens.

## Results

| Target | Result | Time |
| --- | --- | --- |
| `make -C src/toolchain/targets macos-test` | pass: corpus goldens 259 pass, 0 fail, 0 missing (floor 228); storage tests 7/7; storage smoke OK; GPUI host ARC ownership (2000 exports) | 91 s |
| `make -C src/toolchain/targets/macos/tests casting-headless` | pass (4 steps) | 33 s |
| `… flock-headless` | pass (11 steps) | 30 s |
| `… painting-headless` | pass (7 steps) | 26 s |
| `… probe-headless` | pass (12 steps) | 23 s |
| `… signal-flow-headless` | pass (16 steps) | 23 s |
| `… synth-headless` | pass (smoke, 1 step) | 20 s |
| `… storage-headless` | pass (`OSPREY_MACOS_STORAGE_SMOKE_OK`, 0 live objects at exit) | 12 s |
| `… agent-headless` | pass: Swift host XCTest suites 16 + 19 + 46 + 46 + 5 + 6 tests with 0 failures; self-test 50 steps; MCP self-test 10 steps; `verify_mcp` 14 requests | 79 s |
| `make _gpui-self-test` (every GPUI app's `self-test.json`) | pass: probe 12, flock 11, painting 7, signal-flow 16, casting 4 steps | 67 s |

**No failures.** Every target exited 0. GPUI dependency source integrity
reported 18 backports PASS on each run.

## Observations (not failures)

- **63 named macOS corpus skips**, all with the same reason: "a suspended
  continuation cannot cross or outlive a synchronous host call". These are the
  resumable-effect programs under `src/osprey/tests/effects/{multiplicity,resume,scope}`
  and `src/osprey/tests/modules/{ml_layout_depth,module_effects_rows,signature_ascription}`,
  in both flavors. They are pinned limitations of the host-call boundary, not
  regressions. Each one is still an untested path on macOS.
- **`undocumented-export` warnings** in app code: 9 in
  `src/osprey/examples/casting` (`App::*`, `Link::skinned`) and 14 in
  `src/osprey/examples/macos` (`Native::*` and the `Phone` and `View` modules).
  They are warnings only and break nothing.

## Not covered

- The VS Code extension native editor focus failure on macOS (3/3) is owned by
  OspreyCodex and was not run here.
- The windowed GPUI runs and `agent-live`, for the reasons given above.
