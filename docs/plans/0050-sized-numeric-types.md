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

- [ ] `float32` is LLVM `float`; `float16` is `i16` with `osp_f16_to_f32` / `osp_f32_to_f16` in `compiler/runtime/float_spelling.c`'s family, round to nearest even, subnormals, infinities and NaN exact.
- [ ] Arithmetic, comparison, unary minus, `abs`, `sqrt` and every [BUILTIN-MATH] function at `float32` through the `f32` intrinsics and `libm`'s `f` entry points; `float16` through binary32 and a final rounding.
- [ ] Printing: `osp_float32_to_string` and `osp_float16_to_string` find the shortest decimal that round-trips at that width, so `toFloat32(0.1)` prints `0.1`.
- [ ] `toFloat`, `toFloat32`, `toFloat16` open over every numeric source; `floatToInt` open over float sources; `floatBits`/`floatFromBits` at 64, `float32Bits`/`float32FromBits` as `float32 ↔ int32`, `float16Bits`/`float16FromBits` as `float16 ↔ int16`.
- [ ] Delete `float32(x: float) -> float`, `float32_round`, the old `float32Bits` lowering, `float32_64` in `gpu_f64.metal`, their docs pages and `[BUILTIN-FLOAT32]`. Move `math_builtins.test.*`, `packages/faiss`, `packages/safetensors` and `examples/python/ports/vector-search` to the typed forms.
- [ ] Tests: `tests/regressions/basics/types/sized_floats.test.osp` and twin — rounding boundaries, subnormals, `-0.0`, NaN comparison table at each width, printing, bit views, conversions in every direction; `sized_float_literal_range.ospo`.

**Gate:** twins green on all lanes; `grep -rn 'float32(' tests examples packages` finds no emulation call.

### Phase 4 — Dense buffers `[GPU-BUFFER-ELEM]` `[GPU-BUFFER-LOAD]` `[GPU-BUFFER-TENSOR]`

- [ ] `OspreyGpuBuffer` gains `elem_bytes`; `data` becomes bytes. `OspreyGpuArg` carries bytes and an element size; `osprey_gpu_arg_bytes` uses it. Metal and OpenCL adapters upload and download by byte count.
- [ ] `toGpu`, `fromGpu`, `gpuGet`, `gpuIota`, `gpuLength`, `gpuScan`, `gpuFilter` and the host loops of every combinator read and write at the element's width; the codegen tag names it.
- [ ] `gpuLoad` returns `Result<GpuBuffer<float32>, Error>` with no widening. `gpuLoadTensor` returns `Result<GpuBuffer<T>, Error>` where the caller's `T` must match the tensor: `F64`→`float`, `F32` and `BF16`→`float32`, `F16`→`float16`; a mismatch is `tensor wte.weight is F16, the program asked for float32`.
- [ ] `examples/gpt2` (both flavors), `tests/core/gpu/transformer.test.*` and `@osprey/transformers` compute in `float32`, as llm.c does; new goldens, with the existing tolerance rule for `exp` and `tanh`.
- [ ] Tests: `tests/core/gpu/buffers.test.*` round-trips every element type through `toGpu`/`fromGpu`/`gpuGet`; `gpu_runtime_tests.c` checks byte counts, empty buffers and the loader errors; the ARC lane's zero-leak sentinel still passes.

**Gate:** `GpuBuffer<uint8>` of 1,000,000 elements allocates 1 MB, measured in `gpu_runtime_tests.c`.

### Phase 5 — Native device arithmetic `[GPU-DEVICE-NATIVE]` `[GPU-MATMUL]`

- [ ] The dialect spells each row: `char uchar short ushort int uint long`, `half` storage read and written as `float`, native `float`, `ulong` for binary64. `FIXED_PARAMETERS` and `gpuGet` captures are typed per buffer.
- [ ] Metal: `MTLCompileOptions` with safe math and `#pragma METAL fp contract(off)` in the library source; OpenCL: `#pragma OPENCL FP_CONTRACT OFF`. Checked integer ops at each width in `gpu_device_lib.metal`, one template instantiated per width, no copies.
- [ ] `gpuMatMul` is open over `float` and `float32` with one element type for all three buffers; `osprey_matmul32` accumulates in binary32 in `k` order with no FMA; the order test at `2^24 + 1 - 2^24`.
- [ ] Conversions in kernels: `toFloat32` of a `float` is `narrow32`, `toFloat` of a `float32` is `widen32` (both exist), every other conversion is the device's own cast.
- [ ] `gpu_shader_tests.c` compiles the dialect as host C and checks every `float32` operation, conversion and width-checked integer op bit for bit against the host; a forced Metal run of `tests/core/gpu/*` is recorded in the spec with the date and device.

**Gate:** `tests/core/gpu/mlkernels.test.*` over `float32` buffers produces the host golden under forced Metal, and the GPT-2 token time is measured and recorded.

### Phase 6 — Edges `[FFI-NUMERIC-WIDTH]`

- [ ] `extern fn` parameters and returns of sized types map to the C types of the table (`int32_t`, `uint8_t`, `float`, `uint16_t` for `float16`); iOS and Android ABI tables (`osprey-cli/src/ios_abi.rs`) follow.
- [ ] Python flavor: spec 0041 line 76 rewritten; annotations `int8` … `float32` accepted; port guidance in 0043 names `toFloat32` where it named `float32`.
- [ ] Website: regenerate builtin pages (`make` target that owns `website/src/docs/functions`), delete the three emulation pages, add the conversion pages.
- [ ] `deslop top-offenders` over `crates/osprey-codegen` and `compiler/runtime`; `coverage-thresholds.json` only rises; `/spec-check` over 0004, 0012, 0019, 0034, 0041, 0043.
- [ ] Plan 0049 phase 1 loses its two storage items and cites this plan.

**Gate:** `make ci` green; CI green on the branch.

## Risks

- **Literal typing touches inference everywhere.** The open-obligation design already exists for arithmetic (`deferred_arith`) and `abs`; literals reuse it. Phase 2 lands integer literals first and measures `cargo test -p osprey-types` time before and after.
- **ARC layout words.** Buffers hold raw bytes (`OSP_MEM_RAW`), so the width change does not touch layout words; lists keep their 8-byte words. The ARC lane's zero-leak sentinel is the check.
- **Device parity for transcendental functions** keeps today's tolerance rule; only `+ - * /`, `sqrt`, comparisons, conversions and the integer family are bit-exact contracts.
- **Goldens change** for GPT-2 and the transformer tests when they move to `float32`. Each new golden is produced on the host and confirmed on Metal before commit.
