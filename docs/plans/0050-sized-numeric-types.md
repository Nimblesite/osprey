# Plan 0050 — Sized numeric types and native device arithmetic

Specs: [0004 Type System](../specs/language/0004-TypeSystem.md) (`TYPE-NUMERIC-*`), [0012 Built-ins](../specs/runtime/0012-Built-InFunctions.md) (`BUILTIN-CONVERT-*`, `BUILTIN-FLOAT-BITS`), [0034 GPU computation](../specs/runtime/0034-GPUComputation.md) (`GPU-BUFFER-ELEM`, `GPU-DEVICE-NATIVE`), [0019 FFI](../specs/compiler/0019-ForeignFunctionInterface.md) (`FFI-NUMERIC-WIDTH`).
Follows from: [plan 0049](0049-local-coding-model.md), whose phase 1 needs this plan's result.

## Goal

Osprey has two numbers: `int` is 64 bits and `float` is 64 bits. Every value in a `GpuBuffer` costs 8 bytes, and on Metal and OpenCL every float operation is a software binary64 routine, because that is the only float the language has. A 27-billion-parameter model therefore needs about 220 GB and a minute per token.

After this plan:

- Eight sized types exist beside `int` and `float`: `int8`, `int16`, `int32`, `uint8`, `uint16`, `uint32`, `float16`, `float32`. They are ordinary primitives: literals, arithmetic, comparison, interpolation, pattern literals, records, lists, `Result`, effects and FFI all accept them.
- A `GpuBuffer<T>` stores `T` at its own width: one byte for `uint8` and `bool`, two for `float16`, four for `float32`, eight for `float`.
- A kernel over `float32` runs native binary32 on the device, bit for bit the host's result for `+ - * /`, `sqrt` and the comparisons. Software binary64 remains only for `float`, which is the binary64 contract and stays.
- The emulation builtins `float32`, `float32Bits(x: float)` and `float32FromBits` are gone. Nothing widens silently: `gpuLoad` returns `GpuBuffer<float32>`, and `gpuLoadTensor` returns the type the caller asks for or an `Error`.

No legacy path survives: every call site in `examples/`, `packages/`, `tests/` and `apps/` moves to the new types in the same commit as the change that affects it.

## Decisions

| Question | Decision | Why |
| --- | --- | --- |
| Which types | `int8 int16 int32 uint8 uint16 uint32 float16 float32`, with `int` and `float` unchanged | One descriptor (`kind`, `bits`) drives every rule, so each type is a table row. `uint64` is left out: it would need its own `/` and comparison rules against `int` |
| Spelling | `int` and `float` stay the concise defaults; a sized type spells its kind and width in full (`int32`, `uint8`, `float16`), never `i32` or `f32` | The name says what the value is at a glance, and the common case stays short |
| Mixing widths | A binary operator needs both operands at one numeric type; the existing `int`→`float` promotion is the only implicit conversion | Rust's rule. Implicit widening among eight types is where C's bugs come from |
| Literals | A literal takes the numeric type its context asks for and `int`/`float` otherwise; out of range is a compile error | `let x: uint8 = 300` must not compile. Uses the same open-obligation machinery as `deferred_arith` |
| Overflow | Checked at the type's own width through the same `Arith` effect as `int`; narrowing conversions are `Arith` operations too | [ARITH-TOTAL] already covers it; the device fault word covers kernels |
| `float16` arithmetic | Computed in binary32 and rounded to binary16 | For `+ - * /` and `sqrt` the result equals the correctly rounded binary16 operation (24 ≥ 2·11 + 2), so host and device agree and no fp16 extension is needed |
| `float16` in LLVM IR | `i16` bits; conversions through the runtime's `osp_f16_to_f32` / `osp_f32_to_f16` | LLVM `half` needs compiler-rt helpers the wasm32 lane does not link |
| Integer `/` | `float` for every integer type, as today | `/` has type `float` is the existing rule; every sized integer fits a double exactly |
| Conversions | `toInt`, `toInt8` … `toUint32` over integer sources; `toFloat`, `toFloat32`, `toFloat16` over any numeric source; `floatToInt` over any float source; width-named `floatBits` families | Target-named, like `toFloat` today. No casts syntax |
| `List<float32>` | Still one 8-byte word per element | `List` is the persistent, shared structure; `GpuBuffer` is the dense one ([GPU-BUFFER]). Documented, not hidden |
| `bool` buffers | One byte | Same rule as every other element; was eight |
| `bfloat16` | A safetensors `BF16` tensor loads into `GpuBuffer<float32>` by exact widening; there is no `bfloat16` type | Widening is exact and the type would add nothing a kernel can use |
| Device float math | Metal compiled with safe math and `#pragma METAL fp contract(off)`; OpenCL with `#pragma OPENCL FP_CONTRACT OFF` | Without both, `a * b + c` contracts to an FMA and the device stops matching the host |

