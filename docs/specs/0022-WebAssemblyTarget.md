# WebAssembly Target [WASM-TARGET]

`osprey --target=wasm32 --compile` emits a `wasm32-wasip1` command module.
The backend reuses Osprey's LLVM IR, compiles it with clang, and links it with
`wasm-ld`, wasi-libc, and the portable Osprey runtime archive.

The portable language core runs under a WASI host. CI compiles and validates the
hello fixture, runs it through Node's WASI host and the browser shim, then runs
`OSPREY_TARGET=wasm32 zsh crates/run_test_corpus.sh` — the same corpus harness
the native backend uses, pointed at the other code generator. It compiles every
program under `tests/` to wasm32, runs it under Node's WASI host, and compares
stdout byte-for-byte to the same `.expectedoutput` golden the native run must
match. The harness classifies an `undefined symbol` compile error as `SKIP` and
names each one; every other build or runtime error fails. CI requires
`TEST_CORPUS_FAIL=0`, `TEST_CORPUS_GOLDEN_FAIL=0` and
`TEST_CORPUS_GOLDEN_MISSING=0`, plus a golden floor so coverage cannot quietly
shrink — 107 on wasm32 and 160 natively (`crates/run_test_corpus.sh`,
`OSPREY_GOLDEN_MIN`). The floor ratchets up as goldens are added and is never
lowered to turn a red build green.

The wasm runtime includes strings, persistent collections, JSON, test and
coverage hooks, the effect-handler stack, profiler stubs, and the browser host
bridge. It excludes fibers, sockets, HTTP/WebSocket, process and file APIs,
terminal APIs, FFI, random/input, and resumable effect continuations. A program
that calls an excluded symbol fails at link time.

## Target Triple [WASM-TARGET-TRIPLE]

The target is `wasm32-wasip1` (the current spelling of `wasm32-wasi`). wasi-libc
provides the C allocation, string, formatting, memory, and stdout functions used
by the portable runtime. Browser execution uses the WASI Preview 1 shim in
`examples/wasm/wasi-shim.mjs`.

## Target-Neutral LLVM IR [WASM-TARGET-IR]

Osprey's textual IR contains no target triple or data layout; clang supplies
both. Osprey `int` remains `i64`. Heap handles round-trip through `i64`, so a
32-bit wasm pointer is zero-extended when boxed and truncated when recovered.
LLVM computes target-specific aggregate offsets.

## ILP32 Width Rules [WASM-TARGET-WIDTH]

Three paths require an explicit target-independent ABI:

1. String length and concatenation call `osp_strlen`, whose C implementation
   converts target `size_t` to Osprey `int` (`int64_t`).
2. Integer formatting uses `%lld`, because wasm32 `long` is 32-bit while Osprey
   `int` is 64-bit.
3. A returned `Result<T, E>` is repacked to the declared success-slot type so
   its discriminator and error-pointer offsets agree on both LP64 and ILP32.

## WASI Entry Point [WASM-ENTRY]

wasi-libc's command startup calls `__main_void`. The driver appends a thunk that
calls Osprey's `i32 @main()` and returns its status. The resulting module starts
through `_start` under wasmtime, Node's WASI implementation, or the browser
shim.

## Compile and Link Pipeline [WASM-TARGET-LINK]

The driver:

1. invokes clang with `--target=wasm32-wasip1 -O2 -c`;
2. locates the WASI sysroot from `OSPREY_WASI_SYSROOT`, `WASI_SDK_PATH`, or the
   supported platform paths;
3. invokes `wasm-ld` with `crt1-command.o`, the program object,
   `libosprey_runtime_wasm.a`, and wasi-libc.

`OSPREY_WASM_CC`, `OSPREY_WASM_LD`, and `OSPREY_WASM_RUN` override the clang,
linker, and runtime commands. Direct linking does not require clang's wasm
compiler-rt archive.

## Portable Runtime Archive [WASM-TARGET-RUNTIME]

`make _runtime_wasm` cross-compiles the units named by `WASM_RT_SRC` and creates
`compiler/bin/libosprey_runtime_wasm.a`. Static archive members are linked on
demand, so a command-line program that does not use the browser bridge has no
`osprey_web` imports.

