# Plan 0023 — GPU Computation: Surface Completion & Device Backends

**Subsystem:** `crates/osprey-types` + `crates/osprey-codegen` + `compiler/runtime` (+ CLI target drivers)
**Status:** **NOT SHIPPED. No GPU execution exists.** Stages 1–2 (typed
surface, purity checking, CPU host backend, eleven built-ins, differential
corpus) are substantially done. Stages 3–7 — kernel extraction and *every*
device backend — are unstarted. Until stage 4 lands, this feature runs
entirely on the CPU and nothing in it may be described as GPU-accelerated.
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

## Read this first: `gpu*` does not touch a GPU

Nothing in the shipped `gpu*` surface runs on graphics hardware. `gpuMap` is a
`for` loop on the CPU, `gpuDevice()` returns the string `"host"`, and no code
path in the compiler or runtime talks to a driver. `[GPU-BACKEND-HOST]` is the
only backend that exists; `[GPU-BACKEND-DEVICE]` is stage 4 and unstarted.

The names are a *dispatch surface* — the shape a kernel must have to be
offloadable later. They are not a claim about where the work happens. Anything
that reads otherwise (a make target, a demo header, a README line) is a
documentation defect; fix it where you find it.

### What that costs, measured

Measured July 2026 on an M-series Mac, release build, default memory backend:

| Workload | Result |
| --- | --- |
| Per checked arithmetic op inside a kernel | **~18 ns** (~60–70 cycles) |
| `examples/graphics` fragment scene, 2 × `gpuMap` + `gpuZipWith` + blit over 64 000 px | **76 ms/frame → 13 fps** |
| Same scene as a Metal fragment shader, 1920×1200 drawable | **103 fps** |

The middle row is the honest ceiling of the host backend for anything
per-pixel. The dominant cost is *not* buffer overhead or the FFI blit (2
ms/frame, measured separately) — it is that every `+`, `*` and `intDiv` inside
a kernel is a checked, `Result`-returning operation, at roughly 60–70 cycles
each. A kernel doing 40 arithmetic ops per element therefore costs ~700 ns per
element before anything else.

Two consequences that shape the stages below:

- **Stage 3 (kernel extraction) is worth doing for the host backend alone.**
  Extracted kernels with concrete scalar signatures are what let the optimiser
  see a kernel body as straight-line integer code instead of an inlined chain
  of `Result` branches.
- **No amount of surface work closes a 13 → 103 fps gap.** Only stage 4/6
  (real device backends) does. Do not accept host-backend micro-optimisation
  as progress toward the performance claims in this plan.

### The macOS graphics bridge already exists

`examples/graphics/` is a working Osprey → macOS bridge and the reference
shape for stage 6's Metal target:

- `ospgfx.m` — Cocoa window + `CAMetalLayer`, flat C ABI (`osp_gfx_open`,
  `osp_gfx_set`, `osp_gfx_draw`, `osp_gfx_ticks`, `osp_gfx_close`).
- `scene.metal` / `scene2.metal` — fragment shaders, loaded from disk at open
  time.
- `scene.osp` / `scene2.osp` — Osprey as the host: each owns the window, the
  clock and the per-frame scene state, and pushes parameters over FFI
  ([FFI-PTR]).
- `make graphics` builds the dylib and runs it.

It deliberately bypasses `gpu*`, because `gpu*` cannot reach the GPU. It proves
the boundary works — FFI, a real drawable, 103 fps — so stage 6 is wiring
extracted kernels (stage 3) into a Metal compute pipeline, not starting from
nothing. It does **not** run in CI: it needs a display, and it is not in the
differential corpus.

**Pre-existing red, not owned by this plan.**
`crates/osprey-cli/tests/graphics_scenes.rs` (committed in `5f007c6d`) asserts
a structure that has never existed in git history: a shared `base.osp` +
`base.metal`, three ≤4-line entry points `scene{,2,3}.osp` importing
`graphics::SceneBase`, and **no** `scene2.metal`/`scene3.metal`. On disk there
is no `base.*` and no `scene3.osp`, so the test fails at its first read, and
its `panic!` also fails `cargo clippy -D clippy::panic`, which is what turns
`make lint` red today. `scene.osp` does already carry the exact `original*`
timings the test pins and `scene.metal` all seven pinned shader markers, so the
refactor is mostly mechanical — except the "character" scene, which does not
exist in any form and has no spec. Resolve by either implementing the refactor
or deleting the aspirational test; do not leave it red.