Out of scope, owned by [plan 0049](0049-local-coding-model.md): memory-mapped weight buffers, block-quantized weights, the single-owner mutable state buffer.

## Where 64 bits is hard-wired today

Audited on 2026-11-03.

| Layer | Evidence |
| --- | --- |
| Type names | `crates/osprey-types/src/ty.rs` `names::INT`, `names::FLOAT`; `is_numeric_scalar` in `builtin_constraints.rs` tests those two names |
| Literals | `Expr::Integer(i64)`, `Expr::Float(f64)`; arithmetic int-defaults "for want of a numeric class" (`expr.rs:1001`) |
| Lowering | `crates/osprey-codegen/src/types.rs` maps `int`→`i64`, `float`→`double`; `LType` has `I64` and `Double` only; container tags spell `i64`/`double` (`llty.rs:126`) |
| Arithmetic | `expr/operators.rs:162` emits `llvm.*.with.overflow.i64` only; `conv.rs` saturates to `i64`; `arithmetic/math.rs` emits `fptrunc double … to float` for the emulation builtins |
| Printing | `osp_float_to_string(double)` in `compiler/runtime/string_runtime.c`; `float_spelling.c` finds the shortest double spelling |
| Buffers | `OspreyGpuBuffer.data` is `int64_t *` (`gpu_runtime.c:29`); `OspreyGpuArg.words` is `const int64_t *` and `osprey_gpu_arg_bytes` multiplies by 8 (`gpu_device.h`) |
| Device dialect | `Ty { Int, Float, Bool }` spells `long`, `ulong`, `bool` (`gpu_device.rs:52`); `FIXED_PARAMETERS` declares every buffer `ulong *`; `osprey_matmul64` only |
| Device compile | `newLibraryWithSource:options:nil` (`gpu_metal.m:49`): Metal's default is fast math on; OpenCL builds with `-cl-std=CL1.2` and the default `FP_CONTRACT ON` |
| Loaders | `gpuLoad` and `gpuLoadTensor` widen to double (`gpu/io.rs`, spec 0034 lines 69–85) |
| FFI | Every numeric extern parameter is `i64` or `double` (`extern_call.rs`) |
| Flavors | Python flavor states "floats are binary64 … no float32 substitution" (`0041-PythonFlavorSyntax.md:76`); the TextMate grammars list `int|float|string|bool|any|Unit` |

## Phases

Each phase ends with its tests in the corpus, its spec IDs cited in code and tests, a `make ci` pass, and one commit. Work stops at a red gate and reports it.

### Phase 1 — One numeric descriptor

- [x] `crates/osprey-types/src/numeric.rs`: `Numeric { kind: Signed | Unsigned | Binary, bits }` with one table of the ten types: spelling, LLVM type, C type, device spelling, container tag, range. `names::INT` and `names::FLOAT` become two rows of it.
- [x] Every `is_named(INT) || is_named(FLOAT)`, `LType::I64 | LType::Double` and `Ty::Int | Ty::Float` branch that means "is this a number" asks the descriptor instead. `LType::Num(Numeric)` replaces `I64`, `I32` and `Double` (kept as named constants), and `gpu_device::Ty::Num(Numeric)` replaces its `Int`/`Float`.
- [x] Container tags (`llty.rs::elem_of_spelling`) spell every row, with distinct spellings for unsigned (`u8`, `u16`, `u32`) and `float16` (`f16`).
- [x] No behaviour change: GC corpus 306/306 with goldens, `cargo test -p osprey-types -p osprey-codegen` and workspace clippy green (2026-11-03).

