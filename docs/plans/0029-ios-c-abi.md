# Plan 0029 — iOS C ABI application logic and Swift host

**Subsystem:** CLI targets, LLVM code generation, effect checking, C runtime, and the SwiftUI example.

**Spec:** [0038-iOSTarget.md](../specs/0038-iOSTarget.md), with shared target restrictions in [0022-WebAssemblyTarget.md](../specs/0022-WebAssemblyTarget.md).

**Status:** Reviewed against `407e0c3d0a69cf3cc39583b09a5670bd8732fd0e`; confirmed regressions are fixed with tests. The Xcode component and debugger blockers are now resolved and their checks pass. Two truthfulness defects found while building the mobile golden differential are fixed with tests. What remains is a physical-iPhone smoke that needs the phone unlocked, and the hosted PR checks. Completed and pending gates are recorded below.

## Scope and architecture

Osprey supplies application logic through a synchronous C ABI. Swift owns the iOS application lifecycle, UI, and platform APIs. Swift calls generated Osprey exports; Osprey requests platform services through explicit C imports implemented by the Swift host.

This delivery covers `--target=ios` for ARM64 iPhones and `--target=ios-sim` for the ARM64 iOS Simulator. Android uses the same architectural boundary in [plan 0030](0030-reactive-mobile-apps.md) and [spec 0039](../specs/0039-AndroidTarget.md); that extension remains outside this plan's original iOS scope. The exact type mapping, ownership contract, deployment version, and unsupported options belong to the spec rather than a second definition here.

## Phase 1 — Define the host boundary

Implement `[IOS-HOST-ABI]`, `[IOS-HOST-IMPORTS]`, and `[IOS-TARGET-ENTRY]`:

- Derive stable C exports and headers from inferred scalar signatures, including qualified project names.
- Adapt Apple ARM64 boolean arguments/results and Osprey `Unit` to their C representations in both directions.
- Reject unsupported host import signatures, invalid C names, and symbol collisions with imports, exports, runtime declarations, and generated initialization symbols.
- Initialize once, preserve the result for repeated calls, reject reentrant initialization, and retain application globals for later host calls.
- Document synchronous calls, host serialization, borrowed input strings, returned string handling, and the default allocator's lifetime limits.

Implementation: [ios_abi.rs](../../crates/osprey-cli/src/ios_abi.rs), [ios_abi_header.rs](../../crates/osprey-cli/src/ios_abi_header.rs), and `compile_library` in [lower.rs](../../crates/osprey-codegen/src/lower.rs).

Evidence: [ios_abi_tests.rs](../../crates/osprey-cli/src/ios_abi_tests.rs), especially `imports_use_c_bool_attributes_and_adapt_unit_returns`, `generated_header_and_ir_compile_with_clang`, and `initialization_caches_status_and_rejects_reentrant_calls`; [ios_tests.rs](../../crates/osprey-cli/src/ios_tests.rs) pins application-global lifetime.

## Phase 2 — Reject unsupported target capabilities

Implement `[IOS-TARGET-CAPABILITIES]` and `[WASM-TARGET-CAPABILITIES]` before invoking LLVM or a linker:

- Reject explicit `resume` and resumable effect handling on the current iOS C ABI and WASM targets, including inside helpers and in both source flavors.
- Reject unavailable runtime operations with a diagnostic naming the target and offending operation. Distinguish real builtins from local bindings and declared host imports.
- Check exported function effect requirements independently of `main`: a host call cannot depend on a handler whose startup scope has ended.
- Apply target checks to `--check`, `--llvm`, and compilation; reject unsupported memory, debugger, profiler, and execution options explicitly.
- Preserve supported substituting handlers and effects eliminated before runtime. Validate WASM browser signatures and dispatcher effect requirements, and keep browser application globals alive after startup.

Implementation: [target_capabilities.rs](../../crates/osprey-cli/src/target_capabilities.rs), CLI dispatch, `check_program_exports` in [check.rs](../../crates/osprey-types/src/check.rs), and [wasm.rs](../../crates/osprey-cli/src/wasm.rs).

