# GPU branch review

## Baseline and verdict

Reviewed `gpu` at `2691d8dc55d64e022e4a7f4677d04a121f491aec` against
`main` at `2957618e95faa2337d01af18a91b2bef745160dd`. The branch is 118 files and
53,821 additions / 35,608 deletions, so this review covers the whole branch:
GPU spec 0034 and plan 0023, graphics plan 0025, staged-effects spec/plan
0035/0024, memory-runtime changes, and CI gates.

**Verdict: not spec-conformant and not ready to be represented as completed
GPU stages 1–3.** The CPU host prototype is substantial and the device-backend
status is honestly described as unimplemented. The blockers are type safety at
the buffer boundary, scalar transfer fidelity, kernel inference/forms,
fail-open differential gates, staged-effect soundness, and Windows GC root
discovery. The audit records 4 P0, 16 P1, and 10 P2 actions. There is no PTX,
Metal-compute, launch, transfer, multidimensional index-space, or device-memory
implementation yet.

The review is pinned to committed HEAD. Concurrent uncommitted implementation
edits in the shared worktree were excluded. No repository test suite, build, or
new golden was run. One focused reproduction using the already-built compiler
confirmed the generalized scalar-boundary escape described below.

## Blocking findings

### P0 — correctness and soundness

| ID | Finding | Evidence | Required action |
| --- | --- | --- | --- |
| P0.1 | **`[GPU-BUFFER-ELEM]` is bypassable through a polymorphic wrapper.** `fn makeLen(xs) = gpuLength(toGpu(xs))` accepts `List<string>`; `--check` succeeds and the program prints `2`. A wrapper containing `gpuMap` also accepts strings. | The spec requires rejection at buffer creation (`docs/specs/0034-GPUComputation.md:36-50`). `is_gpu_scalar(Type::Var(_))` returns true (`crates/osprey-types/src/builtin_constraints.rs:66-75`), then unresolved generalized obligations are discarded because schemes cannot retain them (`crates/osprey-types/src/check.rs:728-738`). The existing negative fixture covers only a direct call. | Carry the scalar constraint through generalization/instantiation, or revalidate constrained builtins at every concrete instantiation. Keep the new `gpu_buffer_polymorphic_non_scalar.ospo` must-reject golden. |
| P0.2 | **Scalar type metadata is not preserved across all legal buffer paths.** Runtime `List<float|bool>` values lose their tag in `toGpu`; `fromGpu` produces lists whose float/bool words are read as integers; contextually typed empty literals remain bare `GpuBuffer`. | `to_gpu` derives type only from a syntactic flat-list owner (`gpu.rs:228-250`); `from_gpu` pushes raw words into an untyped builder (`gpu.rs:253-264`); an empty literal learns type only from its nonexistent first element (`gpu.rs:159-174`). Plan 0023 admits the first two defects at lines 143-162. Existing list-copy/readback assertions index only ints. | Derive buffer/list representation metadata from the inferred `T`, never syntax or the first runtime element. Preserve it through builders, returns, parameters, and empty values. Make all new `scalar_contracts` cases green for `int`, `float`, and `bool`. |
| P0.3 | **Static-handler monotonicity is not transitive, and invalid unused arms can disappear before checking.** A static arm can call a helper with a dynamic effect; an outer dynamic handler can then make the program survive erasure. Unknown, duplicate, wrong-arity, or ill-typed unused arms can also be erased before ordinary effect/type validation. | `validate_static_arms` checks missing arms, any `resume`, and direct `perform` syntax only (`crates/osprey-ast/src/stage.rs:177-197,225-264`). Static discharge runs before type/effect checking (`crates/osprey-syntax/src/lib.rs:104-110,132-153`), and handler arms are dropped when the region is replaced (`crates/osprey-ast/src/lower_static.rs:110-130`). This violates `[STAGE-STATIC-MONOTONE]` and normal operation typing. | Validate every static arm against declared operation signatures and resolved transitive effect rows before erasure. Add direct/transitive, unused-arm, duplicate-arm, arity, and ill-typed must-reject fixtures. |
| P0.4 | **The branch adds a Windows GC caller-root gate without implementing Windows stack or data/BSS bounds.** The focused test is expected to be optimizer-dependent or fail; global roots are not scanned at all. | `_WIN32` reaches `gc_stack_base`'s fallback, which records an address in its own returned frame (`compiler/runtime/memory_gc.c:291-318`); collection scans only current-SP-to-that-address (`:265-279`). `gc_scan_data` implements Apple and glibc Linux only (`:193-214`). Spec 0018 requires stack, registers, and data/BSS (`docs/specs/0018-MemoryManagement.md:94-100`). The new test checks only an outer stack frame (`memory_gc_stack_root_tests.c:26-39`). | Use `GetCurrentThreadStackLimits` or TEB bounds on Windows, implement executable data/BSS bounds, and add both stack-root and global-root regressions. Make the Windows job required or explicitly remove Windows GC from the supported contract. |

