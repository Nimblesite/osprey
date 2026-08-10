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
- `scene.metal` — fragment shader, loaded from disk at open time.
- `scene.osp` — Osprey as the host: it owns the window, the clock and the
  per-frame scene state, and pushes parameters over FFI ([FFI-PTR]).
- `make graphics` builds the dylib and runs it.

It deliberately bypasses `gpu*`, because `gpu*` cannot reach the GPU. It proves
the boundary works — FFI, a real drawable, 103 fps — so stage 6 is wiring
extracted kernels (stage 3) into a Metal compute pipeline, not starting from
nothing. It does **not** run in CI: it needs a display, and it is not in the
differential corpus.

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

Work the checklist below in this order; each item names the files it lives in.

1. **`toFloat` (plan 0004, ~1 hour, unblocks two checklist items here).**
   One `sitofp` arm in `crates/osprey-codegen/src/conv.rs` + scheme in
   `builtins.rs` + docs entry, per `[GPU-CONVERT]` in spec 0034 (semantics
   already specified: round-to-nearest-even, exact for |n| ≤ 2^53). Then
   seed float pipelines from `gpuIota(n) |> gpuMap(toFloat)` in
   `tests/core/gpu/stress.test.osp` and tick the stage-2 seed item.
2. **Buffer literals + `Iterator` → `GpuBuffer` fusion** (stage 2 remainder,
   self-contained in `crates/osprey-codegen/src/gpu.rs` + `iter.rs`). Fusion
   means `toGpu` consuming an iterator chain writes elements straight into
   the dense buffer — no intermediate `List`. Corpus proof: extend an
   existing suite, don't add a file.
3. **Stage 3 kernel extraction** — the critical path to any device backend,
   fully testable with zero GPU hardware. Everything in stage 4+ waits on
   it; start it as soon as 1–2 land. It is also the only item on this list
   that improves the host backend's 13 fps ceiling, because it is what lets
   the optimiser see a kernel body as straight-line integer code.

Items 1 and 2 are surface polish. **They do not move this feature toward
running on a GPU.** If the goal for a session is "make `gpu*` actually use the
GPU", skip to item 3 and then stage 4 or stage 6 — nothing else on this page
gets there.

Items *not* to start from this plan: block-bodied lambda kernels and
recursive-generic emission belong to plan 0002, F10 int-defaulting to plan
0022. If you land those there, come back and strip the load-bearing
annotations from the `mlkernels`/`stress` twins and tick the corresponding
stage-2 items here.

Landmines the last session hit (avoid re-learning them):

- Unannotated recursive functions now **fail closed** with "annotate its
  parameters and return type" (`genfn.rs` re-entry guard, golden
  `recursive_generic_needs_annotation.ospo`) — that diagnostic is
  deliberate, not a bug to "fix" by reverting the guard.
- Twins must emit **identical IR** (`cross_flavor_ir_equiv`), goldens are
  byte-exact under default/GC/ARC **and** wasm32 — wasm goldens run under
  `make _test_wasm_goldens`, *not* `make ci`; run both before claiming
  green.
- `GOLDEN_MIN` (172 native / 119 wasm in `crates/run_test_corpus.sh`) must
  be bumped whenever the corpus grows — it is a ratchet, never lower it.
- The deslop duplication ceiling sits at 5.00% and the repo is at ~4.99%:
  any combinator added to `gpu.rs` must reuse `kernel_of`/`scalar_acc_init`
  or it will trip CI.
- Float `/` returns `Result<float, MathError>` — corpus kernels need `?:`.
  ML twins: no braces in constructor patterns (`Success value`),
  parenthesize match-arm and pipe continuations.

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
- [x] Buffer literals `[GPU-BUFFER-LITERAL]` — a literal argument to `toGpu`
      stores its elements straight into the dense buffer at constant indices
      (`gpu.rs::buffer_literal`). Neither the flat literal block nor an
      `OspreyList` is built: `toGpu([1.0, 2.0, 3.0, 4.0])` emits one
      `osprey_gpu_alloc` plus four stores — zero `osprey_list_*` calls, zero
      `malloc`, zero loops. Covered by `buffers` twins, which now also pin the
      list-*value* path so the copy loop cannot rot.
- [x] `Iterator` → `GpuBuffer` fusion `[GPU-BUFFER-FUSE]` — `toGpu` is a
      consuming stage of the iterator pipeline, accepting `Iterator<T>`
      wherever it accepts `List<T>` (`expr.rs::fuse_gpu_source` retargets the
      scheme's container while keeping the element variable, so the
      `GpuBuffer<T>` return stays linked; `gpu.rs::fuse_iterator` replays the
      pending stages inside the fill loop). `range(0, 100) |> map(dbl) |>
      toGpu()` emits zero `osprey_list_*` calls. A `filter` stage publishes
      the kept prefix via `osprey_gpu_take`; an inverted range yields an empty
      buffer, not a negative allocation. Covered by `combinators` twins
      (`fusionCase`).