**Gate:** `grep -rn 'names::FLOAT' crates | grep -v numeric.rs` finds only the table and rendering.

### Phase 2 — The integer family `[TYPE-NUMERIC-SIZED]` `[TYPE-NUMERIC-LITERAL]` `[TYPE-NUMERIC-ARITH-WIDTH]`

- [x] Type names resolve in both flavors and the Python flavor's annotations.
- [x] Integer literals carry an open numeric obligation that a slot of any integer type resolves and that defaults to `int`; a literal outside the resolved type's range is `literal 300 does not fit uint8`. Pattern literals follow the scrutinee.
- [x] `+ - * %`, unary `-`, `intDiv`, `abs`, the `wrap*`/`sat*`/`checked*` helpers and the bitwise builtins at each width: `llvm.{s,u}{add,sub,mul}.with.overflow.iN`, the same `Arith` requests, shift clamps at `bits`.
- [x] Comparisons and `==` at width; interpolation prints decimal; `[TYPE-NUMERIC-WORD]` boxing into list, map, `Result`, `any` and closure words with sign or zero extension and back.
- [x] `toInt`, `toInt8`, `toInt16`, `toInt32`, `toUint8`, `toUint16`, `toUint32` `[BUILTIN-CONVERT-INT]`, open over integer sources; narrowing out of range performs `Arith.overflow`.
- [x] LSP hover and completion docs, TextMate and tree-sitter highlight lists, `osprey-fmt` untouched (type names are identifiers).
- [x] Tests: `tests/regressions/basics/types/sized_integers.test.osp` and its ML twin — every type, both range ends, each operator, overflow under a handler, mixed-width rejection in `examples/failscompilation/sized_mixed_width.ospo`, out-of-range literal in `sized_literal_range.ospo`, each with an `ml_` fixture.

**Gate:** both twins byte-identical under GC, ARC and wasm32.

### Phase 3 — `float32` and `float16` on the host `[TYPE-NUMERIC-PRINT]` `[BUILTIN-CONVERT-FLOAT]` `[BUILTIN-FLOAT-BITS]`

