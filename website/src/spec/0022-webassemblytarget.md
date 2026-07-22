---
layout: page
title: "WebAssembly Target"
description: "Osprey Language Specification: WebAssembly Target"
date: 2026-07-22
tags: ["specification", "reference", "documentation"]
author: "Christian Findlay"
permalink: "/spec/0022-webassemblytarget/"
---

# WebAssembly Target

Osprey compiles to WebAssembly so programs run in the browser. The backend
reuses the existing LLVM-IR pipeline unchanged and adds a target selector
(`osprey --target=wasm32`), a wasm-portable runtime archive, and a `wasm-ld`
link step. The output is a `wasm32-wasip1` command module that runs under any
WASI host — `wasmtime`, Node's `node:wasi`, or a browser WASI shim. [WASM-TARGET]

> **Flavor layer — shared core (AST and above).**  The wasm backend lives
> entirely below the surface: it consumes the canonical `osprey_ast::Program`
> through the same LLVM-IR pipeline as the native target and never sees which
> [flavor](/spec/0023-languageflavors/) produced it. Because codegen is a pure
> function of the AST, identical ASTs yield byte-identical IR and `.wasm` —
> this is the [FLAVOR-IR-EQUIV] guarantee, so an ML `.ospml` program runs on
> wasm exactly like its `.osp` twin (the golden suite already proves this via
> the flavor-shared `<stem>.expectedoutput` fallback in Status).

## Status

Implemented for the portable language core. `osprey --target=wasm32 --compile`
emits a validated `.wasm` that prints correctly under wasmtime, Node's WASI, and
in the browser (`examples/wasm/`). Of the tested example suite, **47/70 run on
wasm with byte-identical stdout** — including the Default-flavor `.osp` twin of every ML example under
`examples/tested/ml/`, which the golden harness reaches via the flavor-shared
`<stem>.expectedoutput` fallback ([FLAVOR-IR-EQUIV]); the other 23 use a
non-portable feature and skip (see below). The CI `wasm` job gates on both the
browser-loadable example (`wasm-validate` + Node-WASI stdout) and the full
golden suite (`crates/diff_wasm_examples.sh`, FAIL=0).

Not yet ported (link-time `undefined symbol`, by design — see Limitations):
fibers/`spawn` (pthreads), HTTP/WebSocket (sockets/OpenSSL), FFI (`dlopen`),
the `random`/`input` builtins (CSPRNG/stdin syscalls), and **resumable effect
continuations** — the thread-based `__osprey_coro_*` runtime ([WASM-TARGET-EFFECTS]).

## Design

### Target triple: `wasm32-wasip1` [WASM-TARGET-TRIPLE]

`wasm32-wasip1` (the modern spelling of `wasm32-wasi`) is chosen over
`wasm32-unknown-unknown`. Osprey's runtime needs libc — `malloc`, `sprintf`,
`snprintf`, `strcmp`, `memcpy` — and a `stdout`. wasi-libc supplies all of them,
so the portable C runtime compiles **unchanged** and `print` works via WASI
`fd_write` with no hand-written libc. The browser still runs the module through
a small WASI shim (`examples/wasm/index.html`), so "needs WASI" does not mean
"can't run in a browser".

### Codegen is unchanged — the IR was already portable [WASM-TARGET-IR]

The textual LLVM IR carries no target triple or datalayout (clang supplies the
target's), `int` is `i64`, and pointers round-trip through `i64` (the uniform
machine-word boxing). On wasm32 a pointer is 32 bits, so that round-trip
zero-extends and truncates losslessly — addresses fit in 32 bits. No byte
offsets are hard-coded; LLVM computes struct layout per the target datalayout.
The backend needed **no** pointer-width refactor.

### Three width fixes (correctness on ILP32) [WASM-TARGET-WIDTH]

The IR baked in host type widths that differ on wasm32 (ILP32), each invisible
on LP64 (where `i8*` and `i64` are both 8 bytes) but wrong on wasm32:

