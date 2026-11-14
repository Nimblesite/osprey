# Memory Management

Memory reclamation is an implementation detail. Osprey source cannot select
allocation sites, release values, or observe when a value is reclaimed. Native
builds default to tracing GC (`gc`); `arc` selects Perceus reference counting. WebAssembly, Apple and Android archives, and Python-hosted libraries default to ARC.
Unmanaged allocation requires explicit `--memory=none`. The old
`--memory=default` spelling is rejected. A third reclaiming backend,
compile-time ownership, is planned to replace `none`
([MEM-OWNERSHIP-STATIC](#compile-time-ownership-mem-ownership-static)).

## Collection Is Unobservable [MEM-OPAQUE]

Osprey has no finalizers or destructors. Source code cannot inspect addresses,
collector state, reference counts, collection order, or collection timing.

External resources such as files, sockets, processes, and foreign handles must
therefore be closed through their explicit APIs; losing the Osprey value does
not release the external resource.

## The Managed Value Heap Is Acyclic [MEM-ACYCLIC]

Managed Osprey values are immutable after construction. `mut` rebinds a name or
cell to a new value; it does not mutate an existing list, map, record, union, or
closure environment to point at a newer managed value. Consequently, the
managed heap cannot contain reference cycles and ARC does not need a cycle
collector. Raw pointers obtained through FFI are outside the managed heap.

## Ownership Lowering [MEM-OWNERSHIP]

Code generation tracks managed values in an ownership ledger:

- A fresh managed result enters its current region with one owned reference.
- Binding transfers a fresh owner or retains a borrowed value.
- Storing a managed value retains it unless a fresh owner can be moved into the
  destination.
- Region exits and proved last uses release their remaining owners.
- A returned owner transfers its reference; a returned borrow is retained.

The generated calls are identical for every backend. General retain/release
calls are no-ops under the unmanaged `none` and tracing-GC runtimes and active under ARC.
For a proved-unique value, codegen emits the paired `osp_release_unique` hook
with LLVM allocator/free attributes, allowing `-O2` to remove a non-escaping
allocation and release. The unmanaged `none` runtime also frees that unique value when
the pair survives optimization; tracing GC leaves it for the collector.

## Effect Continuation Threads [MEM-EFFECT-BATON]

A resumable handler runs its body on its own thread and passes one baton with its host: only the holder runs, and the others wait on a condvar. Tracing stays on across that handoff. A collection runs only while every other registered thread is parked, and scans each parked stack from its spilled registers up. Runtime blocks that hold managed pointers no stack names, such as handler stacks and operation mailboxes, are scanned as roots until they're freed. A thread outside the baton that allocates still disables collection for good, as [MEM-FIBER-ISOLATION] requires, and `OSPREY_GC_DEBUG=1` reports when that happens.

A body thread has 256 MiB of address space for its stack, committed as it's touched, so deep recursion inside a handler behaves like deep recursion outside one.

## Collecting While Fibers Run [MEM-GC-SAFEPOINT]

Fibers run at the same time, so tracing needs them all stopped. A thread that wants a collection asks, and the others stop at their next allocation or wherever they block in the runtime: `await`, a channel, `select` or `sleep`. Whichever arrives last collects, and each parked stack is scanned from its spilled registers up. A fiber's capture cell and result, and a channel's buffered values, are scanned blocks.

Asking never blocks for long, because a thread can be in a loop that never allocates, or in a system call waiting on the thread that asked. After 10 ms the collection is put off until more has been allocated, each time further away, up to 256 MiB, so a thread that won't stop costs memory, not progress.

Any other thread that allocates, such as a server's connection thread, still turns collection off for the rest of the process. Such a thread is not registered, so nothing can know its stack is still. On platforms with no exact query for a thread's stack bounds, fiber threads count as such a thread.

## Fiber Boundary Ownership [MEM-FIBER-ISOLATION]

`spawn` allocates a capture cell for that spawn and transfers one reference to
the runtime. The fiber releases the cell after its thunk returns. A managed
result is retained as a runtime root after the thunk returns. Every `await`
retains a separate reference for that receiving side, so repeated awaits are
safe. Once all spawned computations are quiescent, normal program teardown
releases runtime roots after language-owned values have dropped; teardown does
not invalidate cached results while another fiber can still await them. A
managed value boxed into a channel is likewise retained for the receiving side.

The runtime may therefore co-own managed allocations across fiber threads.
Before creating the first pthread-backed fiber it calls
`osp_mem_notify_multithreaded`: ARC switches from its single-threaded fast path
to synchronized retain/release operations, while the conservative GC disables
collection after a second allocator thread appears. Deterministic fiber mode
runs the same ownership protocol without creating a thread.

Under the planned compile-time ownership backend
([MEM-OWNERSHIP-STATIC](#compile-time-ownership-mem-ownership-static)) this
runtime co-ownership is replaced: ownership must transfer to exactly one
side, or repeated `await` of a managed result is rejected. That backend is
not implemented yet.

## Backend Selection [MEM-BACKENDS]

### Why GC is the native default

GC makes ordinary `osp_retain` and `osp_release` calls no-ops, reclaiming unreachable values during collection instead. ARC updates reference counts at ownership changes and releases an allocation when its last owner is dropped. Osprey defaults to GC to avoid that per-ownership-change bookkeeping as the starting policy for native programs; allocation tracking and collection still cost time and memory. This is a policy choice, not a claim that GC is faster than ARC on every workload.

The tradeoff is delayed reclamation, conservative retention and collection pauses. Choose `--memory=arc` when prompt reclamation or avoiding tracing pauses matters, then measure the application. Both backends are automatic and require no ownership annotations. Files, sockets and other external resources still need explicit cleanup under either backend.

The current GC is supported on Apple and glibc-Linux, not Windows, and stops collecting after a second allocator thread appears. Use ARC on Windows and for native programs that need reclamation across allocating fiber threads. WebAssembly, mobile and Python-library targets default to ARC and reject explicit tracing GC. Their managed runtime selection never falls back to unmanaged allocation. Exported host-library globals remain alive for later calls; `--entry-only` programs release globals on return, and native, WASM and mobile differential suites require zero live ARC allocations.

All code-generated heap allocation sites use the same IR symbols, so native
backend selection only changes the runtime archive passed to the linker:

- Explicit `--memory=none` uses `src/toolchain/compiler/runtime/memory_runtime.c`.
- `gc` uses `src/toolchain/compiler/runtime/memory_gc.c`.
- `arc` uses `src/toolchain/compiler/runtime/memory_arc.c`.

The C runtime memory tests exercise each backend directly. The differential
conformance targets exercise compiled Osprey programs and compare their output;
the ARC target additionally checks its live-allocation counter at process exit.

### Tracing Heap Layout [MEM-BACKENDS-GC-HEAP]

Today the tracing backend takes every block from the system allocator and records it in one open-addressing table keyed by the block's address. A collection marks through that table, frees the unmarked blocks one by one, and reseats the survivors in place. The table keeps its capacity between collections and shrinks only after a collection that peaked below it.

That layout is correct but slow for programs that allocate many short-lived small blocks. In the `binarytrees` benchmark the program's own code is about 5% of the run: the system allocator's `malloc` and `free` take about 38%, the table insert on each allocation about 28%, and collection about 25%. The run collects 133 times while about 640 KiB stays live. Resolving scanned words through the table is a small part of that (about 6%).

Planned, not built ([plan 0033](../../plans/0033-generated-code-performance.md), phase 2): small blocks come from size-class pages that the collector owns.

- **Allocation** takes the next free cell of a page of the block's size class. It makes no system allocator call and no table insert.
- **Lookup** of a scanned word finds its page from the address, then its cell from the offset in the page. A word keeps a block alive under exactly the rule the table applies today, so no program retains more or less than it does now.
- **Per-block state** (marked, finalizer, string, pinned) is held in the page, beside the cells and outside them.
- **Sweep** clears a page's unmarked cells for reuse and returns a page with no live cell to the operating system. Finalizers still run before any block of that collection is reused.
- **Large blocks**, above the largest size class, keep the system allocator and the table.

A cell freed by a collection is reused only by later allocations of the same size class. This replaces the allocator. It is not the recycling pool that was measured to make `binarytrees` about 12 times slower, which sat on top of the system allocator and cycled a cold working set.

The collection rules do not change: [MEM-OPAQUE], [MEM-GC-SAFEPOINT] and [MEM-EFFECT-BATON] hold as written, and the supported platforms stay the same.

### Container Element Ownership [MEM-BACKENDS-ELEMENTS]

Persistent container slots use an erased `int64` ABI, so the runtime cannot
infer whether a slot is a managed pointer from its bits. Lists carry an
`elem_managed` flag. Maps derive key ownership from the key type and carry a
separate managed-value flag. Codegen supplies these flags from static types.

- Insertion gives the container an owned key/value reference.
- Path copying and builders retain every managed key, value, or child they
  continue to share.
- Lookup, iteration, loop elements, and list-pattern heads borrow from the
  container; they do not create an owned reference by themselves.

### Backend Hook ABI [MEM-BACKENDS-CUSTOM]

The runtime archives implement the same internal C hooks: allocation,
tagged allocation, retain, release, proved-unique release, layout stamping,
finalizer registration, multithread notification, and collection. Backends
that do not use a hook implement it as a no-op. This ABI keeps emitted IR
independent of the selected native backend.

### Conservative Tracing GC [GC-TRACE-CONSERVATIVE]

The native GC records managed allocation base addresses and conservatively
scans the native stack, flushed registers, and data/BSS ranges. A word is a root
only when it equals a registered allocation base. A false positive can retain an
otherwise dead object but cannot make the collector free a reachable one. The
collector is non-moving.

Apple and glibc-Linux targets provide stack and executable data/BSS bounds.
Windows reads the stack's high bound from the TEB, covered by the caller-stack
root regression in the Windows workflow. Windows executable data/BSS ranges
are still not scanned, so the GC backend is **not part of the supported
Windows contract** until global-root discovery and its required Windows
regression are implemented. Windows native callers should select `--memory=arc` explicitly.

Collection is restricted to the initial allocator thread. The first allocation
from another thread permanently disables collection, while allocation-table
access remains synchronized.

### Perceus ARC [GC-ARC-PERCEUS]

The ownership and `dup`/`drop` discipline follows Perceus (Reinking, Xie, de Moura, Leijen), implemented from the extended technical report [MSR-TR-2020-42](https://www.microsoft.com/en-us/research/wp-content/uploads/2020/11/perceus-tr-v1.pdf) rather than the PLDI 2021 paper, because the report carries the full `λ₁` calculus and insertion algorithm.

The ARC backend stores a 16-byte header before each managed body: a layout word,
a signed reference count, and the body size. Retain/release first probe the live
allocation registry, so literals, foreign pointers, and other unmanaged
pointers are safe no-ops.

The layout word identifies raw blocks, pointer masks, list headers, pointer
arrays, and finalized list headers: a header whose body owns a resource beyond
the heap (a GPU buffer's copy on the device, [GPU-BUFFER-RESIDENT](../runtime/0034-GPUComputation.md#device-resident-buffers--gpu-buffer-resident))
passes through the one finalizer the owning runtime unit registers
(`osp_mem_set_finalizer`) before its data is released; the tracing collector
runs the same finalizer at the sweep that reclaims the header, and the unmanaged `none`
backend, which reclaims no general allocation, never runs it. Releases walk
managed child slots non-recursively. Persistent list and
map nodes retain shared structure and release the portion no longer referenced
by any live version. ARC operations start lock-free and become mutex-protected
before a pthread-backed fiber is spawned.

## Compile-Time Ownership [MEM-OWNERSHIP-STATIC]

**Planned; not in active work and not implemented.** `--memory=ownership` is
not accepted today. Delivery is tracked by
[plan 0045](../../plans/0045-compile-time-ownership.md).

Under this backend the compiler proves the last use of every managed value
and frees it there, as Rust's ownership and borrowing rules do, with no
collector and no reference count at run time. Osprey source gains no
lifetimes, borrow markers or ownership annotations: the ownership ledger
([MEM-OWNERSHIP]) infers owners and borrows, and a program whose ownership it
cannot prove is rejected at compile time with a diagnostic naming the value
and its conflicting uses. The acyclic heap ([MEM-ACYCLIC]) makes a single-owner
tree sufficient. Collection stays unobservable ([MEM-OPAQUE]): the backend
decides which programs compile and when memory is freed, never a compiled
program's output.

Design points still open for this backend; none is implemented yet:

- **Sharing.** Persistent lists and maps share structure between versions.
  Either the backend rejects programs that keep two live versions, or shared
  nodes are copied on the second use (an implicit clone the diagnostic can
  recommend instead).
- **Fibers and channels.** `spawn`, `await` and channel sends currently
  co-own values with the runtime
  ([MEM-FIBER-ISOLATION](#fiber-boundary-ownership-mem-fiber-isolation)).
  Ownership must transfer to exactly one side, or repeated `await` of a
  managed result is rejected.
- **Closures and effects.** Captured values, resumptions and handler state
  need an owner the analysis can see across `perform`/`resume`.
- **Host libraries.** Exported globals and C ABI results
  ([MEM-BACKENDS](#backend-selection-mem-backends)) need a documented
  transfer rule for WASM, iOS, Android and Python callers.
- **Diagnostics.** The rejection must say what to change in source with no
  annotation syntax available, for example binding a copy or restructuring
  the second use.

When shipped, `ownership` replaces the unmanaged `none` backend, which is then
deleted and its spelling rejected.
