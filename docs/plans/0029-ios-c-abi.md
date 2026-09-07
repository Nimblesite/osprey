# Plan 0029 — iOS C ABI application logic and Swift host

**Subsystem:** CLI targets, LLVM code generation, effect checking, C runtime, and the SwiftUI example.

**Spec:** [0038-iOSTarget.md](../specs/0038-iOSTarget.md), with shared target restrictions in [0022-WebAssemblyTarget.md](../specs/0022-WebAssemblyTarget.md).

**Status:** The iOS implementation and targeted validation are complete. Full `make ci` verification remains open: the implementation session's attempt stopped because `deslop` was unavailable. The existing CI gates remain unchanged.

## Scope and architecture

Osprey supplies application logic through a synchronous C ABI. Swift owns the iOS application lifecycle, UI, and platform APIs. Swift calls generated Osprey exports; Osprey requests platform services through explicit C imports implemented by the Swift host.

This delivery covers `--target=ios` for ARM64 iPhones and `--target=ios-sim` for the ARM64 iOS Simulator. Android is a future platform for the same architectural boundary and is outside this plan. The exact type mapping, ownership contract, deployment version, and unsupported options belong to the spec rather than a second definition here.

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

Evidence: driver unit tests and the real device/simulator C host compilation and simulator execution in [test-ios.sh](../../scripts/test-ios.sh).

## Phase 4 — Integrate the Swift application

Implement `[IOS-SWIFT-HOST]` using [examples/ios/](../../examples/ios/):

- Keep counter behavior and message formatting in Osprey; call them from SwiftUI controls.
- Import the generated C header and link the SDK-specific archive through the checked-in Xcode project.
- Implement a Swift platform callback with C linkage, copy its borrowed string, and call Foundation's logging API.
- Provide reproducible device/simulator builds, simulator launch, and assertions running inside the actual application.

Evidence: [SmokeCheck.swift](../../examples/ios/OspreyCounter/SmokeCheck.swift) covers initialization, integer bounds, boolean/string results, and the Swift callback. The [example README](../../examples/ios/README.md) documents build, signing, and launch steps.

## Phase 5 — Verify and record delivery

The implementation session verified device and simulator builds, C host links, the running SwiftUI simulator application, and 14 Default/ML language goldens covering arithmetic, collections, strings, JSON, files, and fibers. It also passed the CLI, codegen, and type suites, target rejection tests, strict workspace Clippy, and formatting checks. Device execution was not claimed; the built device application is unsigned and requires Xcode signing for installation.

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

Full CI completion requires the repository's normal tools, including `deslop`. Install or provide the missing prerequisite and run the unchanged gate; do not remove, bypass, or weaken it. Additional platforms, memory backends, continuation support, teardown, and packaging require their own scoped work and tests before changing the current rejection rules.

## TODO checklist

- [x] Define and implement the scalar C ABI, imports, initialization, and application lifetime.
- [x] Add explicit iOS/WASM capability errors and independent exported-entry effect checks.
- [x] Build device/simulator archives and generated headers through LLVM.
- [x] Deliver the SwiftUI host with a real Swift platform callback.
- [x] Verify C ABI execution, 14 simulator goldens, and the actual SwiftUI application.
- [x] Verify compiler tests, WASM goldens, strict Clippy, formatting, and the unchanged WASM exclusion set.
- [ ] Run the full unchanged `make ci` with its required tools available and record the result.