1. **`size_t` (string length).** Codegen called libc `strlen`, declared `i64`,
   but `size_t` is 32-bit on wasm32 — a `wasm-ld` signature mismatch that traps.
   Fixed by routing the `length` builtin and string concatenation through a
   runtime shim `osp_strlen(const char*) -> int64_t`, so the `size_t -> int64`
   cast lives in C (correct per target) and the IR stays `i64` everywhere.
2. **`long` (integer formatting).** Integer→string used `sprintf("%ld", i64)`;
   on wasm32 `long` is 32-bit, truncating large values. Fixed by `"%lld"`
   (`long long` is 64-bit on every target; identical to `%ld` on LP64).
3. **`Result` success-slot type.** The `Result<T, E>` block is `{ T, i8 disc,
   i8* errmsg }`. An `Error { message }` constructor typed its success slot from
   the *message* (`i8*`), but a function declared `-> Result<int, _>` is read
   back with an `i64` slot. On LP64 both are 8 bytes so it worked by accident;
   on wasm32 the 4-byte `i8*` slot shifts the `disc`/`errmsg` offsets, flipping
   `Error` to `Success` with a garbage payload. Fixed by re-laying a returned
   `Result` to the declared success-slot type (`result::repack_to_inner`, called
   from `coerce_return`) so producer and reader agree on the layout.

All three are width-stable improvements that leave native output byte-identical
(the differential golden suite is unchanged at 48/48).

### Entry point: a `__main_void` thunk [WASM-ENTRY]

wasi-libc's `crt1-command.o` `_start` calls `__main_void`. The wasm driver
appends a thunk `define i32 @__main_void() { call i32 @main() ... }` to the IR,
sidestepping the clang/wasi-libc `main`-mangling skew. The result is a command
module (`_start` → `__wasm_call_ctors` → `__main_void` → `@main`) that runs
identically under wasmtime, Node's WASI, and a browser shim.

### Linking: drive `wasm-ld` directly [WASM-TARGET-LINK]

clang lowers the IR to a wasm object (`-c` only) and the driver then calls
`wasm-ld` itself: `crt1-command.o` + object + `libosprey_runtime_wasm.a`
(on-demand) + `-lc`. Going straight to `wasm-ld` avoids clang's auto-added
`libclang_rt.builtins-wasm32.a`, which stock Homebrew/apt LLVM doesn't ship and
which the portable core doesn't need (wasm has native `i64`). Sysroot and tool
locations are discovered from `OSPREY_WASI_SYSROOT` / `OSPREY_WASM_LD` or the
conventional Homebrew / wasi-sdk / Linux paths.

### Browser application ABI [WASM-WEB-ABI]

The wasm runtime provides a deliberately small message bridge so Osprey can own
application state and behaviour while React (or another JavaScript UI library)
owns the DOM. An Osprey web program declares the two host notifications as
ordinary externs; no JavaScript or React detail leaks into the language surface:

```osprey
extern fn osprey_web_render(message: string) -> int
extern fn osprey_web_command(message: string) -> int
```

`compiler/runtime/web_runtime.c` implements those functions and imports
`render(pointer)` and `command(pointer)` from the WebAssembly import module
`osprey_web`. Each message is a NUL-terminated UTF-8 string, conventionally
JSON. The JavaScript import must decode the message synchronously; the wrapper
then returns status `0` to Osprey. Because `web_runtime.o` is pulled from the
runtime archive only on demand, command-line wasm programs that do not call the
bridge acquire no `osprey_web` imports.

Browser events travel in the other direction through one optional convention:

```osprey
fn osprey_web_dispatch(message: string) -> int = {
    // decode the event, update Osprey state, then render
    0
}
```

The wasm linker always exports `osp_alloc`. When this function is present it
also exports a stable `osprey_web_dispatch`. Project assembly normally mangles
module function names, so the compiler scans the flattened AST for either the
source spelling or its mangled terminal segment and emits an `i8* -> i64`
forwarding thunk under the stable name. The browser therefore never needs to
know Osprey's project mangling. To dispatch an event, the host UTF-8-encodes its
JSON plus a trailing NUL, reserves that many bytes with `osp_alloc`, copies them
into exported linear memory, and calls `osprey_web_dispatch(pointer)`. At the
JavaScript API boundary `osp_alloc` is `(i64) -> i32`, so its byte-count argument
is a `BigInt`; the dispatcher's `i64` status is likewise returned as a `BigInt`.
Because allocation can grow linear memory, the host creates its `Uint8Array`
view from `memory.buffer` *after* the allocation.