- [x] Runtime conversions: `osp_float_to_f16` (from binary64, so there is one rounding) and `osp_f16_to_f32`, round to nearest even, subnormals, infinities and NaN exact, in commit 5d7b9fd3.
- [x] Runtime printing: `osp_float32_to_string` and `osp_float16_to_string` find the shortest decimal that round-trips at that width, tested on both the default and the portable build (`65500.0`, `1234568000.0`, `-0.0`).
- [x] Float literals stay open like integer literals: a `float32` or `float16` sibling or slot decides them, and between two literals or next to anything else they are `float`. Spec sentence added to `[TYPE-NUMERIC-LITERAL]`.
- [x] A float literal too big for its slot is rejected (`1e40` at `float32`; `70000.0` and `65520.0` at `float16`), through one check shared with integer literals.
- [x] `float32` with `int`, `float` or `float16` is a mismatch; `fn half(x) = x / 2.0` applied to a `float32` is rejected; the `[TYPE-NUMERIC-SAME-TYPE]` example is corrected.
- [x] `conv.rs` converts to and from `float32` and `float16`, with the quarantine removed.
- [x] One float arithmetic path: `+ - * / %`, comparison, unary minus and `abs` at each width, with `float16` through binary32 and a single final rounding.
- [x] `sqrt` and every other [BUILTIN-MATH] function open over floats, through the `f32` intrinsics and `libm`'s `f` entry points; an undecided operand defaults to `float`.
- [x] The compiler prints `float32` and `float16` through the runtime functions above, with the printing quarantine removed.
- [x] `toFloat`, `toFloat32`, `toFloat16` open over every numeric source; `floatToInt` open over float sources; `floatBits`/`floatFromBits` at 64, `float32Bits`/`float32FromBits` as `float32 ↔ int32`, `float16Bits`/`float16FromBits` as `float16 ↔ int16`, with docs for each.
- [x] One shared literal-fitting helper replaces `fit_literal` in `stmt/bindings.rs` and `fit_literal_arms` in `pattern.rs`, and covers float literals. It lands together with its callers: a callerless copy broke `make agent`.
- [x] Float literals reach the right width in list, map, `Result`, record and generic record values, closures and match arms; where the type is unknown the backend reports an error, never a wrong word.
- [x] Kernels over sized types: the device emitter declines them by name until Phase 5, so the host path runs; `toGpu` of `List<float32>` either works on the host or is rejected cleanly, and the spec says which.
- [x] Delete `float32(x: float) -> float`, `float32_round`, the old `float32Bits` lowering, `float32_64` in `gpu_f64.metal`, their docs pages, and `[BUILTIN-FLOAT32]` and `[BUILTIN-TOFLOAT]` citations in every crate.
- [x] Move to the typed forms: `math_builtins.test.*`.
- [x] Move to the typed forms: `packages/faiss`.
- [x] Move to the typed forms: `packages/safetensors` (`src/bytes.py`).
- [x] Move to the typed forms: `examples/python/ports/vector-search`, and anything else `grep -rn 'float32(' tests examples packages website` finds.
- [x] Tests: `tests/regressions/basics/types/sized_floats.test.osp` and twin — rounding boundaries, subnormals, `-0.0`, NaN comparison table at each width, printing, bit views, conversions in every direction, division by zero under a handler, literals in every container, builtins passed by name.
- [x] Tests: `sized_float_literal_range.ospo` and its `ml_` twin with goldens.
- [x] Review every line the agents wrote against this plan and correct it; `grep -rn 'names::FLOAT' crates` outside `numeric.rs` shows only rendering and float defaulting.
- [ ] A complete `cargo test -p osprey-cli` run. GPU cross-flavor IR regressions now pass 24/24. The next full run reached the corpus documentation-sigil gate; its two comments are fixed and that gate passes 4/4. The distinct-width record-field loader regression now passes independently; the final full rerun awaits completed Phase 4/5 changes (2026-11-03).
- [x] `cargo test -p osprey-types -p osprey-codegen`: 937 type tests and 221 codegen tests pass (2026-11-03).
- [x] `cargo clippy -p osprey-types -p osprey-codegen -p osprey-cli --all-targets -- -D warnings` passes (2026-11-03).
- [x] GC 312/312, ARC 312/312 and wasm 232/232 corpus lanes green with goldens, on the debug compiler (2026-11-03): release builds were on hold for another agent's suite.
- [ ] Account for the unique `wip/sized-numerics` changes on the feature branch, verify and push. Current repository instructions require the primary checkout only; the inactive historical worktree is retained solely until its unique changes have been accounted for, then removed in Phase 6.
**Gate:** twins green on all lanes; `grep -rn 'float32(' tests examples packages` finds no emulation call.

Sized division report follow-up (`docs/reviews/sized-float-polymorphic-arithmetic.md`):

