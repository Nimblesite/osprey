# Plan 0023 — GPU Computation: Surface Completion & Device Backends

**Subsystem:** `crates/osprey-types` + `crates/osprey-codegen` + `compiler/runtime` (+ CLI target drivers)
**Status:** Stages 1–2 substantially shipped (typed surface, purity checking,
host backend, eleven built-ins, full differential corpus); stages 3–7 are
device work, unstarted
**Spec:** [0034-GPUComputation.md](../specs/0034-GPUComputation.md)

## Summary

The GPU surface is a language feature, not a library: `GpuBuffer<T>` in the
type system, kernel purity proven by the effect checker (fail-closed), and a
host execution backend whose fused loops over dense unboxed buffers define
the reference semantics device backends must reproduce byte-for-byte. What
remains is everything between "host loops" and "PTX": kernel extraction,
the first device target, effect-selected execution, portability backends,
and the differentiation features (autodiff, schedules) that justify the
"superior to Mojo" positioning.

## What works today

- `GpuBuffer<T>` with the scalar element restriction `[GPU-BUFFER-ELEM]`
  enforced at call sites (`builtin_constraints.rs`), and the `Gpu#<lltype>`
  owner-tag scheme propagating element `LType`s through lets, pipes,
  parameters and returns (`crates/osprey-codegen/src/gpu.rs`,
  `types.rs::owner_name`).
- Eleven built-ins: `toGpu`, `fromGpu`, `gpuLength`, `gpuMap`, `gpuFold`,
  `gpuZipWith`, `gpuIota`, `gpuGet`, `gpuScan`, `gpuFilter`, `gpuDevice` —
  each with a docs entry (parity-tested) and spec ID.
- Kernel purity `[GPU-KERNEL-PURE]` via the static effect discharge
  machinery (`effect_rows.rs::gpu_kernel_verdict`), fail-closed, with
  rejection fixtures in `examples/failscompilation/`.
- Dense-buffer C runtime (`compiler/runtime/gpu_runtime.c`) working under
  all three memory backends and wasm32; ARC reclaims buffers with zero
  leaks.
- Differential corpus: six suites × two flavors in `tests/core/gpu/`
  (`buffers`, `combinators`, `mlkernels`, `gamedev`, `stress`, `raster`) —
  34 cases covering round-trips, all ten data combinators,
  dot/matvec/relu/gradient descent, particles/culling/damage, fragment-shaped
  raster rendering, and million-element profiling workloads with closed-form
  expected values. Twins emit identical IR; goldens are byte-exact under
  default/GC/ARC and wasm32 (`GOLDEN_MIN` 172/119).

## Gaps delegated to other plans

- **Block-bodied lambda kernels** and **recursive generic emission** —
  [plan 0002](0002-codegen-generic-function-values.md) (spec
  `[GPU-KERNEL-FORM]`).
- **`toFloat` conversion builtin** —
  [plan 0004](0004-collection-stdlib-completion.md) (spec `[GPU-CONVERT]`).
- **Context-free kernel int-defaulting (F10)** —
  [plan 0022](0022-arithmetic-totality-audit.md) (spec
  `[GPU-KERNEL-ELEM-TYPING]`).

## Design decisions carried from the research foundation

The scholarly grounding lives in the spec's References section
(`[GPU-RESEARCH]`). The *decisions* that research produced are recorded
here because they shape the remaining stages:

- **Device sublanguage, not device ADTs.** Every production functional GPU
  language (Futhark, Accelerate) restricts device data to a regular,
  first-order scalar sublanguage; ADTs, closures and pattern matching stay
  in host code and at kernel boundaries. Representing sum types on SIMT
  hardware is an open research frontier — Osprey's ADT advantage is a
  host/boundary feature by design, and `[GPU-BUFFER-ELEM]` is that
  decision made normative.
- **Backend substrate — decision checkpoint at stage 4 kickoff.** Osprey
  emits textual LLVM IR and hands it to clang (no LLVM linkage), and the
  wasm32 target already works as a sibling link driver
  (`crates/osprey-cli/src/wasm.rs`). The incremental device path follows
  that shape: NVPTX via clang plus a CUDA-driver launch shim. The
  strategic alternative is an MLIR pipeline (`gpu` → `nvgpu` → `nvvm`),
  which is how Mojo gets tensor-core/TMA access without hand-written PTX
  and is the only studied route to Blackwell-class features (`tcgen05`,
  TMEM) through a portable stack. Track NVIDIA's open-source CUDA Tile IR
  (`cuda-tile`) and Triton's in-tree Gluon DSL as evidence of where the
  tile abstraction is heading. Choose at stage 4 kickoff; do not commit in
  the spec.
