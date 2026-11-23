# Memory Management

Memory reclamation is an implementation detail. Osprey source cannot select
allocation sites, release values, or observe when a value is reclaimed. Native
builds default to tracing GC (`gc`); `arc` selects Perceus reference counting. WebAssembly, Apple and Android archives, and Python-hosted libraries default to ARC.
Unmanaged allocation requires explicit `--memory=none`. The old
`--memory=default` spelling is rejected. `--memory=ownership` accepts only programs the compiler can give one owner per
value, and will replace `none`
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
to synchronized retain/release operations, while the conservative GC takes its heap lock on every allocation and
collection from then on. A fiber or effect thread registers with the collector
and keeps collection on; the first allocation from a thread that never
registered turns collection off for the rest of the run ([MEM-EFFECT-BATON]). Deterministic fiber mode
runs the same ownership protocol without creating a thread.

Under compile-time ownership
([MEM-OWNERSHIP-STATIC](#compile-time-ownership-mem-ownership-static)) this
runtime co-ownership is replaced: ownership transfers to exactly one side and
a second `await` of a managed result is refused
([MEM-OWNERSHIP-STATIC-FIBERS]). The checking mode doesn't enforce the
`await` rule yet.

## Backend Selection [MEM-BACKENDS]

### Why GC is the native default

GC makes ordinary `osp_retain` and `osp_release` calls no-ops, reclaiming unreachable values during collection instead. ARC updates reference counts at ownership changes and releases an allocation when its last owner is dropped. Osprey defaults to GC to avoid that per-ownership-change bookkeeping as the starting policy for native programs; allocation tracking and collection still cost time and memory. This is a policy choice, not a claim that GC is faster than ARC on every workload.

The tradeoff is delayed reclamation, conservative retention and collection pauses. Choose `--memory=arc` when prompt reclamation or avoiding tracing pauses matters, then measure the application. Both backends are automatic and require no ownership annotations. Files, sockets and other external resources still need explicit cleanup under either backend.

The current GC is supported on Apple and glibc-Linux, not Windows. It stops collecting once a thread that never registered with it allocates ([MEM-EFFECT-BATON]). Use ARC on Windows and for native programs whose own threads allocate. WebAssembly, mobile and Python-library targets default to ARC and reject explicit tracing GC. Their managed runtime selection never falls back to unmanaged allocation. Exported host-library globals remain alive for later calls; `--entry-only` programs release globals on return, and native, WASM and mobile differential suites require zero live ARC allocations.

All code-generated heap allocation sites use the same IR symbols, so native
backend selection only changes the runtime archive passed to the linker:

- Explicit `--memory=none` uses `src/toolchain/compiler/runtime/memory_runtime.c`.
- `gc` uses `src/toolchain/compiler/runtime/memory_gc.c`.
- `arc` uses `src/toolchain/compiler/runtime/memory_arc.c`.

The C runtime memory tests exercise each backend directly. The differential
conformance targets exercise compiled Osprey programs and compare their output;
the ARC target additionally checks its live-allocation counter at process exit.
`OSPREY_ARC_DEBUG=1` arms that counter and prints the count of blocks still
live when the program exits; `OSPREY_ARC_DEBUG=2` also lists each survivor with
its kind, size and reference count. Unset, ARC keeps no counters.

### Tracing Heap Layout [MEM-BACKENDS-GC-HEAP]

The tracing backend keeps a block in one of two places, chosen by its size.

**Small blocks, up to 1024 bytes, are cells of size-class pages.** A page is a 64 KiB region, aligned to 64 KiB, that holds cells of one size. Cell sizes step by 16 bytes, so there are 64 classes, and every cell is 16-byte aligned.

- **Allocation** takes a cell from a page of the block's class that has room: first a cell a collection found dead, lowest address first, then a cell never used. It makes no system allocator call and no table insert. A new page is taken from the system only when no page of the class has room.
- **Lookup** of a scanned word masks the address to its page, then divides the offset by the cell size. The word names a block only when it is the first byte of a cell in use. A byte inside a cell, a dead cell, a cell never used and the page's own header name nothing, which is the rule the table applies, so no program retains more or less than it did with the table alone.
- **Per-block state** (size, marked, finalizer, string, pinned, leaf) is an 8-byte record per cell in an array at the head of the page, outside the cells. A dead cell's record links the page's dead cells, so a dead cell's own bytes are never written.
- **Sweep** unmarks a page's survivors and lists every other cell as dead. A page left with no live cell goes back to the system. Finalizers of every dying block, in pages and in the table, run before any block of that collection is freed or reused.

**Larger blocks keep the system allocator and one open-addressing table** keyed by the block's address. A collection frees the unmarked ones and reseats the survivors in place. The table keeps its capacity between collections and shrinks only after a collection that peaked below it.

A small block also becomes a table block when no page can be had (the system refused one), and always in a symbolic build (`OSP_SYMBOLIC`), where the engine must see each block as an object of its own. `OSPREY_GC_PAGES=0` turns the pages off for a run, so every block is a table block: it exists to compare the two layouts and for the runtime's out-of-memory tests, which refuse one system block by its size.

Measured on `binarytrees` (Apple M4 Max, same binary, `OSPREY_GC_PAGES=0` against the default): 1.37 s with the table alone, 0.25 s with pages. Before pages the program's own code was about 5% of that run: `malloc` and `free` took about 38%, the table insert on each allocation about 28%, and collection about 25%. `exprtree`, `mergesort`, `quicksort` and `wordfreq` are also faster and no benchmark case is slower. Peak resident memory moves with how full the pages are: `exprtree` falls from 22.7 MB to 11.7 MB, while `mergesort`, `quicksort` and `wordfreq` rise by about 5 MB, because a page with one live cell stays.

The collection budget's floor stays at 4 MiB (`OSPREY_GC_HEAP_MB`). With pages, 2 MiB halves `binarytrees`' peak memory (17.5 MB to 9.8 MB) but slows it from 0.25 s to 0.33 s, and 1 MiB slows it to 0.52 s.

An earlier recycling pool that sat on top of the system allocator made `binarytrees` about 12 times slower, because it cycled a cold working set. Pages replace the allocator for small blocks instead of adding a layer to it.

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

### Heap Ceiling [MEM-HEAP-CEILING]

The managed heap of a native program has a ceiling under both reclaiming backends. It is `OSPREY_HEAP_MAX_MB` mebibytes when that variable holds a positive number, and half of the machine's physical memory otherwise. Where the host does not report its physical memory (Windows and WebAssembly today) there is no ceiling unless the variable sets one.

The ceiling counts what the heap holds, not what the program has allocated in total:

- **GC** counts the bytes of every tracked block. Before a request would pass the ceiling the collector runs, so garbage is reclaimed first. While other threads are slow to park it asks again, for about a second in total.
- **ARC** counts the bytes it holds from the system allocator, which includes dead blocks parked for reuse. Before a request would pass the ceiling every parked block is returned to the system.

A request that still does not fit has two outcomes:

- A single block larger than the whole ceiling is refused. The allocator returns no block, as it does for any size the system cannot serve, and the runtime operation that asked reports its own typed error.
- Otherwise the heap is full of blocks still in use. The runtime writes one line to standard error and ends the process with status 71, after flushing standard output:

  ```text
  osprey: heap limit exceeded: backend=gc limit_bytes=67108864 peak_bytes=67102720 request_bytes=8192
  ```

  `peak_bytes` is the most the heap ever held and never exceeds `limit_bytes`.

Ending the process is a runtime quarantine, not a language operation: compiled code allocates at sites that have no failure branch, so a refusal there would be a store through a null pointer, and growing without a bound takes memory from every other process on the machine. Making exhaustion a value the program can handle needs allocation to be an effect, which is not planned.

Symbolic runs have no ceiling; the engine bounds their memory. Checked by `t_heap_ceiling` in `memory_gc_thread_tests.c` and `memory_arc_tests.c`, and by `a_heap_that_only_grows_ends_at_its_ceiling` in `src/toolchain/crates/osprey-cli/tests/fiber_gc_memory.rs`.

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

Any registered thread may collect once every other registered thread is parked
([MEM-GC-SAFEPOINT]). The first allocation from a thread that never registered
permanently disables collection, while heap access remains synchronized.

The trace keeps its pending blocks on a stack that grows on demand. When that
stack cannot get memory, the block that found no room is already marked, and the
collector scans every marked block again until a pass drops nothing. Running out
of memory during a collection therefore slows it and never frees a reachable
block (`mark_stack_refused_child` in `memory_gc_page_tests.c`).

#### Leaf blocks are marked, never scanned [GC-TRACE-LEAF]

A block whose body holds no managed pointer is a leaf: it is marked when a scan
reaches it, but its own words are never scanned. Strings are leaves, and so is
any block whose layout is set to `OSP_MEM_RAW` through `osp_mem_set_layout`,
which is how the list runtime declares a scalar-element leaf (the element kind
comes from the codegen's one `is_managed_ptr` predicate, the same flag ARC
depends on). A word in a leaf that happens to equal a block's address therefore
retains nothing. Without this, a heap of float sample buffers cost one table
lookup per word on every collection: the DAW's song renderer spent about 60% of
its time in `gc_scan_range`, which this rule cuts to under 5%. A block
allocated through plain `osp_alloc` keeps being scanned, because its contents
are not declared.

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

`--memory=ownership` accepts a program only when the compiler can give every
managed value exactly one owner, as Rust's ownership and borrowing rules do.
Osprey source gains no lifetimes, borrow markers or ownership annotations: the
ownership ledger ([MEM-OWNERSHIP]) infers owners and borrows. Delivery is
tracked by [plan 0045](../../plans/0045-compile-time-ownership.md).

**What is built.** The ledger runs in a checking mode. Wherever it would count
a reference at run time, because it cannot move the one reference that exists,
it reports the value instead, and a program with any report isn't compiled.
An accepted program therefore counts no reference to a source value: each one
is freed at the single release the ledger emits for its owner. The helper
functions the compiler synthesises for rendering and structural equality
still count references among their own temporaries; they keep no value
alive past their return. Until the
backend has a runtime of its own, an accepted program links the reference
counting runtime, where that single release frees the value. Runtime
collections still count the elements two versions share; the sharing rule
below isn't enforced yet.

**What is refused.** A value needs a second owner, and the program is
refused, when:

- a value read out of a temporary, a `mut` binding or a file-scope binding is
  given a name (anything else borrows, see
  [MEM-OWNERSHIP-STATIC-BORROWS](#inferred-borrows-mem-ownership-static-borrows));
- it is stored in a record, union, list, closure or cell while its owner keeps
  it;
- a function returns a parameter, a field of one, or a file-scope binding;
- one arm of a `match` yields a value something else still owns where another
  arm yields a new value;
- it is given to a function that takes ownership of that parameter while the
  caller reads it again;
- it is handed to a fiber, channel, handler or collection that keeps it;
- a file-scope binding holds it as well as its owner.

The diagnostic gives the statement, the function, the binding that names the
value when one does, which of these applies, and the other use that keeps the
value from moving: a later read with its place, or a second read in the same
statement.

```text
main.osp: 8:4: ownership: `made` needs a second owner in `main`: it is stored in a new value while its owner keeps it; it is read again at 9:4 [MEM-OWNERSHIP-STATIC]
```

A fresh value passed straight to its use is moved and never refused. A binding
read exactly once by a statement, on a path that runs once, and never read
after it gives its value to the record, list, closure or binding that takes
it there. A
function parameter is a borrow: the caller keeps the value alive for the
call, so reading a parameter or passing it on costs nothing. A function
result is owned by the caller. The acyclic heap ([MEM-ACYCLIC]) makes a
single-owner tree sufficient. Collection stays unobservable ([MEM-OPAQUE]):
the backend decides which programs compile and when memory is freed, never a
compiled program's output.

### Inferred borrows [MEM-OWNERSHIP-STATIC-BORROWS]

A `let` that names a value another binding owns, such as a field of a record
or an alias of a parameter, borrows it: the new binding holds no reference and
frees nothing. Every binding the value was read from stays alive until the
borrower's last read, so the owner is freed after the borrower, never before.

A binding can't borrow in two cases, and is refused as needing a second
owner. The first is a value read out of something the statement itself made,
as in `let left = make(1).left`: that temporary dies when the statement ends.
The second is a value read from a `mut` binding or a file-scope binding,
which can be replaced while the borrower still reads it. A borrower can be
read and passed to functions. Storing it, returning it or handing it to a
fiber needs an owner and is refused.

### Owned parameters [MEM-OWNERSHIP-STATIC-PARAMETERS]

A parameter is a borrow unless the function can return it. A function whose
result can be one of its parameters, as an accumulator or a function that picks
between its argument and a new value does, takes ownership of that parameter:
the caller gives a fresh value or a binding at its last use, and the function
returns the value or frees it. Giving a value the caller reads again is
refused at the call:

```text
main.osp: 13:4: ownership: `made` needs a second owner in `main`: it is given to a function that takes ownership of it; it is read again at 14:4 [MEM-OWNERSHIP-STATIC]
```

The rule is inferred, never written, and holds under reference counting too,
where it saves the count a returned parameter needed. It applies to a function
of an executable that is called only by name. A function passed as a value, a
generic function, and every function of a library a host calls keep borrowed
parameters, because their callers can't know which parameters are owned.

### Moves on one path [MEM-OWNERSHIP-STATIC-PATHS]

A binding may be given away where no path through the statement reads it
twice and nothing reads it afterwards. When only some paths give it away, as
when one arm of a `match` stores or returns it, the binding is released on the
other paths and not on that one.

### Rules for shared values [MEM-OWNERSHIP-STATIC-RULES]

These are decided. The checking mode enforces the ones stated under "What is
refused"; the rest are the contract the remaining phases build.

- **Persistent collections [MEM-OWNERSHIP-STATIC-SHARING].** Two live versions
  of a list or map are refused. An operation that derives a new version takes
  its argument by move when that is the argument's last use, and may then
  update it in place; when the argument is used again afterwards the program
  is refused. The compiler never inserts a copy, because a hidden copy of a
  whole collection would make cost unpredictable.
- **Fibers and channels [MEM-OWNERSHIP-STATIC-FIBERS].** Ownership transfers to
  exactly one side. `spawn` moves the managed values its body captures into the
  fiber. `await` moves the result to the awaiting side, so a fiber whose result
  is managed is awaited once and a second `await` is refused. `send` moves a
  value into the channel and `recv` moves it out. This replaces the runtime
  co-ownership of [MEM-FIBER-ISOLATION] under this backend.
- **Closures and effects [MEM-OWNERSHIP-STATIC-CLOSURES].** A closure that
  outlives the scope it is written in owns the managed values it captures, so
  capturing is a move and must be the captured binding's last use. A closure
  applied only inside that scope borrows them. Built: a closure written as an
  argument of a call to an Osprey function borrows what it captures. Its cell
  dies with the calling statement and the callee can't keep a parameter, so
  the cell owns none of its captures; binding, storing or returning it is
  refused. A closure inside a list that is passed, and one bound to a name
  before it is passed, still own their captures. A handler owns its state.
  `perform` lends its arguments to the handler until it resumes, and the
  resumed value moves to the performer. A continuation that owns managed
  values is resumed at most once.
- **Host libraries [MEM-OWNERSHIP-STATIC-HOSTS].** A managed result returned
  over the C ABI belongs to the host, which hands it back through the
  exported release function exactly once. Arguments a host passes are lent
  for the call. An exported global is owned by the module and lent to every
  caller. The rule is the same for WASM, iOS, Android and Python hosts.
- **Diagnostics [MEM-OWNERSHIP-STATIC-ADVICE].** With no annotation syntax,
  there are two changes a refused program can make: make the use that needs
  ownership the last use of the value, or build a separate value for that
  use. The diagnostic names the value and the use so either change is local.

When the backend has its own runtime, `ownership` replaces the unmanaged
`none` backend, which is then deleted and its spelling rejected.