- [x] Reproduce the inferred helper rejection; correct the per-call division result relation while retaining fixed-binary64 operand guards while preserving integer and mixed binary64 division. All 941 type-checker tests pass, including 20 mixed-width rejection controls; the unchanged cross-flavor IR parity gate passes (2026-11-04).
- [x] Verify the final bit-exact Default/ML regression under GC, ARC and wasm32, including aliases, subnormals, signed zero and local `Result` payloads; nine golden comparisons pass. Retain all 395 transformer package assertions under GC and ARC (zero live ARC objects). Receipts: `target/logs/plan-0050/polymorphic-verified-receipts.json` and `polymorphic-transformers-{gc,arc}.*` (2026-11-04).
- [x] Full Rust workspace tests and all Rust coverage floors pass on the fixed compiler: codegen 96.3% >= 95%, types 98.3% >= 98%; the 941 type tests, exact rejection goldens and declaration-documentation gates pass (2026-11-04).
- [x] Full `make test` passes (exit 0), including 318-program GC and ARC corpora, zero ARC leaks, all Rust/package/C/extension coverage floors, source extension suites (880 + 14) and installed VSIX tests (170). Receipt: `target/logs/plan-0050/polymorphic-make-test-exit.json` (2026-11-04).
- [x] Close the wasm gate: the wasm-only `tanhf` compatibility helper fixes the pre-existing ±0.5 rounding drift without changing native lowering. A ten-input loaded bit-oracle regression passes in both flavors under GC, ARC and wasm. Full wasm conformance passes 234 programs/goldens, zero leaks, 20 alternate GPU cases and six doctests, with the same 84 pinned unsupported programs; the wasm proof kernel passes all 400 cases. Receipts: `target/logs/plan-0050/wasm-tanh-{corpus-green.log,proof-green.log,cohort.json}` (2026-11-04).

- [x] Reject the CI-only tolerance relaxation from `1e-9` to `1e-6`: restored the unchanged transformer assertions and spec. With the wasm runtime fix, both flavors pass all 15 cases at `1e-9` on host, forced Metal and wasm, with exact goldens and zero native ARC leaks. Receipt: `target/logs/plan-0050/strict-transformer-receipts.json` (2026-11-04).

### Phase 4 — Dense buffers `[GPU-BUFFER-ELEM]` `[GPU-BUFFER-LOAD]` `[GPU-BUFFER-TENSOR]`

- [x] `OspreyGpuBuffer` gains `elem_bytes`; `data` becomes bytes. `OspreyGpuArg` carries bytes and an element size; `osprey_gpu_arg_bytes` uses it. Metal and OpenCL adapters upload and download by byte count. C/runtime and both mock-driver suites pass; real Metal verifies byte and binary32 transfers, including output guards. Parent independently verified 16 width/empty/transient/resident cases per mock driver with exact transfer lengths and untouched output canaries; a legacy eight-byte upload mutation is rejected (2026-11-03).
- [x] `toGpu`, `fromGpu`, `gpuGet`, `gpuIota`, `gpuLength`, `gpuScan`, `gpuFilter` and the host loops of every combinator read and write at the element's width; the codegen tag names it. The expanded `flavor_parity` twins pass all eleven scalar widths under GC and ARC, including inline/extracted lowering. Signed words are sign-extended when returning to lists; float32 scan/fold/filter/zip are verified. The unchanged cross-flavor allocation/fusion gates pass 24/24.
- [ ] `gpuLoad` returns `Result<GpuBuffer<float32>, Error>` with no widening. `gpuLoadTensor` returns `Result<GpuBuffer<T>, Error>` where the caller's `T` must match the tensor: `F64`→`float`, `F32` and `BF16`→`float32`, `F16`→`float16`; a mismatch is `tensor wte.weight is F16, the program asked for float32`. The `tensor_fixture` regression with two direct record-field calls requesting different widths independently passes with four-byte and two-byte results; complete loader/error verification remains open (2026-11-03).
- [x] Reject non-float `gpuLoadTensor` requests during type checking, including aliases and generic helpers. RED: `GpuBuffer<int8>` was accepted. GREEN independently on the primary checkout: all 36 direct/alias/helper cases pass, covering nine float controls and 27 unsupported requests. The result's float obligation survives generalization (2026-11-03).
- [ ] `examples/gpt2` (both flavors), `tests/core/gpu/transformer.test.*` and `@osprey/transformers` compute in `float32`, as llm.c does; new goldens, with the existing tolerance rule for `exp` and `tanh`.
- [ ] Tests: `tests/core/gpu/buffers.test.*` round-trips every element type through `toGpu`/`fromGpu`/`gpuGet`; `gpu_runtime_tests.c` checks byte counts, empty buffers and the loader errors; the ARC lane's zero-leak sentinel still passes.

