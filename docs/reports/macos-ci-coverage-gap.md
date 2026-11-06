# macOS CI coverage gap

Requested 2026-11-06 after the VS Code native-editor focus tests failed 3/3 on macOS and passed on hosted Linux
(`target/tmc/vsix-focus-brief.md`). This report lists every CI suite that runs only on Linux although its code ships on
macOS, and records a local run of each one on this Mac (Apple silicon, Darwin 25.6).

## What already runs on macOS in CI

| Workflow job | Runner | What it covers |
| --- | --- | --- |
| `ci.yml` Tests: Apple (iOS, macOS, Talon Bank, Workbench) | macos-15 | Mobile and macOS host goldens, Bank UI tests, Workbench Swift tests |
| `ci.yml` Tests: Osprey Agent UI | macos-26 | Agent host tests and headless screenshots |
| `gpui.yml` GPUI host (macos-latest) | macos-latest | Each desktop app's self-test in a real window, host clippy, Mac archive string ownership |
| `release.yml` build (darwin-arm64) | macos-15 | Builds and packages the release; it runs no test suite |

## Gaps: Linux-only suites whose code ships on macOS

The Brew formula ships `darwin-arm64` builds of the compiler, the C runtime and `osprey-pkg`, and the VSIX ships a
`darwin-arm64` build. Every suite below tests that code on Linux only.

| CI job (Linux only) | Code that ships on macOS | Local command | macOS result |
| --- | --- | --- | --- |
| VS Code extension | VSIX darwin-arm64: extension, LSP client, native editor UI | `make _test_vscode_extension` (CI wraps it in `xvfb-run`) | 🔴 879 pass, 2 fail: native-editor focus ("challenge first request", "modal boundary"). See the focus brief; OspreyCodex owns the fix |
| Zed manifest and stdio | Zed extension, LSP over stdio | `make -C src/toolchain/zed-extension stdio-test` | 🟢 OK |
| Rust workspace (rest, workbench, packages) | Compiler, LSP, formatter, debugger | `make _test_rust_shard RUST_SHARD=…` | 🟢 all three shards (pr343-local-ci.md) |
| C runtime | `libosprey` runtime archives in the Brew bottle | `make -j2 _test_c_runtime && make _test_hunt_mock` | 🟢 (pr343-local-ci.md) |
| Language corpus GC and ARC | Code generation and both memory backends | `make _conformance-gc`, `make _conformance-arc` | 🟢 GC 322/322, ARC pass (pr343-local-ci.md) |
| WebAssembly differential | wasm32 backend run from a Mac | `make _test_wasm_goldens` | 🟢 (pr343-local-ci.md) |
| WebAssembly example and Data Studio smoke | wasm32 backend, Node and browser WASI | the two `test-wasm` smoke steps | 🟢 hello, studio.osp and studio.ospml compile, validate and match under Node and the browser shim |
| Package source coverage | `src/osprey/packages/` | `make _check_packages && make _test_packages` | 🟢 28/28 (pr343-local-ci.md) |
| Package manager GC and ARC | `osprey-pkg` and the registry | `make _test_registry REGISTRY_MEMORY=gc` and `=arc` | 🟢 525 Osprey and 13 Python tests each (pr343-local-ci.md) |
| Python application ports | Python flavor front end | `make _test_python_app_ports` | 🟢 (pr343-local-ci.md) |
| Proof kernel and Lean | Proof exports | `make -j3 _test_proof_ci` | 🟢 (pr343-local-ci.md) |
| Integration | Bank, profiler, mobile tools, Agent suites, web compiler | `make _bank_test`, `_test_profiler`, `_test_agent`, … | 🟢 except the Android emulator, which was not run (pr343-local-ci.md) |
| GPUI suites (ubuntu-latest) | Native framework, UI test framework, GPUI examples | `osprey test` on the eight suite folders in `gpui.yml` | 🟢 8/8 suites |
| Website E2E and book | Site and book examples (built on any OS) | `npx playwright test`; `make -C docs/book check check-examples` | 🟢 327/327 Playwright; book structure and examples pass |

## Suites that cannot run on this Mac

- **Instruction-count gate** (`test-instructions`): needs valgrind, which runs only on Linux. It measures instruction
  ratios, not macOS behavior, so it is not a macOS gap.
- **Windows jobs** (`ci-windows.yml`, `winui.yml`, GPUI on Windows) need a Windows machine.

## Findings

1. **The VS Code extension suite is the one real macOS bug CI hides.** It is red on macOS and green on Linux xvfb,
   because xvfb has no real window focus. CI should also run `make _test_vscode_extension` on a macOS runner so this
   cannot hide again. The suite runs without xvfb on macOS.
2. Every other Linux-only suite is green on this Mac today, but none of them is gated on macOS. The Rust workspace, the
   C runtime and the GC/ARC corpus test the exact binaries the Brew bottle ships. A macOS leg for those three would
   catch platform-specific code generation and runtime bugs.
3. **Cost:** a macOS runner costs about ten times as many GitHub Actions minutes as Linux. Adding only the extension
   suite and a macOS corpus/C-runtime leg covers the shipped code without duplicating the whole Linux matrix.

This report changes no workflow. Adding the macOS legs is for whoever owns `ci.yml`, and the extension leg should
land together with the focus fix so the gate is green when it starts.