- [x] Float pipeline seed `gpuIota(n) |> gpuMap(toFloat)` — `toFloat` landed
      (plan 0004, `[BUILTIN-TOFLOAT]`); `stress` twins' `floatChurnCase` now
      seeds from `gpuIota(8) |> gpuMap(toFloat)` instead of a literal buffer.
      Landing it required closing a latent hole: a builtin passed *by name* as
      a callback emitted `call @toFloat` to a symbol that is never defined, so
      it failed at **link** time with no diagnostic.
      `expr.rs::call_builtin_with_values` now lowers the intrinsic builtins
      (`print`, `toString`, `toFloat`, `abs`) to their value forms at the
      callback site — previously only `print` was handled.
- [ ] Kernel-form gaps closed (plan 0002): block-bodied lambda kernels;
      recursive helpers without load-bearing annotations — then strip the
      annotations from `mlkernels`/`stress` twins.
      **Re-measured this session — the two halves are in very different
      places, and the earlier framing of this item was wrong.**
      - *Block-bodied lambda kernels already work in the Default flavor.*
        `gpuMap(fn(x) => { let d = x * 2 ?: 0  d + 1 ?: d })` and the `|x| =>
        { … }` spelling both compile and run correctly, as does a
        block-bodied `gpuFold` combine. `block` has been an `expression`
        choice in `tree-sitter-osprey/grammar.js` all along. What does *not*
        parse is the brace-only spelling `fn(x) { … }` (no `=>`).
        The blocker on ticking this box is the **ML twin**: the hand-written
        ML parser has no block-lambda form at all (neither `\x => { … }` nor
        a layout block), so the construct cannot enter the corpus —
        `cross_flavor_ir_equiv` requires twins to emit byte-identical IR, and
        a Default-only construct drifts them (verified: adding one made
        `buffers.test` fail that test). **Next step is ML-parser support for
        a multi-statement lambda body, not grammar work on the Default side.**
      - *Recursive helpers still need their annotations.* Stripping them from
        `mlkernels`' `rowDot`/`train` fails closed with "`rowDot` is recursive
        but its signature is not fully inferred; annotate its parameters and
        return type". This is structural, not a missing special case: generic
        functions are lowered by **inlining** at each call site
        (`genfn.rs::try_inline`), never emitted as symbols, so a recursive
        generic has nothing to call and the re-entry guard must fail. Closing
        it means real monomorphization — emitting a name-mangled copy per
        instantiation with the recursive self-call bound to that symbol —
        which is plan 0002's "recursive generic emission" and touches function
        emission, ARC frames and coverage. The `mlkernels`/`stress`
        annotations stay load-bearing until then.
      - **Defect found meanwhile, fixed:** nine shipped builtin docs examples
        (`range`, `forEach`, `map`, `filter`, `gpuMap`, `gpuFold`,
        `gpuZipWith`, `gpuScan`, `gpuFilter` in `builtin_docs_lang.rs`) used
        the brace-only `fn(x) { … }` spelling the compiler rejects. Four were
        *also* semantically wrong — `x * 2` is checked arithmetic returning
        `Result`, so `map`/`forEach` printed `Success(2)` where the comment
        claimed `2`, and the `filter`/`gpuZipWith` examples did not compile at
        all. These are published to `website/src/docs/functions/`. All nine
        now compile and produce exactly the output their comments claim.
- [ ] Kernel element typing (plan 0022 F10): an unannotated `add` shared by
      an int fold and a float fold in `tests/core/gpu/`.
      **Re-measured this session, and the cause is narrower than "int
      defaulting".** Let-polymorphism is *not* the problem — `fn id(x) = x`
      instantiates fine at `int`, `float` and `string` in one program. The
      problem is `+` itself: in `fn add(a, x) = a + x` the operator resolves
      to the int operation with no numeric constraint to keep it open, so
      - `toGpu([1.5, 2.5]) |> gpuFold(0.0, add)` → `cannot unify int with
        float`, and
      - `toGpu([1, 2, 3]) |> gpuFold(0, add)` → `cannot unify int with
        Result<int, MathError>`, because checked int `+` returns a `Result`
        while the combine slot wants `(v, t) -> v`.
      Both need a numeric constraint on the arithmetic operators — a
      type-class-shaped mechanism HM alone cannot express. That is plan 0022's
      F10 and is not a GPU-surface change; the GPU corpus is only where it
      shows up.

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