## Browser Host ABI [WASM-WEB-ABI]

The optional browser bridge passes NUL-terminated UTF-8 messages, conventionally
JSON, across a coarse event/render boundary. Osprey declares:

```osprey
extern fn osprey_web_render(message: string) -> int
extern fn osprey_web_command(message: string) -> int
```

`compiler/runtime/web_runtime.c` imports `render(pointer)` and
`command(pointer)` from the `osprey_web` import module. The host must decode the
message synchronously; each Osprey wrapper returns status `0`.

A program may define the event entry point:

```osprey
fn osprey_web_dispatch(message: string) -> int = 0
```

The linker always exports `osp_alloc`. When the dispatcher exists, it also
exports `osprey_web_dispatch`. For projects, the driver discovers the flattened
mangled function and emits a stable forwarding thunk. The host allocates a
NUL-terminated message with `osp_alloc`, refreshes its view of `memory.buffer`,
copies the bytes, and calls the exported dispatcher. At the JavaScript boundary
the `i64` allocation size and dispatcher status use `BigInt`.

## Effect Support [WASM-TARGET-EFFECTS]

The handler-stack portion of `effects_runtime.c` is portable and is included in
the wasm archive. Resumable continuations use pthreads and are compiled out
under `__wasm__`; an expression that needs `__osprey_coro_*` therefore fails to
link. The golden harness classifies that known undefined-symbol case as `SKIP`.

## Memory Backend [WASM-TARGET-MEMORY]

The wasm archive contains `memory_runtime.c`, the same default allocator used by
native `--memory=default`. General releases do not reclaim aliased values, but
the compiler's proved-unique release hook frees uniquely consumed temporaries.
The wasm driver does not receive the parsed `--memory` value, so `--memory=gc`
and `--memory=arc` also link this default archive.

The native conservative collector is not in `WASM_RT_SRC`: it depends on native
stack/register/data-segment scanning, `setjmp`, and pthread synchronization.
Osprey also does not emit WebAssembly-GC reference types; its values live in
ordinary wasm linear memory.

## Verification

- `cargo test -p osprey-cli wasm::tests`
- `make wasm`
- `wasm-validate examples/wasm/build/hello.wasm`
- `node scripts/wasm-smoke.mjs examples/wasm/build/hello.wasm examples/wasm/hello.expectedoutput`
- `node scripts/wasm-browser-smoke.mjs examples/wasm/build/hello.wasm examples/wasm/hello.expectedoutput`
- `OSPREY_TARGET=wasm32 zsh crates/run_test_corpus.sh` (or `make _test_wasm_goldens`)

The CI `wasm` job runs the validate, Node-WASI, browser-shim, and golden-harness
checks with a pinned WASI sysroot.

Every check that *runs* a module under `node:wasi` requires **Node 24 or newer**.
Before 24 that host caches the module's memory backing store when the instance starts and never
refreshes it after `memory.grow`, so each WASI call a growing module makes
afterwards touches freed memory: on x86_64 a SIGSEGV inside node — no stderr, no
wasm trap — and where the stale page is still mapped, the module's output is
silently dropped instead. A twelve-line hand-written module that writes, grows
and writes again reproduces it, so the constraint is the host's, not this
target's. `scripts/wasm-browser-smoke.mjs` reads the memory afresh per call and
runs on any supported Node, which is what makes it a second, independent oracle
rather than a copy of the first.

**`[WASM-TARGET-NODE]`** The version rule lives in exactly one place:
`scripts/wasm-smoke.mjs` re-executes itself under a sound interpreter when the
one it was started with is too old — `OSPREY_NODE` when set, else the newest
qualifying `~/.nvm` install — and fails with the defect's own explanation only
when no such interpreter exists. Every caller therefore invokes a plain `node`:
the `Makefile` targets, the CI workflow, `crates/run_test_corpus.sh` and
osprey-cli's wasm end-to-end test carry no version logic of their own, so none
of them can drift from this one. A relaunched child is marked in its
environment, so a second too-old hop fails loudly rather than recursing.