This is an event/render boundary, not a per-DOM-node FFI. A render crosses wasm
once with a view model and React reconciles it entirely in JavaScript; an event
crosses once in the opposite direction. The bridge cost therefore scales with
message size and event frequency rather than component count.

### Runtime subset [WASM-TARGET-RUNTIME]

`make _runtime_wasm` cross-compiles the portable C units — allocator, strings,
list/map containers, JSON, the browser message bridge, and the effect *handler
stack* — to `libosprey_runtime_wasm.a` (the thread-based effect
*continuations* are split out; see [WASM-TARGET-EFFECTS]). The allocator is the
default `malloc`
passthrough (`@osp_alloc`); how memory is managed on wasm — and why the tracing
GC is excluded — is detailed in [WASM-TARGET-MEMORY] below. Non-portable units
(fibers, HTTP/WebSocket, system, terminal, FFI, CSPRNG) are excluded; because
archives link on demand, a program that does not reference their symbols links
cleanly, and one that does fails at link with a clear `undefined symbol`.

### Effects: handler stack portable, continuations native-only [WASM-TARGET-EFFECTS]

`effects_runtime.c` is the one runtime unit that is *partly* portable, so it is
split rather than excluded wholesale. The **handler stack** (push/pop/lookup of
`handle … in` scopes) needs only a mutex — no-op'd on single-threaded wasm — so
it compiles into the wasm archive and a program that merely installs handlers
links cleanly. The **resumable continuation** machinery (`__osprey_coro_*`)
implements `resume` by running the handled computation on its own `pthread` and
ping-ponging control through a condvar; `wasm32-wasip1` has no usable pthreads
(`pthread_create`/`pthread_cond_*`/`pthread_exit`), so that whole section is
guarded out with `#ifndef __wasm__`. With those symbols absent, a program that
actually resumes an effect link-fails on `__osprey_coro_suspend` and is SKIPped
by the golden suite — the same "non-portable feature ⇒ undefined symbol ⇒ skip"
contract used for fibers/HTTP. (`#include <stdint.h>` is explicit because the
wasi sysroot, unlike the host libc, does not transitively supply `int64_t`.)

**Assumption:** resumable algebraic effects on wasm wait on the precise,
thread-free continuation backend the ARC work unlocks ([WASM-TARGET-MEMORY-ARC]);
until then they are a documented wasm limitation, not a regression. The five
`effects/resume_*.osp` examples assert this by SKIPping on wasm while passing
natively.

### Memory management: linear memory now, ARC the wasm-friendly path [WASM-TARGET-MEMORY]

Three distinct things are easy to conflate. The wasm target uses the first,
cannot use the second, and deliberately does not use the third:

1. **Osprey's linear-memory allocator — what wasm uses today.** Exactly like the
   native *default* backend, the wasm runtime links the `malloc`-passthrough
   allocator (`@osp_alloc`, `compiler/runtime/memory_runtime.c`) over wasm linear
   memory. Reclamation is unobservable [MEM-OPAQUE]: an allocation lives for the
   run except where the optimizer statically frees a provably non-escaping value,
   so a long-running wasm program's heap grows like the native default's. This is
   a sound semantics choice, not a leak bug — see
   [spec 0018](/spec/0018-memorymanagement/).

2. **Osprey's tracing GC (`--memory=gc`) — native-only, NOT available on wasm.**
   The shipped conservative collector ([GC-TRACE-CONSERVATIVE], plan 0011) finds
   roots by scanning the C stack, the machine registers (flushed with `setjmp`)
   and the data/BSS segments, and serialises behind a `pthread` mutex. None of
   those exist under `wasm32-wasip1`: wasm has no addressable native stack or
   registers, no `setjmp` register spill to scan, and no pthreads. So
   `--memory=gc` does not combine with `--target=wasm32`, and the wasm runtime
   archive ships only the default allocator (no `memory_gc.o`). A *precise*
   collector — roots from an LLVM shadow stack ([GC-TRACE-CHENEY]) — could target
   wasm, but it is unbuilt.