## What works today

- `GpuBuffer<T>` with the scalar element restriction `[GPU-BUFFER-ELEM]`
  enforced at call sites (`builtin_constraints.rs`), and the `Gpu#<lltype>`
  owner-tag scheme propagating element `LType`s through lets, pipes,
  parameters and returns (`crates/osprey-codegen/src/gpu.rs`,
  `types.rs::owner_name`).
- Eleven built-ins: `toGpu`, `fromGpu`, `gpuLength`, `gpuMap`, `gpuFold`,
  `gpuZipWith`, `gpuIota`, `gpuGet`, `gpuScan`, `gpuFilter`, `gpuDevice` —
  each with a docs entry (parity-tested) and spec ID. `toFloat`
  (`[BUILTIN-TOFLOAT]`) is the scalar conversion that makes float pipelines
  expressible.
- Two list-free buffer constructions: buffer literals
  `[GPU-BUFFER-LITERAL]` (a literal `toGpu` argument stores straight into the
  dense buffer) and iterator fusion `[GPU-BUFFER-FUSE]` (`toGpu` consumes an
  `Iterator<T>`, replaying the pending map/filter stages inside the fill
  loop). Both emit zero `osprey_list_*` calls.
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

- **Recursive generic emission** (monomorphization) and **ML-parser support
  for block-bodied lambdas** — [plan
  0002](0002-codegen-generic-function-values.md) (spec `[GPU-KERNEL-FORM]`).
  Block-bodied lambda kernels already work in the Default flavor; see the
  stage-2 entry for the measurement.
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

## Start here — the next session's work order

**Stage 2 is now complete except for the two items delegated to other plans.**
`toFloat`, buffer literals and iterator fusion all landed; the remaining two
stage-2 boxes are blocked in plan 0002 (recursive generic emission; ML-parser
block-lambda support) and plan 0022 (F10 numeric constraint on the arithmetic
operators). Neither is a GPU-surface change — do not try to close them here.

**The next work on this plan is stage 3, kernel extraction.** It is the
critical path to every device backend, it is fully testable with zero GPU
hardware, and it is the only remaining item that improves the host backend's
13 fps ceiling — because it is what lets the optimiser see a kernel body as
straight-line integer code instead of an inlined chain of `Result` branches.
Everything in stage 4+ waits on it.

If the goal for a session is "make `gpu*` actually use the GPU", the route is
stage 3 → stage 6 (Metal, since `examples/graphics/` already proves the
window/drawable/FFI half on this hardware) or stage 4 (NVPTX). Nothing else on
this page gets there, and no amount of further surface work will.

Landmines previous sessions hit:

