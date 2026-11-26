# Plan 0050 — Sized numeric types and native device arithmetic

Eight sized types exist beside `int` and `float` as ordinary primitives, `GpuBuffer<T>` stores `T` at its own width, and a `float32` kernel runs native binary32 on the device, bit for bit the host's result for `+ - * /`, `sqrt` and the comparisons; nothing widens silently and the emulation builtins are gone. The types, host arithmetic, dense buffers, device dialect, loaders and FFI are landed. What remains is the GPU failure-path migration under [FAILURE-EXPLICIT], the forced-Metal gate on the ML workloads, and the final verification.

Specs: [0004 Type System](../specs/language/0004-TypeSystem.md) (`TYPE-NUMERIC-*`, with the type table and the literal, overflow and conversion rules), [0012 Built-ins](../specs/runtime/0012-Built-InFunctions.md) (`BUILTIN-CONVERT-*`, `BUILTIN-FLOAT-BITS`), [0034 GPU computation](../specs/runtime/0034-GPUComputation.md) (`GPU-BUFFER-ELEM`, `GPU-DEVICE-NATIVE`) and [0019 FFI](../specs/compiler/0019-ForeignFunctionInterface.md) (`FFI-NUMERIC-WIDTH`). A change that affects a sized type moves every call site in `src/osprey/examples/`, `src/osprey/packages/` and `src/osprey/tests/` in the same commit. Out of scope, owned by [plan 0049](0049-local-coding-model.md): memory-mapped weight buffers, block-quantized weights and the single-owner mutable state buffer.

## GPU failure paths [FAILURE-EXPLICIT]

`toGpu`, `fromGpu`, `gpuIota`, `gpuMap`, `gpuZipWith`, `gpuFold`, `gpuScan`, `gpuFilter`, `gpuGet` and `gpuMatMul` return `Result`, and the corpus, fixture, app and documentation callers handle it. The runtime suites and the IR regressions `gpu_get_checks_readback_before_loading_a_scalar` and `gpu_materialization_checks_readback_before_loading_any_scalar` verify those gates only, not every language caller or real-device failure.

- [ ] Remove the remaining GPU allocation, dispatch, readback and decline panic paths and fabricated empty allocation results; pin typed or handled failures and migrate their callers. Check: `src/toolchain/compiler/runtime/gpu_*.c` and the decline paths of `osprey-codegen` hold no abort or exit outside a commented quarantine with its failing test, no failure returns a fabricated empty buffer, and every language caller propagates the producer's `Error`.

## Forced-Metal gate [GPU-DEVICE-NATIVE]

Both ML workload corpus twins (`src/osprey/tests/core/gpu/mlkernels.test.*`) compute in `float32` and pass all 17 cases on the host; on Metal each flavor passed 12 of 17 on 2026-11-12 (Apple M4 Max, gc and arc, zero live ARC objects).

- [ ] Device division by zero in cases 8, 11 and 12: make the device match the host's total result, or change the case's documented contract with the reason, under [ARITH-DEVICE-RECOVERY] in [plan 0055](0055-targets.md#gpu-computation-and-graphics-backends).
- [ ] `rowDot` declines in cases 15 and 16: give the kernel a device form.
- [ ] Twin parity on the device: the 2026-11-05 run showed the ML twin declining `degenerate` in cases 10 and 11 where the Default twin compiles it, and the 2026-11-12 run did not; twins must emit identical IR, so find out why they can diverge and add the IR-parity regression.
- [ ] `src/toolchain/compiler/runtime/gpu_shader_tests.c`: the full binary32 scalar-operation oracle, device half loads and stores, and the checked sized-integer operations, bit for bit against the host (spec 0034 lists them as this plan's remaining gates). Then record a forced Metal run of `src/osprey/tests/core/gpu/*` in the spec with the date and device, and run the same gates on OpenCL hardware or record in the spec why it is unavailable.
- [ ] Run the gate in both flavors under gc and arc on Metal and record the GPT-2 token time the gate requires. Check: `mlkernels.test.*` over `float32` buffers produces the host golden under forced Metal.

## Numeric contract

- [ ] (verify) A complete `cargo test -p osprey-cli` run on the final tree, with `cargo test -p osprey-types -p osprey-codegen` and `cargo clippy -p osprey-types -p osprey-codegen -p osprey-cli --all-targets -- -D warnings` green and `src/toolchain/crates/run_test_corpus.sh` green with goldens under GC, ARC and wasm32. The last complete runs predate the dense-buffer and native-device work.
- [ ] `/spec-check` over GPU 0034 against the implementation and hardware receipts, then cross-check the complete numeric contract across 0004, 0012, 0019, 0034, 0041 and 0043.

## Final verification

- [ ] (verify) Re-run the plan's one-time reviews on the final tree: deslop `top-offenders` over `osprey-codegen` and the C runtime (deslop cannot scan C; the manual review is issue #349) under the `.deslop.toml` ceiling; `src/osprey/tests/coverage-thresholds.json` not below `origin/main`; the CI scope, release-gate, skill-link, node-dependency, package-license, package-coverage wiring and Python app-port contracts; `node scripts/ci/verify-no-dead-code.mjs`.
- [ ] Run final `make ci` against the completed implementation, with all required coverage thresholds intact.
- [ ] Run the workflow checks beyond `make ci`, record host-only gaps explicitly, and verify the branch's required root `CI` check is green before integration. Gate: `make ci` green and CI green on the branch.
- [ ] Clean caches and build files left by this plan, and check free disk.