### P1 — conformance, backend readiness, and gates

| ID | Finding | Evidence | Required action |
| --- | --- | --- | --- |
| P1.1 | **`[GPU-KERNEL-ELEM-TYPING]` fails and is expression-order dependent.** `t * x * y` can specialize as float while `x * y * t` defaults the inner expression to int; unannotated `plus(a, x) = a + x` cannot serve a float fold/scan. | Spec 0034:260-273; plan 0023:171-176,494-501. Call arguments/bodies are inferred before the combinator signature is applied (`crates/osprey-types/src/expr.rs:482-603`), and numeric defaulting closes unresolved operators too early. | Add numeric constraints and bidirectional/contextual inference so buffer/combinator slots type kernel parameters before defaulting. Make both multiplication orders and unannotated float combines green. |
| P1.2 | **`[GPU-KERNEL-FORM]` is neither implemented nor documented consistently.** Default block-bodied lambdas work, ML cannot spell one, and an unannotated recursive helper whose signature includes `GpuBuffer<T>` has no monomorphic emitted self-call. | The spec normatively permits every pure function form (`0034:236-244`) but its own gap bullets are stale/overbroad (`:246-258`). Plan 0023 corrects this at `:183-195,471-488`; `genfn.rs:15-31` fails closed for uninferable recursive generics. | Add ML layout-bodied lambda syntax and real monomorphization/name mangling for recursive generic helpers. Correct spec 0034 now; make `kernel_frontier` green in both flavors. |
| P1.3 | **Extraction can regress to zero while the differential stays green.** Published per-suite extraction counts are not ratcheted, and structural tests cover map/fold but not zip/scan/filter. | Plan 0023 publishes `21/9/20/78/11/13` at `:113-123`. `run_test_corpus.sh:366-409` compares transcripts only. Rust IR tests at `crates/osprey-codegen/src/lib.rs:1675-1848` cover map/fold/captures/declines, not every combinator or suite floors. | Assert extracted-symbol minima per suite, plus exact ABI tests for map, fold, zip, scan, and filter. Require at least one extracted kernel whenever a suite contains an admissible lambda. |
| P1.4 | **The extract/inline and ARC oracles are fail-open.** Alternate runs need not exit successfully, alternate ARC leaks are ignored, and a missing/malformed main ARC sentinel passes. Comparisons are not byte-exact. | Main status is checked at `run_test_corpus.sh:249-276`; main ARC parsing at `:294-300` fails only on a parsed nonzero value. Alternate mode at `:382-395` compares stdout without checking status or ARC. `trimmed()` removes outer whitespace (`:138-144`), and command substitution removes trailing newlines. | Require status `0` for both modes; require exactly one ARC `0` sentinel for both; compare raw files with `cmp`/`diff`. Do not call the current oracle byte-exact until fixed. |
| P1.5 | **Whitespace can make the differential compare inline against itself.** | `mode_of` trims and explicitly accepts `" inline "` (`gpu_kernel.rs:51-60,378-384`), while the shell selects the alternate from the raw string (`run_test_corpus.sh:73-79`). Main normalizes to inline; the harness also chooses inline. | Reject non-exact spellings or canonicalize once before choosing the opposite mode. Add a gate proving the two effective modes differ. |
| P1.6 | **The extraction contract overstates coverage.** “Each kernel” is said to become standalone, but closure cells, builtins by name, and host-bound bodies remain inline. This is safe for the CPU and has no device execution policy yet. | Spec 0034:293-300 versus its exceptions at `:333-358`; `gpu_kernel.rs:143-180`; plan 0023:533-555. | Change the headline to “each admissible kernel.” Before device launch, reject every declined form with a source diagnostic; never silently execute it on the host inside a device-selected region. |
| P1.7 | **The host backend is not array/combinator fused.** Each map/zip/scan/filter allocates a new buffer and runs a separate loop; only Iterator-to-`toGpu` is fused. | `gpu.rs:305-459` eagerly allocates per combinator. Spec 0034:381-387 and plan 0023:19-24 call the host loops “fused” and cite Futhark-style fusion. | Call the current implementation “native counted loops,” or implement a graph/array fusion pass with IR gates proving chained combinators avoid intermediate buffers. |
| P1.8 | **Reduction semantics conflict with the roadmap invariant.** Non-associative fold/scan results are allowed to vary by backend, while later backends are required not to change program meaning and to match the host byte-for-byte. | Spec 0034:131-139,177-186 versus `:368-377,381-390,444-451`; plan 0023 already flags the unresolved decision at `:321-326,579-582`. | Before device emission, either restrict/diagnose non-associative device reductions, define a deterministic association, or use a sequential fallback. Make the differential contract say which. |
| P1.9 | **Staged-effects claims exceed the implementation.** Tail `resume` is permitted by the spec but every `resume` is rejected; discharge is per source before project assembly, so declarations/handlers/helpers split across files cannot resolve; handler-owned static state is specified but not implemented/gated. | Spec 0035:141-167; `stage.rs:184-194`; `lower_static.rs` indexes one `Program`; syntax discharges before project assembly (`crates/osprey-project/src/lib.rs:177-181,236-270`). Plan 0024 marks all four obligations done (`:251-256`) while admitting static state is untested (`:102-103`). | Implement tail substitution, discharge after project assembly or against a project-wide symbol index, and preserve handler-state authority—or narrow the spec/status until each exists. |
| P1.10 | **The staged falsification gate is incomplete.** The plan says it passed, but the spec requires three concrete falsifiers before proceeding; only the `twice` case exists. | Plan 0024:29-41. Spec 0035:457-471 requires the higher-order case, generic-signal identity/reactive rebuild, and nested-parallel matmul. The corpus has only `twice` (`tests/effects/staged/staged_effects.test.osp:33-80`). | Add all three specified programs and keep the prototype status until all pass. Add the missing finite-rewrite-bound fixture. |
| P1.11 | **`--deps` is approximate and error-tolerant where the spec requires exactness.** Raw identifier names can create false dependencies through shadowing or passing an effectful function value; parse errors are dropped and the CLI still exits success. | `stage.rs:304-387`, `crates/osprey-syntax/src/lib.rs:121-130`, `crates/osprey-cli/src/main.rs:146-160`; `[STAGE-SIGNALS-EXACT]` at spec 0035:353-376. | Base dependencies on resolved effect-row provenance. Propagate diagnostics and exit nonzero, or explicitly report a widened/unknown dependency set. |
| P1.12 | **Graphics truth is split across plans and not merge-gated.** Plan 0023 calls Metal and D3D12 a working bridge; plan 0025 correctly says D3D12 has never compiled, linked, or run. Windows CI does none of the shader/bridge/scene checks and is non-required. | Plan 0023:75-88; plan 0025:6-11,116-124,158-184; `.github/workflows/ci-windows.yml:8-10,20-141`. | Correct plan 0023 to “Metal observed; D3D12 written/unverified.” Wire plan 0025 steps 1–4 into required Windows CI; retain step 5 as a hardware/manual gate until a self-hosted runner exists. |
| P1.13 | **D3D12 failure propagation contradicts plan 0025's completed implementation claim.** `Present`, fence wait, frame reset/close, and final queue drain failures are discarded; draw still returns success, and close can release resources after an unsuccessful drain. | `examples/graphics/ospgfx_d3d12.c:165-220,230-240` versus plan 0025:139-141,303-305. | Return status from submit/frame, propagate it through draw/close, and never reset or free GPU-owned resources after a failed drain. Add injected `Present`/`Signal`/timeout failure tests before calling the bridge implemented. |
| P1.14 | **The full wasm GPU differential is non-blocking, and its workflow coverage count is stale.** | `.github/workflows/ci.yml:370-385` runs it in a job explicitly documented as not required; `make ci` does not invoke `_test_wasm_goldens`. Its `103 of 160` comment also disagrees with the committed harness's `119 of 172`. This conflicts with plan 0023's cross-cutting merge gate (`:627-632`). | Move the wasm corpus differential into the required `ci` job or make the wasm job required. Derive the coverage count instead of duplicating it in workflow prose. |
| P1.15 | **Allocation failure/oversize behavior is silent and can leave loops using the original length.** An invalid or failed allocation becomes an empty buffer, while `gpuIota` and other lowerings build loops from the requested/source length. | `gpu_runtime.c:26-51`; `gpu.rs:371-380`. The spec defines no OOM/maximum-length contract. | Return/propagate an allocation error or derive loop bounds from the allocated buffer. Specify negative, oversized, and OOM behavior; add bounded failure tests. |
| P1.16 | **The staged-effect wasm rejection contract is not implemented.** The spec requires a diagnostic naming the effect and operation; the recorded gate reaches an undefined coroutine symbol, which the corpus treats as a generic capability skip. | Spec 0035:325-330; plan 0024:39; `run_test_corpus.sh:93-100`; `tests/WASM_UNPORTABLE.txt:12-15`. | Reject unsupported residual effects before linking, with the required effect/operation diagnostic. Do not use an arbitrary undefined symbol as proof of this contract. |