3. **The WebAssembly GC proposal (Wasm-GC) — a different thing Osprey does not
   target.** "Wasm GC" means host-VM-managed heap objects (typed references,
   `struct.new` / `array.new`); it is orthogonal to Osprey's *own* collector.
   Osprey emits ordinary linear-memory wasm through the unchanged LLVM pipeline
   ([WASM-TARGET-IR]) and manages its own heap — it never lowers Osprey values to
   Wasm-GC types. Wasm-GC is a plausible *future* backend (the host VM would do
   the reclaiming, so no shipped Osprey collector would be needed), but it would
   require target-specific codegen the current design avoids and does not compose
   with the wasi-libc linear-memory model used here.

**ARC is the reclaiming backend that fits wasm [WASM-TARGET-MEMORY-ARC].** The
native ARC backend ([GC-ARC-PERCEUS], plan 0011 phase 2) is *precise* —
`osp_retain`/`osp_release` are compiler-inserted, so it needs none of the
stack/register/segment scanning, `setjmp`, or threads that bar the conservative
GC from wasm — and *complete*, because the value heap is acyclic [MEM-ACYCLIC],
so no cycle collector is required. The Perceus dup/drop pass is target-agnostic
codegen, but the Wasm build does not yet provide or select an ARC runtime
archive. Wiring that archive and forwarding the CLI memory selector would give
Wasm deterministic reclamation in plain linear memory without the Wasm-GC
proposal. Until then, Wasm uses the default allocate-until-instance-exit backend;
`--memory=arc --target=wasm32` does not change that archive today.

## Limitations

- **No fibers/socket HTTP/WebSocket/FFI/`random`/`input`/resumable effects.** These
  depend on pthreads / sockets / OpenSSL / `dlopen` / syscalls absent under
  `wasm32-wasip1`. A program using them fails at link, not silently. Effect
  *handlers* still work on wasm; only `resume`-based continuations are excluded
  ([WASM-TARGET-EFFECTS]). Browser-hosted UI messaging is available through the
  separate `osprey_web` ABI described above; it does not provide network
  sockets inside wasm.
- **WASI in the browser** needs a shim (`examples/wasm/wasi-shim.mjs`, loaded by
  `index.html` and exercised headlessly by `scripts/wasm-browser-smoke.mjs`),
  mapping `fd_write` to the page/console. A future `wasm32-unknown-unknown` mode
  could import I/O from JS directly.
- **No GC on wasm.** The tracing collector (`--memory=gc`) is native-only, so
  wasm reclaims nothing beyond the optimizer's static frees [MEM-OPAQUE], same as
  the native default. ARC ([GC-ARC-PERCEUS]) is the portable path to real
  reclamation — and the WebAssembly-GC proposal is not used. See
  [WASM-TARGET-MEMORY].

## Verification

- `osprey examples/wasm/hello.osp --target=wasm32 --compile -o hello.wasm`
- `wasm-validate hello.wasm` — structural well-formedness
- `node scripts/wasm-smoke.mjs hello.wasm examples/wasm/hello.expectedoutput`
  — runs under Node's WASI and asserts stdout
- `node scripts/wasm-browser-smoke.mjs hello.wasm examples/wasm/hello.expectedoutput`
  — runs under the browser's inline WASI shim (the exact module `index.html` uses)
- `examples/wasm/index.html` — loads and runs it in the browser, output to page
- `zsh crates/diff_wasm_examples.sh` — the golden suite: compile every tested
  example to wasm, run under Node's WASI, diff stdout; non-portable examples
  (undefined symbol) are SKIPped. Reports `PASS=47 FAIL=0 SKIP=23 NOEXP=0`.
- CI `wasm` job runs the validate + Node-WASI smoke **and** the golden suite on
  every PR.
- `cargo test -p osprey-cli wasm::tests` — asserts dispatcher discovery, stable
  thunk generation, and the exact `wasm-ld` export flags without requiring a
  browser host.