Evidence: [target_capabilities.rs integration tests](../../crates/osprey-cli/tests/target_capabilities.rs), `exports_cannot_depend_on_a_handler_installed_only_by_main`, and the WASM browser startup/dispatch tests.

Acceptance requires compiler failure for unsupported features. A successful Osprey build followed by an undefined runtime symbol is not an acceptable substitute. Missing explicitly declared host implementations remain errors at the final host application link, as specified by `[IOS-HOST-IMPORTS]`.

## Phase 3 — Build the iOS artifacts

Implement `[IOS-TARGET-TRIPLE]`, `[IOS-TARGET-OPTIONS]`, and `[IOS-TARGET-LINK]`:

- Select matching device/simulator SDKs, LLVM triples, and runtime archives.
- Compile application LLVM IR with Xcode's clang and combine its object with the matching runtime using `libtool -static`.
- Emit the requested `.a` and sibling `.h`, including project output defaults and useful toolchain/setup errors.
- Build hardened runtime slices incrementally. Exclude unsupported process APIs without leaving unresolved process wrappers in otherwise supported fiber code.

Implementation: [ios.rs](../../crates/osprey-cli/src/ios.rs), [ios.mk](../../scripts/ios.mk), [ios-runtime.sh](../../scripts/ios-runtime.sh), and the iOS conditional in [fiber_runtime.c](../../compiler/runtime/fiber_runtime.c).

Evidence: driver unit tests and the real device/simulator C host compilation and simulator execution in [test-ios.sh](../../scripts/test-ios.sh). `each_slice_names_itself_in_its_header_and_diagnostics` pins that a simulator build names `ios-sim` in its generated header and its errors; it previously told the reader to rebuild with `--target=ios`, which produces the device archive that cannot link into a simulator host.

## Phase 4 — Integrate the Swift application

Implement `[IOS-SWIFT-HOST]` using [examples/ios/](../../examples/ios/):

- Keep counter behavior and message formatting in Osprey; call them from SwiftUI controls.
- Import the generated C header and link the SDK-specific archive through the checked-in Xcode project.
- Implement a Swift platform callback with C linkage, copy its borrowed string, and call Foundation's logging API.
- Provide reproducible device/simulator builds, simulator launch, and assertions running inside the actual application.

Evidence: [SmokeCheck.swift](../../examples/ios/OspreyCounter/SmokeCheck.swift) covers initialization, integer bounds, boolean/string results, and the Swift callback. The [example README](../../examples/ios/README.md) documents build, signing, and launch steps.

## Phase 5 — Verify and record delivery

The implementation session verified device and simulator builds, C host links, the running SwiftUI simulator application, and the language goldens then in scope. It also passed the CLI, codegen, and type suites, target rejection tests, strict workspace Clippy, and formatting checks.

Physical-device execution is verified through the [Issue Inbox application](0030-reactive-mobile-apps.md): the signed app installed and launched on an iPhone 16, passed the full `OSPREY_INBOX_SMOKE_OK` workflow, and displayed eight actual GitHub issues with persisted bookmarks and no application error. Signing belongs to the native host deployment flow; the compiler still emits an unsigned library and the default device build does not assume signing credentials.

The WASM harness passed 142 supported-program goldens and 18 alternative GPU-lowering comparisons. Its same 61 excluded programs remain pinned in [WASM_UNPORTABLE.txt](../../tests/WASM_UNPORTABLE.txt), now with compiler capability reasons. Unexpected LLVM/linker failures are test failures, and changes to either the excluded file set or its reasons fail the harness.

Reproduce the checks from the repository root:

```sh
make ios-test
cargo test -p osprey-cli -p osprey-codegen -p osprey-types
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
make _runtime_wasm _test_wasm_goldens
make ci
```

The earlier full-CI attempt used a local Deslop `0.0.0-dev` binary and stopped at its 9.4% duplication report. The release review reran the gate with the official **0.27.0** version pinned in CI: **4.8%** (3,941 of 81,987 LOC), passing the unchanged **5%** ceiling. The development build result is superseded; no threshold or exclusion was relaxed. Memory backends, continuation support, teardown, and packaging still require their own scoped work and tests before changing the current rejection rules.

