# Memory Management

Memory reclamation is an implementation detail. Osprey source cannot select
allocation sites, release values, or observe when a value is reclaimed. Native
builds provide three link-time backends: `default`, `gc`, and `arc`.

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
calls are no-ops under the default and tracing-GC runtimes and active under ARC.
For a proved-unique value, codegen emits the paired `osp_release_unique` hook
with LLVM allocator/free attributes, allowing `-O2` to remove a non-escaping
allocation and release. The default runtime also frees that unique value when
the pair survives optimization; tracing GC leaves it for the collector.

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

## Backend Selection [MEM-BACKENDS]

All code-generated heap allocation sites use the same IR symbols, so native
backend selection only changes the runtime archive passed to the linker:

- `default` uses `compiler/runtime/memory_runtime.c`.
- `gc` uses `compiler/runtime/memory_gc.c`.
- `arc` uses `compiler/runtime/memory_arc.c`.

The C runtime memory tests exercise each backend directly. The differential
conformance targets exercise compiled Osprey programs and compare their output;
the ARC target additionally checks its live-allocation counter at process exit.

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
multithread notification, and collection. Backends that do not use a hook
implement it as a no-op. This ABI keeps emitted IR independent of
the selected native backend.

### Conservative Tracing GC [GC-TRACE-CONSERVATIVE]

The native GC records managed allocation base addresses and conservatively
scans the native stack, flushed registers, and data/BSS ranges. A word is a root
only when it equals a registered allocation base. A false positive can retain an
otherwise dead object but cannot make the collector free a reachable one. The
collector is non-moving.

Root discovery is implemented for Apple and glibc-Linux targets. On Windows
the stack base falls back to an address inside the collector's own frame and
data/BSS ranges are not scanned at all, so the GC backend is **not part of
the supported Windows contract** until `GetCurrentThreadStackLimits` (or TEB
bounds) and executable data/BSS bounds are implemented, with stack-root and
global-root regressions on a required Windows job.

Collection is restricted to the initial allocator thread. The first allocation
from another thread permanently disables collection, while allocation-table
access remains synchronized.

### Perceus ARC [GC-ARC-PERCEUS]

The ARC backend stores a 16-byte header before each managed body: a layout word,
a signed reference count, and the body size. Retain/release first probe the live
allocation registry, so literals, foreign pointers, and other unmanaged
pointers are safe no-ops.

The layout word identifies raw blocks, pointer masks, list headers, and pointer
arrays. Releases walk managed child slots non-recursively. Persistent list and
map nodes retain shared structure and release the portion no longer referenced
by any live version. ARC operations start lock-free and become mutex-protected
before a pthread-backed fiber is spawned.