### P2 — coverage, performance, documentation, and traceability

| ID | Finding | Required action |
| --- | --- | --- |
| P2.1 | Literal/fusion claims such as “zero `osprey_list_*` calls,” one allocation, constant stores, and filtered `gpu_take` are semantic-test-only. | Add IR goldens/assertions for literal and iterator lowering. Be precise that `osprey_gpu_alloc` internally performs header and payload allocations plus zero-fill. |
| P2.2 | Purity tests cover impure map/fold and unprovable map, but not impure filter/zip/scan, transitive helper chains, or ML end-to-end rejection. | Add a table-driven checker test for all five kernel-bearing combinators plus Default/ML fixtures. |
| P2.3 | `gpuFilter` reads every source element twice. | Reuse the already-loaded `arg`/raw word in `gpu.rs:442-453`; add an IR call-count assertion. |
| P2.4 | Builtin docs “parity” checks names and arity only. `toGpu` docs omit Iterator input, and `gpuMap` repeats the stale “any effect” rule. | Validate prose/examples or compile documentation examples. Update `builtin_docs_lang.rs:523-545` for Iterator and statically discharged effects. |
| P2.5 | `[GPU-IOTA]` omits `n <= 0`; `[GPU-SCAN]` does not define how `initial` participates. Code/tests define empty for non-positive iota and `result[0] = combine(initial, source[0])`. | Put those semantics in spec 0034. |
| P2.6 | Spec traceability at committed HEAD is incomplete: no implementation reference for `[GPU-KERNEL-ELEM-TYPING]` or `[GPU-KERNEL-FORM]`; `[GPU-ROADMAP]` and `[GPU-RESEARCH]` have no code/test reference. | Add references where behavior exists; make roadmap/research headings explicitly non-normative if they are not implementation requirements. Comments alone do not close behavioral failures. |
| P2.7 | The July performance numbers have no benchmark artifact, and extraction's payoff is explicitly unmeasured. | Check in reproducible benchmarks/environment metadata; do not claim extraction speedup until measured. |
| P2.8 | The default backend now frees proved-unique values, matching updated spec, but has no direct default/ASan test. A separate plan still calls it an unchanged malloc passthrough. | Add null/exactly-once/conditional unique-free tests and update stale plan 0010 wording. |
| P2.9 | The graphics drift guard checks export names/text fragments rather than complete normalized ABI signatures, and compares shader constants Metal-to-HLSL only. HLSL-only drift can pass. | Derive complete signatures from one canonical ABI and compare constant sets bidirectionally with an explicit backend-only whitelist (`graphics_scenes.rs:315-347,449-477`). |
| P2.10 | The sole required CI job is named “Test, Format, Build & Validate,” but its lint step does not run formatting; `cargo fmt --check` exists only in the non-required Rust job. | Add format checking to the required job/`_lint`, or make the Rust job required (`.github/workflows/ci.yml:44-47,131-145,268-278`). |