**Gate:** `GpuBuffer<uint8>` of 1,000,000 elements allocates 1 MB, measured in `gpu_runtime_tests.c`.

Verified storage evidence, 2026-11-03: `memory_golden_tests.c` measures exact ARC bytes for widths 1/2/4/8; `gpu_runtime_tests.c` verifies truncation and adjacent elements. The real compiled `tensor_fixture::a_million_uint8_elements_allocate_one_megabyte_and_drop_cleanly` program measures 1,000,040 live bytes (1,000,000 payload + 40 header) under both lowering modes and exits with zero live objects. The remaining loader, GPT-2, device arithmetic and failure-handling items still gate phase completion.

### Phase 5 — Native device arithmetic `[GPU-DEVICE-NATIVE]` `[GPU-MATMUL]`

- [x] The dialect spells each row: `char uchar short ushort int uint long`, `half` storage read and written as `float`, native `float`, `ulong` for binary64. `FIXED_PARAMETERS` and `gpuGet` captures are typed per buffer. The existing eleven-width allocation IR test now also checks the second input, output and captured-buffer pointer spellings. Both corpus flavors round-trip identity and captured gathers at every width under host extraction, host inline lowering and actual Metal extraction, matching unchanged goldens with zero ARC objects; each Metal run records 26 dispatches on Apple M4 Max. Receipts: `native-width-typed-captures.log` and `native-width-gather-receipts.json` (2026-11-04).
- [x] Metal: `MTLCompileOptions` with safe math and `#pragma METAL fp contract(off)` in the library source; OpenCL: `#pragma OPENCL FP_CONTRACT OFF`. Verified 2026-11-03: Metal and OpenCL driver regressions failed before the fix; both pass after, as do 28,802,095 shader checks and the forced-Metal hardware suite (6,741 checks, including binary32 FMA and `2^24 + 1 - 2^24` discriminators) on Apple M4 Max. OpenCL hardware is not verified by its mock driver.
- [x] Checked integer ops at each width in `gpu_device_lib.metal`, one template instantiated per width, no copies. Primary review confirms seven integer rows × seven operations; the independent `__int128` oracle covers exhaustive byte pairs, wider boundary pairs and random values. The complete host shader suite passes 31,343,382 checks (2026-11-04).
- [ ] Remove GPU allocation/dispatch/readback/decline panic paths and fabricated empty allocation results; pin typed or handled failures and migrate their callers under [FAILURE-EXPLICIT]. Existing abort assertions are not evidence of this invariant.
- [x] `gpuMatMul` is open over `float` and `float32` with one element type for all three buffers; `osprey_matmul32` accumulates in binary32 in `k` order with no FMA; the order test at `2^24 + 1 - 2^24`. Verified 2026-11-03: host shader parity (28,802,102 checks), real Metal on Apple M4 Max (6,770 checks), and both compiled tensor-fixture flavors pass; separate rounding and ordered cancellation are checked by exact bytes. Receipts: `typed-matmul-{shader-green,metal-hardware,fixture}.log` under `target/local-coding-model/`. Parent independently repeats all 6,770 forced-Metal checks. Two independently reproduced RED regressions are now GREEN: the operand restriction survives direct, alias and helper calls across 18 cases, and the rejection retains its original line 5, column 4 site.
- [ ] Conversions in kernels: `toFloat32` of a `float` is `narrow32`, `toFloat` of a `float32` is `widen32` (both exist), every other conversion is the device's own cast.
- [ ] `gpu_shader_tests.c` compiles the dialect as host C and checks every `float32` operation, conversion and width-checked integer op bit for bit against the host; a forced Metal run of `tests/core/gpu/*` is recorded in the spec with the date and device.

**Gate:** `tests/core/gpu/mlkernels.test.*` over `float32` buffers produces the host golden under forced Metal, and the GPT-2 token time is measured and recorded.