- Unannotated recursive functions **fail closed** ("annotate its parameters
  and return type", `genfn.rs` re-entry guard). Deliberate — generic functions
  are inlined, never emitted, so a recursive one has no call target.
- Twins must emit identical IR (`cargo test -p osprey-cli --test
  cross_flavor_ir_equiv`), so a construct only one flavor's parser accepts
  cannot enter the corpus.
- Goldens are byte-exact under default/GC/ARC **and** wasm32. Wasm goldens run
  under `make wasm`, not `make ci` — run both. `make wasm` also *builds*
  `libosprey_runtime_wasm.a`; without it every wasm golden fails at once and
  `TEST_CORPUS_WASM_SKIPPED` reads 0 instead of 53.
- `GOLDEN_MIN` (172 native / 119 wasm) counts *programs*, not test cases —
  adding a `test(...)` to an existing suite does not move it. Never lower it.
- The deslop duplication ceiling is 5.00% and the repo is near it: any new
  combinator in `gpu.rs` must reuse `kernel_of`/`scalar_acc_init`.
- Float `/` returns `Result<float, MathError>` — kernels need `?:`. ML twins:
  no braces in constructor patterns (`Success value`), lambdas are `\x => …`,
  parenthesize match-arm and pipe continuations.
- A builtin passed by name as a callback needs a value form in
  `expr.rs::call_builtin_with_values`, or it emits `call @name` to a symbol
  that is never defined and fails at *link* time with no source location.
- A statement followed by a line starting with `(` parses as a **call**:
  `let d = 5` then `(d + 1) ?: d` becomes `5(d + 1)`. Write `d + 1 ?: d`.

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
- [x] Buffer literals `[GPU-BUFFER-LITERAL]` — a literal `toGpu` argument
      stores straight into the dense buffer at constant indices
      (`gpu.rs::buffer_literal`). `toGpu([1.0, 2.0, 3.0, 4.0])` emits one
      `osprey_gpu_alloc` plus four stores: no `osprey_list_*`, no `malloc`, no
      loop. `buffers` twins now also pin the list-*value* path.
- [x] `Iterator` → `GpuBuffer` fusion `[GPU-BUFFER-FUSE]` — `toGpu` accepts an
      `Iterator<T>` wherever it accepts `List<T>` and replays the pending
      map/filter stages inside the fill loop (`expr.rs::fuse_gpu_source`
      retargets the scheme's container, keeping the element variable so the
      `GpuBuffer<T>` return stays linked; `gpu.rs::fuse_iterator` lowers it).
      `range(0, 100) |> map(dbl) |> toGpu()` emits zero `osprey_list_*` calls.
      A `filter` stage publishes the kept prefix via `osprey_gpu_take`; an
      inverted range yields an empty buffer. Covered by `combinators`
      `fusionCase`.
- [x] Float pipeline seed — `toFloat` landed (plan 0004,
      `[BUILTIN-TOFLOAT]`); `stress` `floatChurnCase` seeds from
      `gpuIota(8) |> gpuMap(toFloat)`. Required closing a latent hole: a
      builtin passed *by name* as a callback emitted `call @toFloat` to a
      symbol never defined, failing at link time with no source location.
      `expr.rs::call_builtin_with_values` now lowers `print`, `toString`,
      `toFloat` and `abs` to value forms at the callback site.
- [ ] Kernel-form gaps (plan 0002). Re-measured; the two halves differ:
      - Block-bodied lambda kernels **already work in Default** —
        `gpuMap(fn(x) => { let d = x * 2 ?: 0  d + 1 ?: d })` and `|x| => { … }`
        both run. Only the brace-only `fn(x) { … }` (no `=>`) is rejected.
        The blocker is the ML parser, which has no multi-statement lambda body,
        so the construct cannot enter the corpus without drifting
        `cross_flavor_ir_equiv`. Next step is ML-parser support, not Default
        grammar work.
      - Recursive helpers still need annotations. Generic functions are
        lowered by inlining (`genfn.rs::try_inline`), never emitted as symbols,
        so a recursive generic has no call target and the guard must fail.
        Closing it means real monomorphization — a name-mangled copy per
        instantiation with the self-call bound to it. `mlkernels`/`stress`
        annotations stay load-bearing until then.
      - Fixed meanwhile: nine builtin docs examples used the rejected
        `fn(x) { … }` spelling, and four were also semantically wrong (`x * 2`
        is checked arithmetic, so `map`/`forEach` printed `Success(2)` where
        the comment claimed `2`; `filter`/`gpuZipWith` did not compile). All
        nine now run and match their stated output.
- [ ] Kernel element typing (plan 0022 F10). Re-measured; narrower than "int
      defaulting". Let-polymorphism is fine (`fn id(x) = x` instantiates at
      three types). `+` is the problem: in `fn add(a, x) = a + x` it resolves
      to the int operation with no numeric constraint, so `gpuFold(0.0, add)`
      gives `cannot unify int with float` and `gpuFold(0, add)` gives
      `cannot unify int with Result<int, MathError>` (checked int `+` returns a
      `Result`; the combine slot wants `(v, t) -> v`). Both need a numeric
      constraint on the operators — plan 0022 F10, not a GPU-surface change.

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

- [ ] Metal backend (runnable in macOS CI). The window/drawable/FFI half is
      already proven by `examples/graphics/` (see above) — the missing half is
      compiling stage-3 extracted kernels to MSL and dispatching them as a
      compute pipeline, so `gpuMap` itself reaches the GPU instead of a demo
      shader hand-written beside it.
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