## Spec 0034 conformance matrix

Status is against committed `2691d8dc`, before the intentionally red tests
added by this review.

| Requirement | Status | Review result |
| --- | --- | --- |
| `[GPU-BUFFER]` | Pass | Opaque type and dense `{length, i64*}` host layout exist; immutability/alias behavior is covered. |
| `[GPU-BUFFER-ELEM]` | **Fail** | Direct calls reject strings; generalized wrappers escape the scalar constraint. |
| `[GPU-BUFFER-FROM-LIST]` | **Fail** | Int/literal paths work; runtime float/bool lists lose element metadata. |
| `[GPU-BUFFER-LITERAL]` | Partial | Nonempty direct lowering works; contextually typed empty float/bool literals remain bare. Structural cost claims are ungated. |
| `[GPU-BUFFER-FUSE]` | Pass/ungated | Iterator typing and fused replay/compaction exist and have semantic coverage; zero-list-call structure is not asserted. |
| `[GPU-BUFFER-TO-LIST]` | **Fail** | Int works; float/bool readback reconstructs the wrong low-level type. |
| `[GPU-BUFFER-LENGTH]` | Pass | Constant-time runtime length with broad corpus coverage. |
| `[GPU-MAP]` | Pass with tagged inputs | Host semantics and scalar-result backstop exist; extraction/readiness limitations are separate. |
| `[GPU-FOLD]` | Pass with tagged inputs | Scalar accumulator and left-to-right host behavior exist. Device association policy remains open. |
| `[GPU-ZIPWITH]` | Pass with tagged inputs | Shorter-length semantics and scalar-result backstop exist. |
| `[GPU-IOTA]` | Implemented, underspecified | `[0,n)` behavior exists; non-positive behavior is tested but absent from the spec. |
| `[GPU-GET]` | Partial | Bounds/result behavior works when the buffer tag is correct; transfer/empty-tag defects break typed reads. |
| `[GPU-SCAN]` | Implemented, underspecified | Inclusive seeded behavior exists; seed semantics are absent from the spec. |
| `[GPU-FILTER]` | Pass, inefficient | Stable compaction exists; source elements are loaded twice. |
| `[GPU-KERNEL-PURE]` | Partial | Dynamic effects fail closed for every slot in code; coverage is incomplete and spec 0034 is stale versus static discharge. |
| `[GPU-KERNEL-FORM]` | **Fail** | Default block bodies work, ML parity and uninferable recursive generic emission do not. Spec text contradicts plan measurements. |
| `[GPU-KERNEL-ELEM-TYPING]` | **Fail** | Context-free/operator defaulting and expression order override buffer slots. |
| `[GPU-CONVERT]` | Pass | `toFloat` works directly and as a callback with boundary coverage. |
| `[GPU-KERNEL-EXTRACT]` | Partial | Flat ABI, deterministic captures, and lambda lifting exist; documented forms decline and no extraction-count ratchet exists. |
| `[GPU-BACKEND-HOST]` | Partial | Deterministic counted loops exist across runtimes/wasm; “fused” and byte-exact claims overstate implementation/gates. |
| `[GPU-BACKEND-DEVICE]` | Roadmap | Correctly documented as unimplemented. No GPU compute path exists. |
| `[GPU-DEVICE]` | Partial/roadmap | `gpuDevice()` truthfully returns `host`; effect-selected device execution is unimplemented. |
| `[GPU-ROADMAP]` | **Fail as a gate** | Later-stage invariance is stated, but alternate mode, ARC, raw-byte, wasm-required, and reduction guarantees are not closed. |
| `[GPU-RESEARCH]` | Documentation | References exist; treat this as non-normative or give it traceability appropriate to a requirement ID. |