Native-width integration, 2026-11-04: reviewed and integrated `db8d33d7`, `d66b08da` and the reserved-Metal-name fix `b0ea0e0f` on the primary checkout. All 221 codegen tests pass. The original float oracle compared several expressions with themselves: an addition-to-subtraction mutation incorrectly passed 933,185 checks. The strengthened oracle calls shared device arithmetic helpers against separately computed host results and derives comparisons from IEEE words; that mutation now fails, while the unchanged implementation passes 1,433,697 checks. Real Metal shader compilation first reproduced the reserved `half` identifier error, then all 6,770 device checks pass after the reviewed rename. Receipts: `target/logs/plan-0050/native-width-{primary-c-tests,primary-codegen,metal-before,metal-after}.log` and `native-width-mutation/`. This does not close the full emitted-kernel, conversion, failure-handling or timing gates.

The strengthened eleven-width gather twins also pass the unchanged cross-flavor IR parity gate. Final codegen tests (221), strict clippy and workspace format checks pass; compiler, runtime and regression hashes are recorded in `native-width-cohort.json`. The separately preserved forced-Metal/inline refusal is expected for the documented host-only inline oracle; the passing differential runs use both host lowerings and the actual device runs use extraction.

Verified binary32 progress, 2026-11-03: the compiled transformer suite passes all 15 cases on host and real Metal (Apple M4 Max), with zero live ARC objects; scan/filter also pass a compiled hardware regression. `MTLMathFloatingPointFunctionsPrecise` is now selected explicitly after a red driver regression. Precise Metal `exp`/`tanh` still differed from the host by one binary32 ulp, so those two functions use the shared wider approximation followed by one rounding; tensor arithmetic and storage remain binary32. Existing numerical assertions are unchanged. Receipts: `transformer32-metal-complete.{out,err}`, `sequence32-metal-hardware.log`, and `metal-precise-functions-{red,green}.log` in `target/local-coding-model/`. Extraction-floor, sized device operations, explicit failures and timing gates remain open.

### Phase 6 — Edges `[FFI-NUMERIC-WIDTH]`

- [x] `extern fn` parameters and returns of sized types map to the C types of the table (`int32_t`, `uint8_t`, `float`, `uint16_t` for `float16`); iOS and Android ABI tables (`osprey-cli/src/ios_abi.rs`) follow. Verified 2026-11-03: 38 host ABI tests, real C round-trips of all ten numeric types, and 9 backend symbolic/FFI checks pass; direct calls, extern function values and C callbacks preserve target extension attributes.
- [x] Python flavor: spec 0041 line 76 rewritten; annotations `int8` … `float32` accepted; port guidance in 0043 names `toFloat32` where it named `float32`. All ten numeric annotations parse and type-check in the host ABI regression (2026-11-03).
- [x] Python-hosted libraries: support sized numeric descriptors in ctypes and reject out-of-range host arguments before crossing the C boundary. RED `KeyError: int8`; all 20 real native-library interop tests pass after the fix (2026-11-03).
- [x] Verify the Python wrapper releases returned ARC-owned strings after copying. RED: 100 calls leaked 100 objects. GREEN: the committed UTF-8 regression reports zero live objects after both zero and 100 calls; all 35 Python tests pass (2026-11-03). The wrapper preserves the raw pointer and releases it in `finally` after copying.
- [x] Website: regenerate builtin pages (`make` target that owns `website/src/docs/functions`), delete the three emulation pages, add the conversion pages. `make -C website docs` regenerated 220 pages and ML twins; `float32.md` is gone and the bit-view pages now carry typed signatures (2026-11-03). Hover, generated reference parameters and rejection diagnostics now derive every accepted numeric width from the descriptor; all 245 LSP tests and the exact rejection-golden gate pass.
- [x] `deslop top-offenders` over `crates/osprey-codegen` and `compiler/runtime`: 117 Rust files scanned; the C scan finds zero supported files, an engine limitation requiring the separate manual review below (2026-11-03).
- [ ] Independently review runtime duplication where Deslop cannot scan C.
- [x] Remove the tensor reader's duplicate binary16 widening implementation. The existing numeric helper now serves it; a real safetensors file exercises all 65,536 binary16 patterns before and after the refactor, with 131,152 assertions passing. Finite bits, infinities, signed zeros and NaN class/sign are preserved; NaN payload canonicalization is not asserted (2026-11-03).
- [x] `coverage-thresholds.json` only rises: all existing keys remain and no threshold is lower than `origin/main`; the only additions are two C projects at 100% (2026-11-03).
- [x] `/spec-check` over numeric sections of 0004, 0012, 0019, 0041 and 0043. Corrected stale math signatures and C extension rules; math corpus runs and host ABI regressions verify the documented behavior (2026-11-03).
- [ ] `/spec-check` over GPU 0034 against the completed Phase 4/5 implementation and hardware receipts, then cross-check the complete numeric contract across all six specs.
- [x] Plan 0049 phase 1 delegates sized buffers and native arithmetic to this plan and waits for its Phase 5 gate; mapped weights, block quantization and mutable state remain with 0049. Verified against the current plan (2026-11-03).
- [ ] Merge `wip/sized-numerics` into the feature branch, push, delete that branch and its worktree `.claude/worktrees/sized-numerics`.
- [ ] Clean caches and build files left by this plan, and check free disk (98% full when Phase 3 started).