## Phase 6 — Hold the boundary with the whole corpus

Fourteen hand-picked Default/ML goldens were the entire runtime evidence for this target. That set cannot notice a boundary that truncates a string, loses a bool's high bits, or miscompiles arithmetic inside an archive — the same weakness the WASM harness was built to remove, and for the same reason.

Two layers now cover it, and both run on iOS and Android from one source:

- The shared C ABI fixture ([`mobile-abi.osp`](../../scripts/mobile-abi.osp), [`mobile-abi.c`](../../scripts/mobile-abi.c)) asserts what a header cannot state: both 64-bit integer extremes, checked arithmetic yielding its `?:` default, doubles round-tripping, C booleans passed through registers under both bool ABIs, UTF-8 crossing intact both ways, empty and 4096-byte strings, an Osprey-allocated string reaching a host import, and a borrowed host buffer being copied rather than aliased.
- `make _test_ios_goldens` and `make _test_android_goldens` run the WHOLE `tests/` corpus through the mobile C ABI — built as a library, linked into a C host, executed on a simulator or device — and hold each program to the byte-exact stdout the native backend produces. Both ride inside the existing `ios-test` and `android-test` targets, so the already-required CI jobs pick them up with no new gate.

Result: **130 byte-exact goldens and 18 alternate GPU-lowering comparisons on each platform**, up from 14. The 79 rejected programs are pinned by name and reason in the shared [`MOBILE_UNPORTABLE.txt`](../../tests/MOBILE_UNPORTABLE.txt); a new, removed or changed entry fails the harness, and an unexpected clang, linker, simulator or device failure is a failure rather than a skip. Removing one entry was verified to fail the run. The two platforms share one manifest because they share one boundary implementation; if they ever diverge, the comparison fails and the file must be split rather than widened.

Building this surfaced two diagnostics that named the wrong thing, both now fixed with tests: a simulator build reported itself as `ios`, and a continuation rejected on a phone cited a WebAssembly proposal. The hand-picked golden loops were deleted from both platform scripts rather than left to re-run a strict subset of what the harness already observes.

## Release review — 8 September 2026

Compared the branch with `407e0c3d0a69cf3cc39583b09a5670bd8732fd0e`, including mobile hosts, C ABI generation, runtime changes, target restrictions, shared compiler changes, and build/CI packaging.

- Fixed UTF-8 truncation in descriptions/search and byte-based note validation. Limits now count Unicode scalars, and regression tests verify complete envelopes and persisted notes at the limit. All 32 shared app cases passed natively and on Android ARM64; the suite is now included in both platform harnesses.
- Fixed incompatible runtime/host-import collisions that previously emitted invalid LLVM. All 21 ABI unit tests pass, including the new collision cases.
- Pinned Android's launcher to Gradle 8.7 while preserving the explicit `GRADLE_BIN` override and respecting `GRADLE_USER_HOME`. The reproduced wrong-version regression now passes.
- Fixed the web compiler Docker build by including the Makefile's required scripts. The rebuilt container passed the real API integration test.
- Fixed a WASM test race by sharing the environment lock with tests that substitute tool commands. The workspace tests pass. Added mobile build-driver tests for both Android architectures and iOS targets, SDK/NDK selection, scratch cleanup, and failed builds preserving existing artifacts. CLI coverage rose from 92.2% to **95.3%**; every existing Rust coverage gate passes.
- Added Android emulator/domain/lint validation to the existing required integration job and iOS device/simulator validation to the existing required coverage job. Workflow lint and the live branch-protection check pass; hosted execution of the edited workflow is still pending.
- Fresh independent checks passed: Android APKs for ARM64/x86-64, ARM64 C ABI and 14 language goldens, application deterministic/live GitHub fresh/restart smoke, Android lint, all C runtime coverage gates, 209 language goldens in each of default/GC/ARC (zero ARC leaks), 144 WASM goldens plus 18 alternate GPU comparisons, and 107 website tests.
- Real Xcode SDK builds of both iOS runtime slices and the shared application archives pass, and the C ABI fixture links for both device and simulator.
- The profiler end-to-end suite, benchmark tooling, 13 bank native cases, and 17 bank browser tests pass. The bank build regenerated its committed browser bundle with the reviewed compiler, so ordinary native builds also receive the current WASM output. Formatting, strict workspace Clippy, Hawk, and the node dependency guard pass.
- Full `make ci` is **not green**. After fixing the WASM test race and restoring the CLI coverage gate, its remaining stages were run directly. The extension suite reached debugger execution and timed out waiting for a session. macOS reports Developer Tools access disabled; standalone LLDB also times out launching a trivial C program. Repeated blocked debugger attempts were stopped, and the subsequent independent bank/tool/domain targets passed. Fresh extension coverage therefore remains unverified.
- Xcode's first-launch component installer is waiting for administrator authorization; the first full iOS build attempt failed because `IDESimulatorFoundation` could not load the installed system framework. Complete `xcodebuild -runFirstLaunch` and the macOS Developer Tools authorization, then rerun `make ios-test mobile-ios-test` and `make _test_vscode_extension _coverage_check_vscode_extension`. The final physical-iPhone smoke and hosted macOS/Linux/Windows PR checks remain required.

