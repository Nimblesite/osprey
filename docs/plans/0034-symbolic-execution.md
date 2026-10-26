# Plan 0034: Symbolic bug hunting with KLEE

**Subsystem:** `crates/osprey-cli` (`osprey hunt`), `crates/osprey-codegen` (`compile_program_symbolic`, synthetic entry), `compiler/runtime/klee/` (mock libc), `scripts/make/runtime.mk` (`_runtime_klee`), `scripts/hunt_corpus.sh` + `scripts/hunt_summary.py`, `tests/hunt/`, `benchmarks/symex/`
**Status:** Phases 1–2 delivered; Phase 3 is implemented with completion proof pending. `osprey hunt` runs KLEE over the mock-libc module and reports classified, deduplicated findings with native replay. The real planted-crash gate passes 15/15 cases, but the required clean regression still reports incomplete exploration at its unchanged 64-byte, 30-second settings (`target/hunt-profiler-2026-10-25/current-red.log`, verified 2026-10-25). Phases 4–6 remain: no typed harness, no `libkleeRuntest` replay, no regression writer, no Windows, no proof benchmarks. The corpus script and `tests/HUNT_UNPORTABLE.txt` exist; recorded budget-limited sweeps are not proof of complete exploration or a passing required corpus check.
**Specs:** [Symbolic bug hunting with KLEE](../specs/tooling/SymbolicExecution.md) (`[SYMEX-TOOL]`, `[SYMEX-INSTALL]`, `[SYMEX-CLI]`, `[SYMEX-KLEE-INVOCATION]`, `[SYMEX-MOCK-*]`, `[SYMEX-FINDINGS]`, `[SYMEX-REPLAY]`, `[SYMEX-REGRESSION]`, `[SYMEX-CORPUS]`, `[SYMEX-HOSTS]`, `[SYMEX-PROOF]`) and [KLEE code generation](../specs/compiler/KleeCodegen.md) (`[SYMEX-CODEGEN]`, `[SYMEX-HARNESS-TYPED]`, `[SYMEX-RUNTIME-BITCODE]`)

## Summary

Run KLEE on the LLVM IR the compiler already emits, with no annotations in the program, and get back Osprey source lines, triggering inputs and native reproduction for each finding, with a red regression test still to come. The system libc is replaced by a mock compiled to bitcode, so the same module runs on macOS and Linux (WSL2 to follow) without klee-uclibc or the POSIX runtime, which the macOS KLEE package does not build.

**Status (verified):** what runs today —

- `osprey hunt <file> [--max-time] [--max-memory] [--input-bytes] [--out] [--json] [--no-html] [--no-json] [--no-replay] [--quiet]`: the fixed KLEE command, findings classified and deduplicated with Osprey `file:line` and the recorded inputs, native replay telling `reproduced` from `klee-only`, exit codes 0/1/2 ([SYMEX-CLI](../specs/tooling/SymbolicExecution.md)).
- The mock C library in `compiler/runtime/klee/` and the runtime bitcode `_runtime_klee` (part of `make build`, with the `[SYMEX-MOCK-CLOSED]` check inside it).
- `compile_program_symbolic` codegen: debug metadata forced on, `abort()` on fatal paths.
- Mock unit tests `make _test_hunt_mock` (45 checks, part of `make test`); deterministic engine tests under `tests/hunt/` (`make _test_hunt`).
- `scripts/hunt_corpus.sh` + `scripts/hunt_summary.py`, wrapped by `make hunt FILE=` and `make hunt-corpus`; `tests/HUNT_UNPORTABLE.txt` skips listed programs.
- The VSCode command `osprey.hunt` ("Osprey: Hunt for Bugs") on the editor context menu and command palette: `osprey hunt <file> --json` on the active file, the report in the "Osprey Hunt" output channel, each finding a diagnostic at its line (source "Osprey Hunt (KLEE)").
- Not built: `--fn`/`--all-fns`, `libkleeRuntest` replay with the entropy/clock provider, `[SYMEX-REGRESSION]`, Windows (WSL2, `--replay-host=windows`), `[SYMEX-PROOF]`.

## Verified constraints

