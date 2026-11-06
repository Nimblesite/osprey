# PR #343: local CI gate before any push

User order (2026-11-06): nobody pushes PR #343 again until every CI check has passed locally.
Each row is one `ci.yml` job, run with the job's own command. A row is ticked only after a local
pass on the HEAD that will be pushed. Every push rule in AGENTS.md still applies.

| CI job | Local command | Owner | Result |
| --- | --- | --- | --- |
| Detect changed areas | `node --test scripts/ci/ci-scope.test.mjs`; `python3 src/toolchain/benchmarks/test_merge_results.py --tooling-only`; `python3 src/toolchain/benchmarks/test_instruction_gate.py` | check-once-claude | [x] check-once-claude 2026-11-06 |
| Format & Analyse | link-skills, node-deps, release gates, dead code, licenses, package coverage scripts, `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `npm run lint`, `npm run test:shipwright`, deslop, `make -C src/toolchain hawk` | check-once-claude | [x] check-once-claude 2026-11-06 (fmt, clippy, lint, shipwright, deslop, hawk) |
| Rust workspace (rest) | `make _test_rust_shard RUST_SHARD=rest` | check-once-claude | [x] check-once-claude 2026-11-06 (no-fail-fast, all binaries ok) |
| Rust workspace (workbench) | `make _test_rust_shard RUST_SHARD=workbench` | check-once-claude | [x] check-once-claude 2026-11-06 (3/3) |
| Rust workspace (packages) | `make _test_rust_shard RUST_SHARD=packages` | OspreyCodex | [x] GC/ARC oracle matrix passed in 219.82s; LCOV collected (`f4a096d2e`) |
| Package source coverage | `make _check_packages && make _test_packages` | OspreyCodex | [x] All 28 passed (`f4a096d2e`) |
| Package manager (gc, arc) | `make _test_registry REGISTRY_MEMORY=gc` and `=arc` | OspreyCodex | [x] 525 Osprey and 13 Python tests per backend (`f4a096d2e`) |
| Python application ports | `make _test_python_app_ports` | OspreyCodex | [x] pi deepseek-flash 2026-11-06, verified log |
| C runtime | `make -j2 _test_c_runtime && make _test_hunt_mock` | OspreyCodex | [x] pi deepseek-flash 2026-11-06, verified log |
| Language corpus (gc, arc) | `make _conformance-gc` and `make _conformance-arc` | pi deepseek-flash | [x] pi deepseek-flash: gc 322/322, arc pass, logs verified |
| WebAssembly target | `make _test_wasm_goldens` | pi deepseek-flash | [x] pi deepseek-flash: wasm32 corpus run_test_corpus.sh passed |
| Website E2E | `cd src/toolchain/website && npm test`; `make -C docs/book check check-examples` | pi deepseek-flash | [ ] |
| VS Code extension | `make _test_vscode_extension` | check-once-claude | [ ] local macOS: 879 pass, 2 fail (native-editor focus: challenge first request, modal boundary); hosted Linux xvfb green at a917c32dc. Investigate |
| Proof kernel and Lean | `make -j3 _test_proof_ci` plus the proof script tests | OspreyCodex | [x] GC/ARC/WASI corpora, 3 byte-identical exports, 70 script checks and both Lean speed cases (`f4a096d2e`) |
| Integration | `make _bank_test`, profiler, mobile tools, `make _test_agent`, bank Playwright, docs HTML, web compiler Docker | check-once-claude | [x] 2026-11-06: pi deepseek-flash (bank, profiler, mobile, agent 1876+45, docs HTML, bank Playwright 17/17), check-once-claude (mobile app 25/25, web compiler Docker); Android emulator not run |
| Apple (macOS, iOS, Workbench, Talon Bank) | each matrix `run` command from `ci.yml` | OspreyCodex | [x] 259 goldens each on macOS and iOS; 114 Workbench tests; 5 Bank UI tests (`f4a096d2e`) |
| Agent UI (macos-26) | `make -C src/toolchain/targets/macos/tests agent-headless` | OspreyCodex | [x] 46 Swift tests and all headless scenarios with the local Retina goldens (`f4a096d2e`) |
| Coverage thresholds | `make _coverage_check_rust _coverage_check_vscode_extension _coverage_check_packages _coverage_check_c_runtime` | check-once-claude | [ ] |

## Gaps that cannot run on this Mac

- Instruction-count ratios need valgrind on Linux. This file records the gap and does not claim a pass.
- CI (Windows), the WinUI host and GPUI on Windows need a Windows agent. They are ask-the-user items.
- The Android emulator part of Integration needs an Android SDK with KVM. It is run only if the SDK is present.