## Plan and branch alignment

What is genuinely delivered:

- Eleven GPU builtins have schemes and docs; the host runtime is linked into
  default, GC, ARC, and wasm archives.
- The original six Default/ML GPU suite pairs contain exactly 100 cases
  (`15/18/17/21/16/13`), share six goldens, stay under 500 lines, and are all
  wasm-eligible.
- The extracted lambda ABI is first-order: sorted scalar/buffer uniforms,
  element slots, scalar return, no environment pointer. The documented suite
  counts `21/9/20/78/11/13` match the emitted IR measured for the commit.
- `gpuDevice()` and the `gpu-demo` target honestly report CPU/host execution.
- Plan 0025 is candid about unverified D3D12, and plan 0024 lists several
  prototype limitations rather than hiding them.
- GPU runtime archive integration is consistent across native memory backends
  and wasm. The default proved-unique free change matches updated spec 0018.

Claims that must be downgraded or corrected:

- Plan 0023's “stages 1–2 complete except delegated items” contradicts its own
  un-delegated `toGpu(list-value)` defect and the scalar-wrapper/empty-literal
  failures. Mark checklist items 430-437 and 450-454 incomplete or qualified.
- “Each kernel is extracted,” “fused host loops,” “byte-exact under every
  backend,” and “zero ARC leaks” are stronger than the implementation/gates.