### Final verification

- [x] Derive CI checks from `ci.yml`, `ci-windows.yml`, `gpui.yml` and `winui.yml`, including their platform and coverage requirements (2026-11-03).
- [x] Run the CI scope, release-gate, skill-link, node-dependency, package-license, package-coverage wiring and Python app-port contracts. All eleven commands pass; exact commands, statuses and logs are in `target/logs/plan-0050/ci-contract-receipts.json` (2026-11-03).
- [x] Product-code reachability: `node scripts/verify-no-dead-code.mjs` finds all 2,715 public items reachable from product code; `ci-dead-code-contract.log` records the result (2026-11-03).
- [ ] Run final `make ci` against the completed implementation, with all required coverage thresholds intact.
- [ ] Run workflow checks beyond `make ci`, record host-only gaps explicitly, and verify the branch's required root `CI` check is green before integration.

**Gate:** `make ci` green; CI green on the branch.

## Risks

- **Literal typing touches inference everywhere.** The open-obligation design already exists for arithmetic (`deferred_arith`) and `abs`; literals reuse it. Phase 2 lands integer literals first and measures `cargo test -p osprey-types` time before and after.
- **ARC layout words.** Buffers hold raw bytes (`OSP_MEM_RAW`), so the width change does not touch layout words; lists keep their 8-byte words. The ARC lane's zero-leak sentinel is the check.
- **Device parity for transcendental functions** keeps today's tolerance rule; only `+ - * /`, `sqrt`, comparisons, conversions and the integer family are bit-exact contracts.
- **Goldens change** for GPT-2 and the transformer tests when they move to `float32`. Each new golden is produced on the host and confirmed on Metal before commit.

### Verification notes (2026-11-03)

Phase 6 spec review has checked numeric sections of 0004, 0012, 0019, 0041 and 0043 against the shared numeric descriptor, inference, arithmetic lowering, C thunks and regressions. Corrected stale binary64-only math signatures and the 32-bit C extension rule. Eight Default/ML GC/ARC math runs match their existing goldens; all four ARC runs report zero live objects. Spec 0034 and the final whole-plan audit await the Phase 4/5 implementation and hardware receipts.

Deslop 0.40.1 scanned 117 codegen files: 536/28,658 duplicated LOC (1.9%); the highest-ranked groups are test scaffolding and structural similarities. It scanned zero C runtime files because C is unsupported by the installed CLI: this is an engine limitation, not a clean-runtime result. The C runtime still requires independent duplication review. Coverage thresholds have not been lowered. Logs are under `target/logs/plan-0050/`.
