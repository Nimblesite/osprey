# Chapter 12 — Ship a real program

**Chapter outline.** Build walkthroughs and the Flight Log artifact checkpoint are planned. The target and memory qualifications below have been checked against the current contracts and CLI.

## Reader outcome

Choose `--check`, `--run`, or `--compile`, select a supported target, and describe what the resulting artifact depends on.

## Flight Log state

The planned checkpoint turns the tested project into a native executable, then builds its supported portable portion for WebAssembly. Native mobile deployment is an optional host-integration path, with a separate build and test toolchain.

## Core sections

1. Check, run, and compile are different jobs
2. Native code travels through LLVM and clang
3. WebAssembly supports a portable subset
4. Debug information and profiling answer different questions
5. Memory management is a build choice
6. Measure before changing the memory mode
7. C libraries are powerful and outside the safety guarantee

## Current target choices

| CLI target | Artifact | Boundary to explain |
|---|---|---|
| `native` | Executable for the build host | Native runtime; operating-system and library dependencies still matter. |
| `wasm32` | `wasm32-wasip1` command module | Needs a WASI host or the supported browser shim; built-in HTTP, fibers, processes, terminal APIs, and arbitrary C imports are unavailable. |
| `ios`, `ios-sim` | ARM64 static archive and generated C header | Needs an Apple SDK and Swift/Objective-C host; compiling an archive is not building or testing a complete iPhone app. |
| `android-arm64`, `android-x64` | Static archive and generated C header for that ABI | Needs the Android NDK and an Android/JNI host; validate each ABI and the host application. |

Value effects and validated static handler discharge have native, WebAssembly, and mobile paths. Dynamic control handlers require the native target. Static selection removes handled effect dispatch; it does not imply that all calls, allocations, or other runtime work disappear.

Mobile hosts provide platform networking through their C boundary. Keep records, collections, and handler values inside Osprey and expose supported scalar wrapper functions. Returned strings must be copied as documented by the mobile target guides; the current default allocator exposes no stable host release API for returned strings.

## Memory, debugging, and project boundaries

- Native builds accept `--memory=default`, `--memory=gc`, and `--memory=arc`. The default backend retains general allocations; proved-unique releases are a narrower optimization. GC and ARC have different reclamation behavior, so measure a representative workload before choosing one.
- The conservative GC contract covers Apple and glibc-Linux root discovery. It is not supported on Windows, and collection is disabled after a second allocator thread appears. Do not promise that selecting GC bounds memory in a concurrent native program.
- WebAssembly and mobile targets accept only `--memory=default`. The CLI rejects unsupported memory choices even with `--check`; it does not silently substitute a runtime. Strict static-memory checking and tail-call optimization remain unimplemented.
- `--debug` uses unoptimized native code with debug information. `--profile` keeps optimization and supplies profiling metadata. These native flags are rejected for WebAssembly and mobile targets; profiling a debug build would answer a different performance question.
- Project directories and `osprey.toml` manifests work. Logical namespaces and module imports are independent of source file paths, and one project may contain Default and ML files. Package registry and package-manager workflows are still design work; a working project is not evidence that dependency download or publication commands exist.

For an existing program, `osprey hello.osp --check` validates it, `osprey hello.osp --run` compiles and executes it, and `osprey hello.osp --compile -o hello` creates a native executable. Supply `--target=wasm32` when checking or compiling for that target; a successful native check does not establish WebAssembly support. A project path may replace the single-file path.

## Compiler-feedback exercise

Check the Chapter 1 hello program with `--target=wasm32 --memory=gc`. The current CLI rejects it with `error: wasm32 supports --memory=default; other runtime archives are not available`. Choose a supported target/runtime combination and retain the limitation in the artifact's instructions.

## Flight Log checkpoint

Produce a native artifact, record its command and environment, then build only the portion supported on WebAssembly.

## Planned visuals

- Source-to-target compile pipeline
- Native versus Wasm decision map
- Memory-mode comparison

## Source map

- Memory and foreign resources: `docs/specs/0018-MemoryManagement.md` and `docs/specs/0019-ForeignFunctionInterface.md`.
- Targets: `docs/specs/0022-WebAssemblyTarget.md`, `docs/specs/0038-iOSTarget.md`, and `docs/specs/0039-AndroidTarget.md`.
- Debug/profiling distinction: `docs/specs/0028-Profiler.md`, `[PROF-BUILD-MODE]`; executable checks in `scripts/test_profiler.sh`.
- Project support: `docs/specs/0025-ModulesAndNamespaces.md`, `examples/projects/modules/`, and `crates/osprey-cli/tests/cross_flavor_ir_equiv.rs`.
- Package status: `docs/specs/0029-PackageManagement.md` and `docs/plans/0020-package-manager.md`.