Checked against the tree and against KLEE's sources on 2026-10-24.

- `osprey --llvm` writes textual IR with no target triple (`write_ir` in `crates/osprey-cli/src/native.rs`). Codegen writes `ptr` in 64 places and `i8*` in 652, so the IR needs LLVM 15 or newer to parse. KLEE 3.3 requires LLVM ≥ 15 and supports up to 19.
- The only fatal path in codegen is `emit_unhandled_guard` in `crates/osprey-codegen/src/effects.rs`, which calls `exit(1)`. KLEE treats `exit` as a completed path, so under symbolic codegen it has to become `abort()`.
- The portable, pthread-free runtime subset is `WASM_RT_SRC` in `scripts/make/config.mk`; compiled against the macOS SDK it has 76 unresolved symbols, including Darwin-only `___memcpy_chk`, `___stderrp` and `___error`. Compiling with `--target=x86_64-unknown-linux-gnu -nostdlibinc` and the mock's own headers removes the Darwin set.
- `osp_input_byte` in `compiler/runtime/random_runtime.c` reads stdin one byte at a time with `read(0, …)`; `readFile` in `file_runtime.c` loops on `fread` until a short read. Both need a stream with a cursor, which is the one stateful mock.
- The native runtime archives are built with `-ftrapv -D_FORTIFY_SOURCE=2 -fstack-protector-strong` (`scripts/make/runtime.mk`). KLEE does not model the fortified `__*_chk` variants and reports signed overflow only through `-fsanitize=signed-integer-overflow` plus `--ubsan-runtime`, so the KLEE build uses different flags.
- Homebrew's `klee` 3.3 formula builds with `ENABLE_POSIX_RUNTIME=OFF` and `ENABLE_KLEE_UCLIBC=OFF`, and pins x86_64 on Linux because KLEE stalls on arm64 Linux. `brew` is not installed on the development machine, so the first toolchain step is installing it.
- No corpus program receives stdin: `run_test_corpus.sh` redirects it from `/dev/null`. The deterministic programs for this plan live under `tests/hunt/`, outside the golden corpus (which discovers only `*.test.osp`/`*.test.ospml`); the hunt sweep skips that directory and `failscompilation`, with `WASM_UNPORTABLE.txt`'s successor `HUNT_UNPORTABLE.txt` as its skip list.

## Phases

### Phase 1: mock library and runtime bitcode

`compiler/runtime/klee/` with the headers, the stream model, the stateless models and the unsupported stubs, plus the native unit-test driver `hunt_mock_tests.c`. `make _runtime_klee` compiles `WASM_RT_SRC` minus `web_runtime` and `wasm_builtins_runtime`, plus the mock, with KLEE's clang, and runs the `[SYMEX-MOCK-CLOSED]` check: `llvm-nm --undefined-only` over the linked bitcode, failing on any symbol KLEE does not itself provide. **Delivered.**

### Phase 2: symbolic codegen

`CodegenOptions` gained a `symbolic` flag, exposed as `compile_program_symbolic`; `emit_unhandled_guard` calls `abort`, debug metadata is forced on, no `optnone` is emitted. **Delivered**, except the `llvm-as` parse test over the corpus, which is not built — KLEE itself parses the module on every hunt.

### Phase 3: `osprey hunt` main mode

The subcommand in `osprey-cli`: compile, link, run the fixed KLEE command line, parse `test*.<suffix>.err` and the `.ktest` objects into findings, replay stdin and files natively, print the report and save HTML/JSON artifacts by default. **Implemented; the required deterministic completion gate remains red.** `tests/hunt/crash.osp` deliberately calls C `abort()` for input `"9"` and must reproduce `SIGABRT`. `clean.osp` is a no-finding control; `process.osp` pins an unsupported operation. `json_empty.osp` pins the fixed runtime defect that formed `NULL + 0` when closing an empty JSON container. A planted C failure demonstrates detection; it is not evidence of a new compiler defect.

### Phase 4: typed harness and `libkleeRuntest` replay

`__osprey_klee_entry` emission and the `osp_sym_*` builders; `--fn` and `--all-fns`; replay linking `libkleeRuntest` with `KTEST_FILE`; the entropy/clock replay provider.