- The test suites do route around float/bool transfer defects, but do not say so
  in source comments despite plan 0023:145-147 claiming they do.
- GPU spec 0034 says any handled effect remains illegal; staged spec 0035 and
  the staged GPU corpus intentionally accept effects erased by static handlers.
  The coherent rule is: **no dynamic/runtime effect may reach a kernel; a
  statically discharged effect is gone before the purity gate.**
- Plan 0024 cannot call its four obligations and falsification gate complete
  until tail resume, transitive monotonicity, arm typing, and all three specified
  falsifiers are implemented.
- Plan 0023 cannot call D3D12 working while plan 0025 and the source headers say
  it has never been compiled.

## Golden tests added by this review

These are deliberate forward-contract tests. They were **not run** and are
expected to be red against committed HEAD until the missing behavior lands.

| Suite | Cases | Contract pushed |
| --- | ---: | --- |
| `tests/core/gpu/scalar_contracts.test.{osp,ospml}` | 4 | Runtime float/bool list tags, float/bool `fromGpu`, repeated crossings, contextually typed empty literals. |
| `tests/core/gpu/kernel_frontier.test.{osp,ospml}` | 4 | Multiplication-order independence, unannotated float combine specialization, recursive generic kernel monomorphization, ML block-bodied kernels. |
| `tests/core/gpu/flavor_parity.test.{osp,ospml}` | 2 | Minimum-int literal parity and negative `gpuIota` argument parity. |
| `examples/failscompilation/gpu_buffer_polymorphic_non_scalar.ospo` | 1 reject | Scalar constraints survive generalized wrappers. |

The three shared runtime goldens raise the corpus from 100 to **110 GPU cases**
and from 12 to **18 GPU programs**. Static enumeration also found that the
existing global floor already lagged the committed corpus by one program.
Ratchets were raised to the actual totals, never relaxed:

- Native `GOLDEN_MIN`: `172 -> 179`
- wasm `GOLDEN_MIN`: `119 -> 126`
- `GPU_MODE_MIN`: `12 -> 18`

## Action order

1. Fix P0.1–P0.4; make the new scalar and must-reject contracts green.
2. Fix contextual numeric inference, recursive monomorphization, and ML block
   lambdas; make `kernel_frontier` and `flavor_parity` green.
3. Harden the differential/ARC/raw-byte/wasm gates and ratchet extraction
   structure before treating host output as a device oracle.
4. Reconcile specs 0034/0035 and plan status claims; close staged-effect
   transitivity, arm validation, project assembly, and tail resume.
5. Decide reduction association, declined-kernel diagnostics, transfer failure,
   multidimensional index spaces, and residency before writing a device emitter.
6. Only then start the first real compute backend and gate it against the host;
   separately verify D3D12 with plan 0025's five-step acceptance sequence.
