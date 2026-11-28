# Plan 0055 — Compile targets and code generation

Contract: [WASM-TARGET-CAPABILITIES], [IOS-TARGET-CAPABILITIES], [IOS-HOST-ABI], [SYMEX-CORPUS], [spec 0034 GPU computation](../specs/runtime/0034-GPUComputation.md), [spec 0037 arithmetic effects](../specs/language/0037-ArithmeticEffects.md), [performance gate](../specs/ecosystem/PerformanceGate.md), [GC heap layout](../specs/compiler/0018-MemoryManagement.md#tracing-heap-layout-mem-backends-gc-heap), [embedded targets](../specs/compiler/EmbeddedTargets.md) and the [graphics contract](../../src/osprey/examples/graphics/README.md).

Goal: every program in `src/osprey/tests/` and `src/osprey/examples/` runs on every target, the generated code meets the performance gate, and the GPU, embedded, JavaScript and .NET targets land behind one typed program. The three `src/osprey/tests/*_UNPORTABLE.txt` manifests are the corpus acceptance: a row leaves only when the program works on that target, and `src/toolchain/crates/run_test_corpus.sh` fails on any other change to them.

## Corpus: `osprey hunt` cannot explore (`HUNT_UNPORTABLE.txt`) [SYMEX-CORPUS]

`scripts/hunt` hunts these rows and marks each with its listed reason ([symbolic execution](../specs/tooling/SymbolicExecution.md)).

- [ ] Model resumable continuations for KLEE: `__osprey_coro_new` is native code outside the explored module, so every path through a resumable handler ends `unmodelled`. Check: the 42 `fibers and continuations` rows finish a hunt with no `unmodelled` finding and are deleted.
- [ ] Give nested fibers room under the engine: a fiber runs on the stack of whatever waited for it, so `fiber_showcase` reaches KLEE's stack limit (`Maximum stack size reached`). Check: both twins finish a hunt and their two rows are deleted.
- [ ] Model sockets in the mock C library so programs explore past `http_create_server`, `http_create_client`, `websocket_create_server` and `osprey_net_local_address`. Check: the 19 `HTTP needs sockets`, `websocket_create_server` and `osprey_net_local_address` rows are deleted.
- [ ] Add a `pipe` model, then what process spawning needs after it. Check: the four `process spawning needs pipe` rows explore their spawn paths and are deleted.
- [ ] Decide per library whether to model or keep skipping: `PQlibVersion`, `osp_gfx_open`, `ios_host_log`, `osprey_ffi_cell`, `osprey_python_call`, `term_raw_mode`. Check: each of the ten rows is either modelled and deleted, or states why it stays.

## Mobile C ABI targets reject (`MOBILE_UNPORTABLE.txt`) [IOS-TARGET-CAPABILITIES] [IOS-HOST-ABI]

- [ ] Support resumable continuations on the mobile targets, where a suspended continuation cannot cross or outlive a synchronous host call (shared with [plan 0016](0016-algebraic-effects-and-handlers.md)). Check: the 37 `a continuation for` rows match the native goldens on `ios-sim`, `macos`, `android-arm64` and `android-x64` and are deleted.
- [ ] Give the HTTP builtins a mobile implementation through the host (shared with [plan 0037](0037-osprey-native.md)). Check: the 14 `HTTP builtin` rows match the native goldens on all four targets and are deleted.
- [ ] Give the WebSocket builtins the same treatment. Check: the two `WebSocket builtin` rows match the native goldens and are deleted.
- [ ] Decide per target whether process spawning exists (no iOS implementation today, and the one manifest cannot express a per-target answer). Check: the four `process spawning builtin` rows are implemented and deleted, or the manifest is split and each target's rows are justified.
- [ ] Make `// @link: sqlite3` and `// @link: ssl` work in a mobile archive. Check: the four `link directive` rows match the native goldens and are deleted.

## wasm32 rejects (`WASM_UNPORTABLE.txt`) [WASM-TARGET-CAPABILITIES]

Tracking issue: #30.

- [ ] Implement one-shot continuations on wasm32 when the stack-switching proposal lands ([plan 0016](0016-algebraic-effects-and-handlers.md)). Check: the 37 `a continuation for` rows match the native goldens and are deleted; a `many` program never appears in the manifest ([EFFECTS-INV-MULTISHOT-WASM], [CI-MULTISHOT-WASM]).
- [ ] Give wasm32 the HTTP builtins, which need sockets today. Check: the 14 `HTTP builtin` rows match the native goldens and are deleted.
- [ ] Give wasm32 the `listDir` directory builtin. Check: the two `directory access builtin` rows are deleted.
- [ ] Resolve the foreign C imports `sqlite3_open`, `fiber_set_deterministic_mode`, `osprey_net_local_address` and `websocket_stop_server` on wasm32, keeping the two documented browser host imports. Check: each is compiled to wasm or supplied by the host and its seven rows are deleted.
- [ ] When a manifest has no rows left, delete it with its branch in `src/toolchain/crates/run_test_corpus.sh` or `scripts/hunt` and its references in specs, scripts and `.github/workflows/ci.yml`. The four child-process rows are permanent on `wasm32-wasip1` ([target contract](../specs/compiler/0022-WebAssemblyTarget.md#target-capability-checking-wasm-target-capabilities)).

## JavaScript and .NET targets

Not accepted yet: feasibility work only. Both need a new emitter and a non-C runtime; Osprey owns the control-flow transform and treats host async features as scheduling primitives, never as the meaning of effects.

- [ ] Decide: Node only, or browsers too.
- [ ] Decide: .NET emits an IL assembly, C# source, or C# first and IL later.
- [ ] Decide: JavaScript `int` is `BigInt` or `number`.
- [ ] Decide: whether `resume` and blocking fibers are out of the MVP.
- [ ] Split the backend: typed program → target-neutral lowered IR → LLVM, JavaScript and IL emitters, making explicit closure conversion, control flow, ADT layout, the effect operation table, fiber and channel operations, resumable regions, per-function calling convention and each target's runtime imports. Check: LLVM output stays byte-identical or golden-equivalent.
- [ ] JavaScript MVP: ES modules and a runtime for values, ADTs, closures, `Result`, lists, maps and effect context. Check: the portable corpus matches its goldens.
- [ ] .NET MVP: the same surface with `Task`-backed fibers and channels. Check: the portable corpus matches its goldens.
- [ ] Shared resumable lowering: mark the functions that cross suspension points (`perform`, `resume`, `yield`, `await`, `send`, `recv`, `select`) and lower them to compiler-emitted state machines or CPS; mapping `resume` onto promises, `Task` or plain calls is the one too-lossy approach. Check: control-operation programs give the native answers on both targets, and `many` continuations run as the ordinary heap closures the compiler already writes ([EFFECTS-INV-MULTISHOT-PORTABLE], [plan 0016](0016-algebraic-effects-and-handlers.md)).

Background: [OCaml effects](https://ocaml.org/manual/5.5/effects.html), [Koka](https://koka-lang.github.io/koka/doc/index.html), [JS execution model](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Execution_model), [Node AsyncLocalStorage](https://nodejs.org/api/async_context.html#class-asynclocalstorage), [TC39 Async Context](https://github.com/tc39/proposal-async-context), [.NET AsyncLocal](https://learn.microsoft.com/en-us/dotnet/api/system.threading.asynclocal-1?view=net-10.0), [.NET Channels](https://learn.microsoft.com/en-us/dotnet/core/extensions/channels), [ECMA-335](https://ecma-international.org/publications-and-standards/standards/ecma-335/).

## Embedded targets [TARGET-EMBEDDED-PLAN]

Code for these targets is profiled on the device and optimised continuously, beside every item ([DEVICE-PERF]). The first consumer is the [Prism device](https://github.com/Nimblesite/perception/blob/main/docs/specs/synth/Perception.md#perception-device-device).

Cross-compiled ARM Linux executables [TARGET-EMBEDDED-LINUX]:

- [ ] Failing CLI tests first: `--target=linux-arm64 --sysroot <dir> --compile` from macOS and Linux x86-64 produces an `aarch64` ELF executable; a missing or unusable sysroot is a typed error naming it [TARGET-EMBEDDED-LINUX-CROSS].
- [ ] Build the hardened C runtime for `aarch64-unknown-linux-gnu` against the sysroot, cached by the hash of sources, flags and sysroot [TARGET-EMBEDDED-LINUX-RUNTIME].
- [ ] Link with LLD against the sysroot; no host library may leak into the link.
- [ ] Run the test corpus under ARC on a GitHub-hosted ARM Linux runner in the CI test phase, byte-for-byte against the goldens, with coverage, and run a macOS cross-built executable on the same ARM runner [TARGET-EMBEDDED-LINUX-VERIFY].
- [ ] GPU kernels on this target are a compile error naming the feature, with a `failscompilation/` program [TARGET-EMBEDDED-LINUX-CAPS].

Real-time code [TARGET-EMBEDDED-REALTIME]:

- [ ] Decide and specify the source spelling for a real-time function in the language specification, with a regression and a `failscompilation/` program, before any code.
- [ ] Lower real-time functions with Clang's `nonblocking` effect; mark the runtime functions they may call.
- [ ] Compile error for a real-time call graph that reaches an allocation, free, blocking lock, fiber operation, I/O or such a handler, naming the call path.
- [ ] Real-time test build with `-fsanitize=realtime`; the synth render journeys run under it.
- [ ] Release of a value whose count reaches zero inside the callback is deferred to the end of the block, with a test that proves no free happens inside.

Bare metal [TARGET-EMBEDDED-BARE]: not started; it begins after the Prism device passes its hardware journeys on the embedded Linux target.

## Generated-code performance [MEM-BACKENDS-GC-HEAP]

The GC heap now allocates small blocks from size-class pages (`src/toolchain/compiler/runtime/memory_gc_pages.h`); what remains is the evidence.

- [ ] Record `rust-checked` timings with `make -C src/toolchain/benchmarks bench` (`run.sh` builds it with `-C overflow-checks=on`; `merge_results.py` and `report.py` render the column). Check: `src/toolchain/benchmarks/results/results.json` lists `rust-checked` in `languages` with a `status: "ok"` cell for every case, and `src/toolchain/website/src/_includes/benchmarks-tables.html` shows a `Rust (checked)` column.
- [ ] Re-time the page allocator with `make -C src/toolchain/benchmarks bench-osprey` on a quiet machine. Check: `binarytrees` under GC takes at most half its recorded 0.98 s, no Osprey GC cell is more than 5% slower than its record, and no other language's value changes.

Out of scope: ARC allocation profiling, a moving or generational collector, and Windows support for GC.

## GPU computation and graphics backends

Contract: [spec 0034](../specs/runtime/0034-GPUComputation.md). `gpuScan` and `gpuFilter` have device forms; GPT-2 runs every kernel and product on the selected device; Half A is done and the 2× native-reference gate (Half B) is not.

Device coverage gates:

- [ ] Preserve recovering `Arith` policies in selected-device regions [ARITH-DEVICE-RECOVERY]: under a wrapping handler forced Metal returns a handled `Error` instead of an abort, but the host recovers and the device does not, so `mlkernels` and `Qwen3_5Device.wrapping` fail on Metal. Check: the no-trap contract and the fault tests in `src/toolchain/compiler/runtime/gpu_device_tests.c` hold on the device.
- [ ] Transcendental determinism [FLOAT-IEEE-RESULTS]: one shared host/device `exp`/`ln`/`tanh` (`gpu32_exp`/`gpu32_tanh` in `gpu_device_lib.metal` already agree bit for bit with the host), verified exact across vendors before it is claimed; `gpu_shader_tests.c` proves there is no universal correct rounding.
- [ ] Kernel-to-kernel fusion and capability negotiation beyond the two adapters ([GPU-BACKEND-HOST], [GPU-BACKEND-PORTABILITY]; the [LLM-SPEED] lever, with [plan 0049](0049-local-coding-model.md)): the package model's token still spends about 2,000 small generated kernels per pass. Check: an `OSPREY_GPU_TIMING=2` profile shows fewer dispatches and less device time per token than the recorded 2,230-dispatch profile, with the numbers under `src/toolchain/benchmarks/`.
- [ ] Device-index arithmetic without workarounds: let a kernel take a run-time non-zero divisor without a fault word (`may_fault` in `gpu_runtime.c`), prove host and device agree, then delete `Div`, `quot` and `rem` from `src/osprey/examples/llms/qwen/model/src/device.py`.
- [ ] Make the selected device visible: `OSPREY_GPU` unset means the host oracle, so `osprey test` should name the adapter in its summary, with a test for host and Metal.
- [ ] Add a required forced-Metal lane for `src/osprey/packages/transformers/tests` beside the core GPU corpus in `.github/workflows/ci.yml`.
- [ ] Host and Metal differential over the real Qwen workload: compare the stages' bytes between the two, or record the tolerance per stage with its reason [FLOAT-IEEE-RESULTS].
- [ ] Vendor-neutral substrate checkpoint [GPU-BACKEND-PORTABILITY]: record the shared device representation and launch/transfer interface for Metal, SPIR-V and WebGPU (optional CUDA never determines the common ABI or requires CUDA tooling), with benchmark evidence.
- [ ] CI with and without GPUs: adapter code compiles on its platforms, and hardware lanes verify completed device dispatch and host parity on Apple, AMD, Intel and NVIDIA; a CUDA-only lane cannot close portable GPU delivery.
- [ ] Benchmark gate: within 2× of a native reference on the same GPU at matching precision for the stencil and reduction micro-benchmarks and for GPT-2 Half B before the device flag is promoted; record the numbers under `src/toolchain/benchmarks/`.
- [ ] GPT-2 Half B: unchanged `src/osprey/examples/llms/gpt2/` source against a native reference on the same GPU at matching precision within 2×, with CUDA as the NVIDIA comparison only; record completed device commands and prove execution without CUDA on a non-NVIDIA GPU.

Effect-selected execution:

- [ ] Lexical adapter selection [GPU-DEVICE]: a handler-scoped selector replaces the `OSPREY_GPU` environment bootstrap; unknown selections keep failing in `src/toolchain/compiler/runtime/gpu_selection_tests.c::invalid_case`.
- [ ] Platform kernel sets as static handlers ([LLM-KERNELS-STATIC], [plan 0049](0049-local-coding-model.md)): one effect over the products and attention with reference, mac (MLX through mlx-c) and cuda (cuBLASLt and cuDNN on Windows) handlers chosen at compile time, plus a CI job that compiles the cuda build on a hosted Windows runner without a GPU.
- [ ] Design review against Xie et al. (ICFP 2024) on handler commutation before any device interpretation reorders operations.

Backend coverage and graphics convergence:

- [ ] Multi-dimensional index spaces (rank-2/rank-3) shared by the device adapters: a `build2`/`get2` pair with declared shapes in the `Tensor<B>` effect, designed against the attention and LayerNorm in `src/osprey/examples/llms/gpt2/model/model.osp` ([GPU-BUFFER] rank-1 limit; [GPU-MATMUL] shape arguments).
- [ ] SPIR-V compute coverage for AMD, Intel and NVIDIA without CUDA: unchanged-source GPT-2 and the differential device gates [GPU-BACKEND-PORTABILITY].
- [ ] WebGPU/WGSL backend paired with wasm32 [GPU-BACKEND-PORTABILITY]; WGSL and SPIR-V are one device sublanguage with two spellings.
- [ ] Optional CUDA interop through the shared backend contract; test that removing CUDA leaves non-CUDA GPU execution available.
- [ ] Extend the differential harness (`src/toolchain/crates/run_test_corpus.sh`) so every backend must match the host goldens byte-for-byte.
- [ ] Metal kernel and graphics coverage in macOS CI: compute kernels already reach Metal through `src/toolchain/crates/osprey-codegen/src/gpu_device.rs`; graphics convergence reuses that path and the uniform ABI `src/osprey/examples/graphics/ospgfx_abi.h`.
- [ ] Graphics convergence: a device shader emitter over the extracted-kernel ABI [GPU-KERNEL-EXTRACT], MSL first, with `ospgfx_abi.h`'s slot protocol as the uniform binding. Check: one scene's fragment body written as an Osprey kernel renders a frame byte-identical to the hand-written shader at matched uniforms, and `base.metal` becomes generated output with the hand-written library kept as the differential reference.
- [ ] When convergence lands, add `[GFX-HOST-PORTABLE]` to [GraphicsBackends](../specs/runtime/GraphicsBackends.md) and reference `[GFX-ABI]`, `[GFX-SHADER-LIB]` and `[GFX-UNIFORMS]` from the implementing code.

Graphics plumbing (the WinUI scene window, for [plan 0037](0037-osprey-native.md)):

- [ ] Draw the three scenes on a hardware Direct3D 12 adapter and compare a frame with the Mac's at matched uniforms, the only proof of the cbuffer packing and matrix order.
- [ ] WinUI window: `OspreyHost.exe --scene <scene.dll>` with a `SwapChainPanel`, the `ospgfx_host_bind` callbacks (attach, state, title), a composition swap chain (`CreateSwapChainForComposition`), the scene loop on its own thread and the WARP fallback; `make -C src/toolchain/targets/macos graphics SCENE=kali` on Windows builds the scene DLL, bridge and host.
- [ ] Extend `src/toolchain/crates/osprey-cli/tests/graphics_scenes/frame.rs`: stub `CreateSwapChainForComposition` and the host callbacks; injected-failure cases for a failed attach, a closed window and a resize.
- [ ] Thread test: the ARC runtime on Windows must not assume a particular thread.
- [ ] Graphics smoke: `OSPREY_GFX_FRAMES=120 make -C src/toolchain/targets/macos/tests graphics` renders each scene and fails on a single-colour frame.
- [ ] Delete the Win32 window path (`src/osprey/examples/graphics/ospgfx_d3d12_window.c` and the message pump) once the WinUI window renders; update `src/osprey/examples/graphics/README.md`.
- [ ] Add the graphics job to `.github/workflows/winui.yml`: fxc every entry, the bridge with `-Wall -Werror`, a drift guard and a 120-frame WARP smoke of all three scenes.

Autodiff and schedules:

- [ ] Autodiff design spike: Enzyme-style AD over the emitted IR against a source-level categorical core (Elliott, ICFP 2018) with Dex's parallelism-preserving accumulation and Slang.D's differentiable-type safety; a design doc before any surface.
- [ ] Optional typed schedule layer (Halide/Exo separation), algorithm untouched; CuTe/ThunderKittens tile semantics as the expressiveness bar.
- [ ] Publish the device sublanguage's type-and-effect rules (PLDI/ICFP/ARRAY) once the stages above are implemented.

MLIR investigation (decides only, and builds nothing; MLIR may be a backend but never changes source semantics [STAGE-DIALECT-INDEPENDENT], and the goal is the simplest design that keeps the hand-rolled path's fine-grained control):

- [ ] Map every device construct to an upstream dialect and record each gap: `gpuMap`/`gpuZipWith` → `linalg.generic`, `gpuFold` → `linalg.reduce`, `gpuScan`/`gpuFilter` → `scf` + `gpu` (or say why not), `gpuMatMul`/`gpuMatMulQuant` → `linalg.matmul` plus a Q4_K dequantize op, and buffers → `memref`/`tensor` with [GPU-BUFFER-RESIDENT] residency.
- [ ] Checked arithmetic: design `osprey.checked_*` ops whose conversion lowers to overflow-flagged `arith` plus the `Arith` handler's recovery path, so [ARITH-TOTAL] holds on every device (the one construct with no upstream equivalent).
- [ ] Prototype: lower one admitted kernel (the GPT-2 GELU map) and one product through `linalg → scf.parallel → gpu → nvvm/spirv`, and diff the output against the host goldens and the current Metal path.
- [ ] Measure dispatch count and device time per GPT-2 and Qwen token against the 2,230-dispatch baseline; `linalg` fusion and tiling are the main reason to switch, so the prototype must show them.
- [ ] Speed gate, head to head: benchmark both lowerings of every admitted kernel shape in `src/osprey/tests/core/gpu`, `gpuMatMul`/`gpuMatMulQuant` at the GPT-2 and Qwen shapes, and a whole GPT-2 and Qwen token on the same GPU at matched precision, with repeated runs and variance committed under `src/toolchain/benchmarks/`. A construct moves to MLIR only when MLIR matches or beats the hand-rolled path; a kept hand-rolled lowering cites its measurement.
- [ ] Cost: build and ship size of the MLIR C++ libraries, Rust access through [`melior`](https://github.com/raviqqe/melior) or the C API, the Windows (MSVC) and wasm32 toolchains, and CI time; the release (Shipwright, Homebrew, Scoop) must not need a user-installed LLVM/MLIR.
- [ ] Metal, which MLIR has no target for: decide between SPIR-V → MSL (SPIRV-Cross) and keeping the Metal emitter, and measure the cost against today's native path on Apple GPUs.
- [ ] Where MLIR enters the pipeline: (a) a GPU-only backend fed from the typed AST at the kernel-extraction seam [GPU-KERNEL-EXTRACT], (b) an Osprey dialect for the whole program with effects, handlers and static discharge as conversion passes [EFFECTS-STATIC-DISCHARGE], or (c) MLIR types and ops carried in the AST. Test (a) first; (c) couples the language core to a C++ framework.
- [ ] Replace or allow both: pick the simplest of (1) MLIR lowers everything and custom kernels are dialect ops with their own lowering, or (2) layered, where MLIR lowers the standard constructs and a narrow escape hatch keeps the hand-rolled emitter for what it cannot express or tune well. Every construct keeps exactly one lowering; a selectable parallel path is ruled out as duplication.
- [ ] Escape-hatch criteria, if layered: name the constructs that keep the hand-rolled emitter and the measured reason for each; prove that a new custom kernel needs no change to the MLIR path and that removing the escape hatch would lose something real.
- [ ] Decision record: the chosen shape, the measurements behind it and the files deleted; a construct MLIR wins loses its hand-rolled lowering in the same change.

Settled constraints: no signed 64-bit division on a device (divide unsigned magnitudes only), and never a Metal device across `fork`; a kernel's captures decide its device form, and a device run's recorded reason explains an abort; a new combinator sits on the existing chassis (`kernel_of`, `map_into`, `reduction`, `kernel_loop`) with `gpu::is_scalar` as the element rule, and new lowering goes in a new module beside `src/toolchain/crates/osprey-codegen/src/gpu/buffer.rs`, cache-keyed through `test_cache_key` in `src/toolchain/crates/osprey-cli/src/executable_cache.rs`; `GOLDEN_MIN` and `GPU_MODE_MIN` in `run_test_corpus.sh` are never lowered; twins emit identical IR and goldens are byte-exact under GC, ARC and wasm32; `base/base.osp` and every `scene.osp` stay byte-identical across backends, `ospgfx_abi.h`'s static-inline helpers are included with `-I` and never pasted, and the bridge stays `-Wall -Werror`.