### Phase 5: regression generation, corpus, Windows

`tests/regressions/klee/` writer; a fully completed `make hunt-corpus` run and a required check in `scripts/verify-branch-protection.mjs` (not done); WSL2 execution with `--replay-host=windows`. The recorded 224-program run found no findings, but 20 programs hit the budget. The corpus scripts exist; that bounded result does not complete this phase.

### Phase 6: proof

`benchmarks/symex/` with the eight cases in four languages, `run.sh`, `report.py` and a checked-in `results/results.json`. This phase closes last and gates the comparison table in the spec.

## What the first corpus sweep found (2026-10-24)

`scripts/hunt_corpus.sh --all` over 366 programs (tests/ and examples/, 20 s each, six in parallel) took about three minutes; 215 programs were explored completely and 92 hit the budget on a single long path. Verified outcomes:

- **Runtime bug, fixed:** `close_container` in `compiler/runtime/json_runtime.c` formed `NULL + 0` when closing an empty JSON container (`{}`, `[]`), a `memory` finding (UBSan pointer-overflow) at the Osprey `jsonParse` call in `json_document_query.test.osp`. Native builds ran clean because the arithmetic has no visible effect; `tests/hunt/json_empty.osp` now pins it.
- **Mock gap, fixed:** `_Thread_local` in `json_codec.c` lowered to `llvm.threadlocal.address`, which KLEE 3.3 does not implement; every `jsonEncode` path died as "unimplemented intrinsic". The bitcode is now built with `'-D_Thread_local='` beside `'-D__thread='`.
- **Report bugs, fixed:** prelude functions carry prelude line numbers (`PRELUDE_LINE_BASE`), which the report printed as `file:16777290`; findings now take the first user frame. KLEE 3.3 files a call outside the module under `--external-calls=none` as `user.err` ("external calls disallowed"), which was counted as incomplete; it is now an `unmodelled` finding.
- **Engine bounds, added:** a solver query on a large project module ran past `--max-time` and `--watchdog` for over ten minutes; every run now passes `--max-solver-time=30s`, the tool kills the engine after the budget plus 120 s, and a native replay that does not exit in 30 s is skipped.
- **Compiler bug, fixed (#293):** a record update whose new field value is a list literal of records (`box(items = [Item(...)])`) stored the literal's flat codegen-local layout where the field's `List<Item>` promised a runtime list, so the next `listGet` read a wrong slot and segfaulted. Hunt reported it on the issue's twelve-line repro as a `memory` finding at the `listGet` line, replayed and `reproduced (SIGSEGV)`, in under a second. The update path now settles field values the way construction does (`aggregate::settled_field_value`); `tests/regressions/basics/types/record_update_list_literal.test.osp` (both flavors) and `tests/hunt/record_update_list.osp` pin it.
- **Compiler bug, open:** `examples/tui/api_browser.osp` does not compile natively or symbolically (also on `main` before this work): `awaitSpinning(fib: Fiber, frame: int)` is a recursive generic whose return type inference stays a variable, so its instantiation cannot be emitted ("recursive and its return type is not inferred"); the example runs only through the manual `_tui` target. Minimal reproduction: `fn work() -> int = 42` / `fn awaitSpinning(fib: Fiber, frame: int) = match fiberDone(fib) { 1 => await(fib) _ => awaitSpinning(fib: fib, frame: frame) }` / `print("${awaitSpinning(fib: spawn work(), frame: 0)}")`.
- **Performance, observed:** interpretation cost on string-heavy tests is dominated by the byte loops of `strcpy`/`strcat`/`strlen` the string-concatenation lowering calls (about half of 367 M instructions on `list_basics.test.osp`, whose single path needs 86 s); the whole-program inlining codegen takes about 48 s on an 800-statement project before KLEE starts. Neither is a defect; both bound what a budget can explore.
- **Unportable by design:** 94 programs end their first path as `unmodelled` — process builtins (`pipe`), resumable effects and fibers (`__osprey_coro_new`, `fiber_spawn_env_owned`: the native fiber runtime is outside the module), HTTP (`http_create_server`), graphics, database, Python interop and platform examples — and are listed with the function named in `tests/HUNT_UNPORTABLE.txt`, together with the gpt2 modules, the deliberate type-error example and `api_browser.osp`.
- **Recorded corpus result:** after the fixes above, `scripts/hunt_corpus.sh` over the 224 remaining programs reports 0 findings; 204 are explored completely within 20 s and 20 hit the budget on a long single path (string-heavy tests). This is a bounded no-findings result, not a complete passing corpus gate. It is not yet a required CI check.

## What the second sweep found (2026-10-25)

The whole corpus again (343 programs: tests/, examples/, every registry and package project, 40 s each) plus 68 stress programs written for the purpose by pi agents (DeepSeek V4.1; GLM 5.3 ran out of its usage window) under `--input-bytes 16`, each reading stdin and steering branches, indices, loop counts and recursion depth from it. Verified outcomes:

- **Compiler bug, fixed:** a `mut` list literal that an effect handler arm appends to (`mut parts = ["seed"]` … `push s => { parts = listAppend(parts, s) }`) is promoted to a shared heap cell, and the cell stored the literal's flat codegen-local layout where the arm's `listAppend` expected a runtime list; the native program segfaulted. Hunt reported it on the stress program as two `memory` findings, replayed and `reproduced (SIGSEGV)`, with empty stdin. `stmt/cells.rs` now settles list literals before the cell's declaration store and every reassignment (`listlit::escaping`, the same conversion #293 needed for record fields); `tests/regressions/effects/handler_mutates_captured_list_literal.test.osp` (both flavors, identical IR) pins it.
- **Report bugs, fixed:** a project's findings were reported at the entry file with the flattened line (`main.osp:2172` for a 60-line file); the tool now maps the line back to the module file and its own line through the project metadata the checker's diagnostics use (`packages/manager/api` reports `core/src/policy.osp:21`). Two replay defects pinned by the spec-table tests: `unmodelled` findings stayed `not-run` when nothing else was replayable, and a program exiting with a status of 128 or more was reported as a signal; the verdict now reads the process's signal status.
- **Codegen gap, open (packages/manager/cli, packages/manager/e2e):** a field read through an untyped lambda parameter whose record three types share (`fn(e) => e.release.key`, `entryAt(xs, i).release`) is refused by codegen as "quarantined field access `release` on an untracked receiver" although the checker accepted the program; an annotated parameter compiles. Minimal repros that type the receiver through `filterList`, a declared return type or a record field all compile, so the untracked shape is narrower than the message suggests and still to be isolated.
- **Performance, measured:** on `tests/hunt/clean.osp` (one `parseInt(input())`) with a 30 s budget, `--input-bytes 2` explores 26 paths, all complete; 4 bytes gives 122 paths with 18 complete; 8, 16 and 64 bytes all give about 347 paths with 17 complete, because `parseInt` forks on every symbolic byte and the budget, not the input size, bounds the run. The 68 stress programs under 16 bytes each explored 180–750 paths and completed 0–51 inside 25 s. Small inputs (2–4 bytes) are what make a run finish; the default 64 is right for coverage of the first path. `listSort` over strings with a symbolic element forks slowest (45 paths in 25 s).
- **Language questions the stress corpus raised**, not defects of Hunt: a union variant's payload erased to `any` does not narrow through a brace pattern (`match forward(Circle { radius: 3 }) { { radius } => … }` takes the catch-all) while a declared record row does, which [PATTERN-STRUCTURAL] leaves open; a `List<Fiber<string>>` read back with `listGet` cannot be awaited ("fiber's result type did not reach `await`", the `api_browser.osp` family); a string literal nested inside an interpolation hole does not parse, which [STRING-INTERPOLATION] does not promise.
- **Invalid IR, caught (ARC frame slots, in-progress work):** on one stress program KLEE refused the module: `store i8** %arc.s0, i8*** %arc.fs0` precedes `%arc.s0 = alloca`, "Instruction does not dominate all uses!". The native pipeline never runs LLVM's verifier, so `--run` happened to work; `opt -passes=verify` on the native IR fails the same way. Reported to the session owning the uncommitted `arc.rs` change. Hunt's error excerpt now shows the head of KLEE's stderr too, where LLVM prints the fatal error, not only the stack-trace tail.
- **Engine ending, handled:** when a solver query is still running at the budget, KLEE's watchdog kills the engine and it exits non-zero; the tool reported that as a tool error and threw the run away. It now reads the states KLEE dumped on the way out and adds one "incomplete" entry naming the kill, so the run is cut, not failed (`engine::Ending`).
- **Stress corpus re-hunted with the fix, 4-byte input, 30 s:** 69 programs, 16,436 paths, 4,039 complete; 63 clean, 6 `unmodelled` by design (fibers, channels, `control` handlers), the two engine cases above. No other finding.
- **Recorded finding classes:** no `memory`, `abort` or `arith` finding in any of the 343 programs or the 67 other stress programs; the 95 programs that stop `unmodelled` are the fiber, process, HTTP, graphics and `control`-handler programs already listed, plus the registry projects that install `control` handlers. Unsupported operations and budget exhaustion prevent this from establishing complete corpus coverage.

## Known limitation: dynamic `control` handlers (reported 2026-10-25)

A driver that installs a `control` effect handler stops at the handler region's entry with `unmodelled: external call outside the module: __osprey_coro_new`, on empty input, and nothing inside the region is explored (reported from `packages/testing/hunt/arith`, `osprey hunt packages/testing/hunt/arith --max-time 60 --input-bytes 8`). Static effect handlers are unaffected (Vigil's `app/hunt` explored 2,529 paths). The cause is structural, not a missing stub: `effects_coro.c` runs each resumable handler region on its own pthread and hands control back and forth, and KLEE interprets one thread on one stack, so `pthread_create` and the coroutine entry points are unsupported stubs by design ([SYMEX-MOCK-UNSUPPORTED]). Modelling them means a different lowering of resumable regions for the symbolic build (one-shot resumption run to completion on the single stack, or a CPS form), which is Phase 4 work and is not scheduled. Until then such programs belong in `tests/HUNT_UNPORTABLE.txt`, and a hunt driver should keep its handlers static or wrap the region so the input-driven code runs outside it.