- **Effect classification is the stage-5 design.** Dex distinguishes
  parallelism-destroying `State` from the parallelism-preserving,
  monoidal `Accum` effect; Xie et al. (ICFP 2024) give the formal account
  of which handler shapes commute with parallel evaluation — the theory
  closest to Osprey's handlers. The `Gpu` selection effect (stage 5) and
  any future accumulation effect must be designed against that work, not
  ad hoc.
- **Associativity is a documented contract, not a checked one.**
  `gpuFold`/`gpuScan` document backend-dependent results for
  non-associative combines (the Futhark position). Verifying associativity
  is out of scope for every stage below.
- **Performance thresholds are gates, not aspirations.** Stage 4 does not
  promote its build flag until within 2× of Futhark/CUDA on stencil and
  reduction micro-benchmarks. The longer-term bar: close on hand-written
  CUDA for memory-bound kernels and win where Mojo has measured gaps —
  the ORNL study (Godoy et al., SC-W 2025) found Mojo at ~87% of CUDA on
  a memory-bound stencil, with no fast-math option, weak AMD atomics, and
  no POD structs in GPU memory.
- **Algorithm/schedule separation is the stage-7 shape.** The functional
  combinator program is the algorithm; scheduling (tiling, fusion, memory
  placement) becomes an optional, typed, separate layer per
  Halide/TVM/Exo — never annotations smeared through kernel code.
- **Competitive snapshot (July 2026, re-verify before any public claim):**
  Mojo 1.0 is still in beta (beta 2, June 2026), its compiler is closed
  until Fall 2026, and `match`/enums are deferred to 1.x releases. Typed
  GPU programming has active venue appetite (Prism/Bundl at PLDI 2026,
  Kuiper at ARRAY 2025, linear-logic AD at POPL 2026) — publishing the
  device sublanguage's type-and-effect rules is a credibility play worth
  its own milestone.

## Remaining stages

3. **Kernel extraction.** Lower kernels passed to GPU combinators into
   standalone IR functions with explicit scalar signatures (today they are
   inlined into the host loop). This is the compiler-side prerequisite for
   any device target and is fully testable with no GPU: diff
   extracted-kernel output against host-loop output in the differential
   harness.
4. **First device target — NVPTX.** Kernel IR compiled to PTX, a
   CUDA-driver launch path in the runtime behind a build flag, buffer
   transfer using the existing dense layout (which was chosen as the
   staging layout for exactly this moment). Gated on the 2× benchmark
   threshold above.
5. **Effect-selected execution.** A `Gpu` effect whose handler chooses the
   execution strategy per lexical region; `gpuDevice()` reports the
   handler's choice; a test handler pins `"host"` so CI is deterministic
   with no GPU attached. This is the construct competitors bolt on as
   library calls — the headline language feature.
6. **Portability targets.** Metal (macOS CI can actually execute it) and
   WebGPU/WGSL beside the existing wasm target.
7. **Autodiff and schedules.** Differentiable kernels
   (Elliott's categorical AD; Dex's parallelism-preserving AD; Slang.D's
   differentiable type discipline) and the optional schedule layer.

Stages ratchet: each keeps `make ci` green and the differential harness
byte-exact, and later stages must not change the meaning of programs
accepted by earlier stages (`[GPU-ROADMAP]`).

## TODO checklist

### Stage 1 — typed surface + host backend

- [x] `GpuBuffer<T>` type, `[GPU-BUFFER-ELEM]` call-site enforcement,
      `Gpu#` owner-tag element recovery.
- [x] `toGpu`/`fromGpu`/`gpuLength`/`gpuMap`/`gpuFold` + docs entries.
- [x] `[GPU-KERNEL-PURE]` fail-closed purity proof reusing static effect
      discharge; fixtures `gpu_kernel_impure.ospo`,
      `gpu_kernel_unprovable.ospo`, `gpu_buffer_element_not_scalar.ospo`.
- [x] Dense-buffer C runtime under default/GC/ARC + wasm32, zero ARC leaks.
- [x] `tests/core/gpu/buffers` twins + goldens; IR-equivalent flavors.

### Stage 2 — surface completion

