# Plan 0033: Generated-code performance foundation

Osprey's native binaries trail C by 1.5x to 5x on the integer-recursion benchmarks (`collatz`, `digitsum`, `fib`, `hanoi`, `isqrt`) and tie or win on the allocation-heavy ones. This plan builds the three things every later optimisation needs: an honest baseline, deterministic evidence of what LLVM did with the emitted module, and a module LLVM is allowed to optimise as a whole. It does not change arithmetic semantics; the one optimisation that would is recorded as a specification question.

## What the measurements say

Measured on 2026-10-17 on an Apple M-series laptop with `hyperfine -N`, 30 runs, Osprey at the default `-O2`, C at `cc -O2`. "C checked" is the same C program with every `+`, `-` and `*` routed through `__builtin_*_overflow` and an `abort()` on overflow, which is the closest C has to Osprey's checked arithmetic ([spec 0037](../specs/language/0037-ArithmeticEffects.md)).

| Case | C | C checked | Osprey |
|------|---|-----------|--------|
| digitsum | 6.4 ms | 15.9 ms | 15.4 ms |
| collatz | 11.7 ms | 62.5 ms | 67.9 ms |

Osprey is level with checked C. The gap to plain C is the overflow check: not the check itself, which is a predicted-not-taken branch, but what it stops LLVM doing. `1 + f(n)` with a plain add becomes a loop with a running total; with a checked add it stays a recursion, because reassociating the adds moves the point at which overflow fires.

The second experiment rewrote the emitted module so every function except `main` had `internal` linkage, which is what an executable's module should say, and changed nothing else:

| Case | Osprey | Osprey, internal linkage | C |
|------|--------|--------------------------|---|
| tak | 47.2 ms | 34.0 ms | 34.5 ms |
| coprime | 119.5 ms | 79.3 ms | 76.3 ms |
| primes | 13.4 ms | 8.3 ms | 7.1 ms |
| isqrt | 35.1 ms | 26.7 ms | 23.0 ms |
| powmod | 40.6 ms | 33.0 ms | 24.1 ms |
| gcdsum | 117.0 ms | 94.1 ms | 78.9 ms |
| hanoi | 113.2 ms | 93.0 ms | 72.3 ms |

Every emitted function is exported today, so LLVM must keep every definition and cannot specialise, drop or re-parameterise any of them. `tak` goes from 1.4x C to level with C from that one fact.

## Principles

- Every optimisation lands with a test that asserts what LLVM did with the module, not how long the binary ran. `clang -O2 -S -emit-llvm` on the emitted module is deterministic and runs on any runner.
- Wall-clock time is reported, never gated, on hosted runners. The gate uses a runner-independent measure (below).
- Arithmetic semantics are owned by spec 0037. No pass may change which operands an `Arith.overflow` handler observes without a specification change first.

## Phases

### Phase 1: honest baseline

`benchmarks/run.sh` gains a `rust-checked` column (`-C overflow-checks=on`) so the published table shows what checked arithmetic costs in a language that also checks, instead of comparing Osprey only against unchecked code. Done with this plan's first change.

### Phase 2: optimisation oracle

`crates/osprey-cli/src/tests/execution.rs` gains `optimised_ir`, which feeds the emitted module through the same clang driver the compiler uses at `-O2` and returns the optimised module. Tests assert its shape. The first red test, `an_executable_drops_a_helper_once_it_is_inlined`, pins Phase 3: the optimised module still defines `@add` after inlining it everywhere, because the definition is exported.

### Phase 3: whole-program linkage for executables

`CodegenOptions` gains a linkage choice. `compile_program` (native executables, wasm command modules) emits `define internal` for every function except `main` and the entry thunks; `compile_library` keeps external linkage for the C ABI exports (`[IOS-TARGET-ENTRY]`, `[WASM-WEB-ABI]`, the Python bridge). One emit site changes: `finish_function` in `builder/function_state.rs`. The symbol-name consumers to re-verify before this lands: the sampling profiler's frame walk (`[PROF-CODEGEN-FP]`, which resolves names through `dladdr`; internal symbols stay in the symbol table but the profiler tests must prove it), `--symbols`, DWARF (`DISubprogram` is independent of linkage), and the 54 IR assertions across 13 test files that spell an external `define`.

### Phase 4: cold slow paths

Every checked operation currently inlines a five-block slow path (handler lookup, environment lookup, scope suspend, call, restore). Outline it into one `cold` runtime call per operation so hot functions are small enough to inline and unroll. Expected gain is modest; the oracle test is that a hot function contains no `__osprey_handler_lookup` after `-O2`.

### Phase 5: accumulator recursion under checked arithmetic (specification question)

Turning `1 + count(n - 1)` into a loop reassociates the adds. Under spec 0037 an `overflow` handler receives the operands and the wrapped result and may resume with a value, so the rewrite is observable: the handler would see a different operand pair at a different moment. Options, to be settled in spec 0037 before any code: permit the rewrite only where the compiler proves no overflow can occur; or define that reassociation is permitted and the handler's operands are unspecified beyond the wrapped result; or leave the recursion as is and document the cost. Until that is settled there is no test for this phase.

### Phase 6: a performance gate that runs on hosted runners

Timing on `ubuntu-latest` is too noisy to gate on and the Mac runners are slower still. The gate measures instruction counts, which do not depend on runner speed: `valgrind --tool=cachegrind` on Linux reports `Ir` for a binary deterministically to well under 1%, at roughly 50x slowdown, which on 10 ms to 100 ms cases is seconds. The job builds the Osprey and `c`/`rust-checked` binaries for a fixed subset of cases, records `Ir` for each, and fails when an Osprey/`rust-checked` ratio rises above the value in a thresholds file that only goes down, in the manner of `coverage-thresholds.json`. It is a required check with a job-level `if:`, never `on: paths:`, per CLAUDE.md. Hardware counters (`perf stat -e instructions`) replace cachegrind if a dedicated runner is provisioned; the ratio file is the same.

## TODO

- [x] `rust-checked` benchmark column (`benchmarks/run.sh`, `benchmarks/report.py`, `benchmarks/README.md`).
- [x] `optimised_ir` oracle and the red linkage test in `crates/osprey-cli/src/tests/execution.rs`.
- [x] Phase 3 linkage change behind `CodegenOptions`; the red test goes green; profiler, `--symbols`, iOS, Android, Python and wasm web ABI tests re-run.
- [ ] Re-run `make -C benchmarks bench` after Phase 3 and update `website/src/benchmarks.md` from the recorded data.
- [ ] Phase 4 cold slow path with its oracle test.
- [ ] Phase 5 decision recorded in spec 0037.
- [ ] Phase 6 cachegrind job, thresholds file, entry in `scripts/verify-branch-protection.mjs`.