## TODO checklist

The checked items record implementation and earlier validation. The open items are requirements before declaring this branch ready to merge into `main`.

- [x] Define and implement the scalar C ABI, imports, initialization, and application lifetime.
- [x] Add explicit iOS/WASM capability errors and independent exported-entry effect checks.
- [x] Build device/simulator archives and generated headers through LLVM.
- [x] Deliver the SwiftUI host with a real Swift platform callback.
- [x] Verify C ABI execution, simulator goldens, and the actual SwiftUI application.
- [x] Run the whole `tests/` corpus through the mobile C ABI on both platforms — 130 byte-exact goldens and 18 GPU comparisons each — with rejections pinned in a shared reviewed manifest.
- [x] Extend the shared C ABI fixture to the boundary contract: integer extremes, float round-tripping, register-passed booleans, UTF-8, empty/long strings, host-allocated strings, and the borrowed-input copy rule.
- [x] Fix the diagnostics that named the wrong target: `ios-sim` reported as `ios`, and a phone told about a WebAssembly proposal.
- [x] Verify signed physical-iPhone execution through the shared Issue Inbox, including its full native smoke and real GitHub data.
- [x] Verify compiler tests, WASM goldens, strict Clippy, formatting, and the unchanged WASM exclusion set.
- [x] Complete the branch code review against `407e0c3d0a69cf3cc39583b09a5670bd8732fd0e`, covering the iOS/Android boundary and changes to shared native/WASM compiler behavior.
- [x] Resolve the review findings and add regression coverage for confirmed defects.
- [x] Verify the reviewed compiler/native/WASM checks and record fresh results separately from the earlier validation.
- [x] Verify Android ARM64 execution, both architecture builds, shared tests, deterministic/live app workflows, and lint.
- [x] Verify the final iOS application builds and simulator execution after Xcode component installation. `make ios-test` and `make mobile-ios-test` both pass, ending in `OSPREY_IOS_SMOKE_OK` and `OSPREY_INBOX_SMOKE_OK`.
- [ ] Complete the updated Markdown smoke on the physical iPhone, as tracked in [plan 0030](0030-reactive-mobile-apps.md). Blocked on the phone itself: it is paired with Developer Mode enabled, but reports `tunnelState=disconnected` over the local network, so the device build refuses before signing. Unlock it and connect it to this Mac, then rerun.
- [x] Resolve the duplication gate discrepancy using CI's pinned Deslop version; preserve the 5% ceiling.
- [x] Run the extension suite and its coverage gate, previously blocked by macOS Developer Tools authorization: 316 tests pass and coverage is 98.3% lines, 98.3% statements, 95.9% branches, 96.5% functions, all above the 95% threshold.
- [ ] Pass both hosted PR workflows, including the newly enforced mobile checks, before merging.