- [x] `gpuZipWith` (`[GPU-ZIPWITH]`), `gpuIota` (`[GPU-IOTA]`), `gpuGet`
      (`[GPU-GET]`, `Result`-typed OOB), `gpuScan` (`[GPU-SCAN]`),
      `gpuFilter` (`[GPU-FILTER]` via `osprey_gpu_take` compaction).
- [x] `gpuDevice` (`[GPU-DEVICE]`) host introspection.
- [x] Corpus expansion: `combinators`, `mlkernels`, `gamedev`, `stress`,
      `raster` suites × 2 flavors; `GOLDEN_MIN` 172 native / 119 wasm.
- [ ] Buffer literals (construct a `GpuBuffer` without a `List` detour).
- [ ] `Iterator` → `GpuBuffer` fusion so `range |> map |> toGpu` never
      materializes a list.
- [ ] Float pipeline seed `gpuIota(n) |> gpuMap(toFloat)` once `toFloat`
      lands (plan 0004); extend `stress.test.osp` to seed from `gpuIota`.
- [ ] Kernel-form gaps closed (plan 0002): block-bodied lambda kernels;
      recursive helpers without load-bearing annotations — then strip the
      annotations from `mlkernels`/`stress` twins.
- [ ] Kernel element typing (plan 0022 F10): an unannotated `add` shared by
      an int fold and a float fold in `tests/core/gpu/`.

### Stage 3 — kernel extraction

- [ ] Extract each combinator kernel into a standalone IR function with an
      explicit scalar signature; host loop calls it instead of inlining.
- [ ] Spec ID `[GPU-KERNEL-EXTRACT]` in 0034 defining the extracted ABI
      (params, return, no captures beyond scalar/buffer operands).
- [ ] Harness mode diffing extracted-kernel output against the inlined
      host-loop output for the whole `tests/core/gpu/` corpus.
- [ ] Closure captures lowered to explicit kernel parameters (the
      `mean`-capture and `w`-capture cases in `mlkernels.test.osp` are the
      acceptance tests).

### Stage 4 — first device target (NVPTX)

- [ ] Backend-substrate decision checkpoint: clang/NVPTX sibling driver
      (wasm-shaped) vs MLIR `gpu`→`nvgpu`→`nvvm`; record the decision and
      its benchmark evidence here.
- [ ] PTX emission for extracted kernels behind a build flag.
- [ ] CUDA driver launch + transfer shim in `compiler/runtime/` using the
      existing dense staging layout; no API surface change.
- [ ] CI story with no GPU: device path compiles everywhere, executes where
      hardware exists, host diff remains the source of truth.
- [ ] Benchmark gate: within 2× of Futhark/CUDA on stencil + reduction
      micro-benchmarks before the flag is promoted; record numbers in
      `benchmarks/`.

### Stage 5 — effect-selected execution

- [ ] `Gpu` effect + handler-selected execution strategy per region;
      design reviewed against Xie et al. (ICFP 2024) handler-commutation
      results.
- [ ] `gpuDevice()` reports the innermost handler's selection; host
      default with no handler.
- [ ] Purity interaction specified: kernels stay pure; *scheduling* is the
      effectful part ([GPU-DEVICE] example becomes real).
- [ ] Rejection fixtures: selecting an unknown device; performing `Gpu`
      inside a kernel.

### Stage 6 — portability targets

- [ ] Metal backend (runnable in macOS CI).
- [ ] WebGPU/WGSL backend paired with the wasm32 target.
- [ ] Differential harness extended so every backend must match the host
      goldens byte-for-byte.

### Stage 7 — autodiff and schedules

- [ ] Autodiff design spike: Elliott (ICFP 2018) categorical core +
      Paszke et al. (ICFP 2021) parallelism-preserving accumulation +
      Slang.D differentiable-type safety; produce a design doc before any
      surface.
- [ ] Optional typed schedule layer (Halide/Exo separation) — algorithm
      untouched, schedules separate and checkable.
- [ ] Publish the device sublanguage's type-and-effect rules
      (PLDI/ICFP/ARRAY workshop) once stages 3–5 are implemented.

### Cross-cutting (every stage)

- [ ] Every new builtin: scheme in `builtins.rs`, docs entry, spec ID,
      corpus coverage in both flavors, `GOLDEN_MIN` ratchet bump.
- [ ] `make ci` green at each stage boundary: clippy, coverage floors,
      deslop ceiling, IR-equivalent twins, three memory backends + wasm32.
- [ ] `docs/messaging.md` GPU qualification updated whenever a stage
      changes what may truthfully be claimed.