## Open questions

- Whether KLEE's LLVM 19 `clang` accepts the runtime's C as written, or whether a few `#if` guards are needed for the Linux-only configuration. Answered by Phase 1's first build.
- Whether Owi's WASI model is complete enough to run Osprey's `wasm32-wasip1` output without stubs. Only worth answering if KLEE blocks; recorded in `[SYMEX-TOOL]`.

## TODO

- [x] Phase 1: mock library with unit tests (`make _test_hunt_mock`, part of `make test`); `_runtime_klee` with the `[SYMEX-MOCK-CLOSED]` check inside it (part of `make build`).
- [x] Phase 2: symbolic codegen (`compile_program_symbolic`) with the IR tests in `crates/osprey-codegen/src/ir_tests/symbolic.rs`; the planned `llvm-as` corpus parse check is not built.
- [ ] Phase 3 completion proof: main mode and HTML/JSON artifacts are implemented, and planted-crash/native-replay tests pass. The deterministic `make _test_hunt` and default-budget regression must pass without incomplete exploration at their unchanged settings before this phase is complete.
- [ ] Known performance defect: `clean.osp` can exceed the unchanged exploration budget under GC and ARC. The original CI checks remain required; no known-bug exclusion is active. See the historical experiment report in `docs/reports/klee-gc-status-2026-10-26.md`.
- [ ] Phase 4: `--fn` / `--all-fns`; `libkleeRuntest` replay with the entropy/clock provider (native stdin/file replay ships without it); planted single-random-byte bug.
- [ ] Phase 5: regression writer; fully completed corpus made a required check (the scripts and recorded budget-limited sweeps exist, but that proof and gate do not); WSL2 and `--replay-host=windows`.
- [ ] Phase 6: `benchmarks/symex/` results meeting all four `[SYMEX-PROOF]` conditions; README rendered from JSON in `make ci`.
