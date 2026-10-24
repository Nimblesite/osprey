# KLEE code generation

**Status: symbolic codegen (`compile_program_symbolic`) and the runtime bitcode (`_runtime_klee`) are implemented; the synthetic entry point is not.** [Plan 0034](../../plans/0034-symbolic-execution.md) tracks delivery. The tool that consumes this output is specified in [Symbolic bug hunting with KLEE](../tooling/SymbolicExecution.md).

This document owns the compiler's side of symbolic bug hunting: the symbolic code generation mode, the synthetic entry point for function-level exploration, and the runtime bitcode archive. The mock C library, the `osprey hunt` command, findings, replay and the corpus are tooling contracts.

## Symbolic codegen `[SYMEX-CODEGEN]`

Symbolic codegen is `compile_program_symbolic` in `crates/osprey-codegen/src/lower.rs` — `CodegenOptions` with the `symbolic` flag set and a debug source always provided — called by the `osprey hunt` subcommand ([SYMEX-CLI](../tooling/SymbolicExecution.md#cli)); there is no `--symbolic` command-line flag. It is not a language change, and builds without it do not change. Under it:

- Every fatal path calls `abort()` after printing its message, never `exit()`. KLEE treats a clean `exit` as a completed path and writes no error file for it; `abort` is reported as `abort.err` ([KLEE termination types](https://github.com/klee/klee/blob/master/include/klee/Core/TerminationTypes.h)). Today the only fatal path in codegen is the unhandled-effect guard in [effects.rs](../../../crates/osprey-codegen/src/effects.rs), `emit_unhandled_guard`; new fatal paths follow the same rule.
- Debug metadata is always emitted, with the same content as `--debug` ([DEBUGGER-BUILD](DebugArtifacts.md#debug-build-contract-debugger-build)), so KLEE's error files and `run.istats` name Osprey lines.
- The module carries no `optnone` attribute and no `-O` passes have run on it. KLEE's documentation requires unoptimised input and runs its own passes under `--optimize`; `-O0` without `-Xclang -disable-O0-optnone` leaves `optnone` on every function, which stops those passes ([KLEE tutorial](https://klee-se.org/tutorials/testing-function/)).
- The IR parses with KLEE's LLVM: the module is textual `.ll`, which KLEE accepts directly, so an unparseable module fails the hunt itself. KLEE 3.3 requires LLVM 15 or newer and supports 15 to 19 ([release notes](https://github.com/klee/klee/releases/tag/v3.3)). The planned `llvm-as` parse check over the whole `tests/` corpus is not built.

## Synthetic entry point `[SYMEX-HARNESS-TYPED]`

**Not built.** There is no `--fn` or `--all-fns` today; `osprey hunt` explores from `main` only. The design:

With `--fn <name>`, codegen keeps the program's `main` and adds `define i32 @__osprey_klee_entry()`. The tool passes `--entry-point=__osprey_klee_entry` to KLEE, which starts execution there instead of at `main` (`--entry-point` in [KLEE options](https://klee-se.org/docs/options/)). KLEE calls the entry with `argc`/`argv`/`envp` and warns once about extra arguments; the warning is harmless.

The entry builds one value per parameter of `<name>` from the parameter's inferred type, calls the function, and passes the result to the printer `print` uses, so output paths run too. Value builders are C functions in the mock library, named `osp_sym_*`, each wrapping `klee_make_symbolic` or `klee_range` from `klee/klee.h`:

| Type | Builder |
|---|---|
| `int` | 64-bit `klee_make_symbolic` |
| `bool` | `klee_range(0, 2, name)` |
| `float` | Concrete samples from a fixed set (0, -0, ±1, ±inf, NaN, smallest subnormal), one KLEE run each, because KLEE concretises floating point |
| `string` | A byte stream ([SYMEX-MOCK-STREAM](../tooling/SymbolicExecution.md#byte-streams-the-one-stateful-model--symex-mock-stream)) turned into an Osprey string |
| `List<T>` | `klee_range(0, 5, name)` elements, each built recursively |
| `Map<K, V>` | `klee_range(0, 5, name)` entries over builders for K and V |
| Record | Each field built recursively |
| Union, `Result`, `Option` | `klee_range` over the variant count, then the chosen variant's payload |
| Unconstrained type variable | Instantiated at `int` |

Every symbolic object is named `<param>` or `<param>.<field>` so the `.ktest` objects and the replay report read as source names. Parameters of function type, effectful functions whose effects have no handler at the call, and recursive types deeper than 3 levels are skipped with a named reason, which `--all-fns` reports.

For native replay the same entry is emitted with the same builders; the replay build links KLEE's `libkleeRuntest`, which replaces `klee_make_symbolic` with the recorded value from the file named by `KTEST_FILE` ([KLEE tutorial, replaying a test case](https://klee-se.org/tutorials/testing-function/)). No second code path exists for concrete arguments.

## Runtime bitcode `[SYMEX-RUNTIME-BITCODE]`

`make _runtime_klee` builds `compiler/bin/libosprey_runtime_klee.bc` from the C runtime and the mock library. KLEE links it with `-link-llvm-lib=compiler/bin/libosprey_runtime_klee.bc`, the option KLEE provides for bitcode libraries ([Linking external libraries](https://klee-se.org/docs/options/)).

- **Sources:** the portable, pthread-free `WASM_RT_SRC` list from `scripts/make/config.mk`, minus `web_runtime` and `wasm_builtins_runtime`, plus `compiler/runtime/klee/*.c` minus the unit-test driver. The list is reused, not copied.
- **Compiler:** KLEE's own `clang` (Homebrew's `llvm@19`), found through `OSPREY_KLEE_LLVM` or the Homebrew prefixes, with the `klee` prefix through `OSPREY_KLEE_PREFIX` (`scripts/make/config.mk`). KLEE's LLVM cannot read bitcode written by a newer clang; the host's Apple clang 21 is not usable.
- **Flags:** `--target=x86_64-unknown-linux-gnu -nostdlibinc -isystem runtime/klee/include -emit-llvm -c -g -O0 -Xclang -disable-O0-optnone -std=c11 $(WARN_MAX) -U_FORTIFY_SOURCE -fno-stack-protector -fsanitize=undefined -fno-sanitize=function -DOSP_SYMBOLIC -DOSP_FLOAT_SPELLING_PORTABLE -D_GNU_SOURCE '-D__thread=' '-D_Thread_local='` (`KLEE_CFLAGS` in `scripts/make/config.mk`; `$(WARN_MAX)` is the shared hardened `-Werror` warning set; the mock's own `klee/klee.h` stands in for KLEE's header).
  - `'-D__thread=' '-D_Thread_local='`: KLEE 3.3 has no `llvm.threadlocal.address` intrinsic, so a thread-local variable ends the path with "unimplemented intrinsic". One thread explores the program, so plain globals are equivalent. The corpus sweep found this through `json_codec.c`'s `_Thread_local` writer table.
  - `-U_FORTIFY_SOURCE`: fortified variants such as `__fprintf_chk` are not modelled by KLEE and would be treated as external calls ([coreutils tutorial](https://klee-se.org/tutorials/testing-coreutils/)).
  - `-fsanitize=undefined -fno-sanitize=function` replaces the native archives' `-ftrapv`. KLEE handles the UBSan calls clang inserts when run with `--ubsan-runtime` and reports signed overflow as `overflow.err` instead of a trap; `function`, `local-bounds`, `objc-cast` and `vptr` are the checks KLEE does not support ([UBSan tutorial](https://klee-se.org/tutorials/using-ubsan/)).
  - One target triple and the mock's own headers on every host, so the bitcode does not depend on the host. Darwin symbols such as `___memcpy_chk` and `___stderrp`, which appear when compiling against the macOS SDK, never enter the module.
- **Closed module:** `llvm-nm --undefined-only` over the linked runtime bitcode must list only what KLEE itself provides — its intrinsics, UBSan handlers, allocator and `--libc=klee` — and the check runs inside `_runtime_klee`, failing the build on anything else ([SYMEX-MOCK-CLOSED](../tooling/SymbolicExecution.md#mock-c-library)).
- **Linux configuration:** Windows-only and Darwin-only `#if` branches of the runtime are not explored symbolically; native replay covers the host's own branches concretely.

## Verification

- `klee_mode_aborts_on_the_unhandled_effect_guard` (`crates/osprey-codegen/src/ir_tests/symbolic.rs`): the symbolic module contains `call void @abort()` and no `call void @exit` on the guard path, where the native build keeps `exit`.
- `klee_mode_carries_debug_metadata_without_debug_flag` (same file): the symbolic module has `!DICompileUnit` and `!DILocation` entries and no `optnone`.
- `wrapped_module_targets_x86_64_and_sets_the_stream_cap` and `program_module_carries_locations_for_findings` (`crates/osprey-cli/src/hunt/module.rs`): the wrapped module targets x86_64 Linux and appends `@osp_hunt_input_bytes`.
- The closed-module check runs inside `_runtime_klee` (`make build`) when the toolchain is present, as above.
- `hunt_tests.rs` (`crates/osprey-cli/src/hunt/`) runs the whole pipeline — compile, link, run, scan, replay, report, exit code — against a stand-in `klee`; the real engine is covered by the deterministic programs under `tests/hunt/`, and the mock library by `make _test_hunt_mock`.
- Not built: `klee_entry_is_built_from_inferred_types` (the harness) and `klee_corpus_parses_with_pinned_llvm` (the `llvm-as` corpus check).
