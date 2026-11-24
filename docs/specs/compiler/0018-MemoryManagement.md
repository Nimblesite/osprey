# Memory Management

Memory reclamation is an implementation detail. Osprey source cannot select
allocation sites, release values, or observe when a value is reclaimed. Native
builds default to tracing GC (`gc`); `arc` selects Perceus reference counting. WebAssembly, Apple and Android archives, and Python-hosted libraries default to ARC.
The CLI rejects `--memory=none` and `--memory=default`. `--memory=ownership` checks that each managed allocation has one owner
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

GC and ARC share this lowering. The collector ignores general retain/release calls. ARC uses them to track live references.
Compile-time ownership adds the checks and lowering specified in
[MEM-OWNERSHIP-STATIC](#compile-time-ownership-mem-ownership-static).
For a proved-unique value, codegen emits the paired `osp_release_unique` hook
with LLVM allocator/free attributes, allowing `-O2` to remove a non-escaping
allocation and release. Tracing GC reclaims surviving allocations during collection.

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

Heap allocation sites use the same IR symbols across backends. For these
backends, native selection changes the runtime archive passed to the linker:

- `ownership` uses `src/toolchain/compiler/runtime/memory_ownership.c`.
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

### Shared code and research basis [MEM-BACKENDS-SHARED]

The three backends share code wherever one implementation is right for all of them, and each follows the published technique for its kind in full. Sharing never weakens a technique: where the best algorithm for one backend differs, that backend has its own code, as each has its own allocator and reclamation today. Nothing requires the backends to be decoupled, and a copy of shared logic in two backends is a defect.

| Shared by | Code | Why one implementation is right |
| --- | --- | --- |
| all three | the hook ABI (`memory_hooks.h`), block layouts, the heap ceiling (`memory_heap_limit.h`) | The emitted IR and the limit a program may reach don't depend on how memory is reclaimed. |
| `arc` and `ownership` | the tracked heap (`memory_tracked_heap.h`): headers, the live-block registry, the non-recursive walk that frees a block's children | Both free a block at a point the compiler chose. ARC adds a count; ownership adds none. |
| `arc` and `ownership` | the ownership ledger in `osprey-codegen` (`arc/**`) | Both run the Perceus last-use analysis [Reinking 2021]. ARC emits `dup` where a second owner is needed; ownership refuses the program there. |
| all three | the list and map runtime | One persistent structure; each backend supplies allocation, release and the element ownership rule ([MEM-BACKENDS-ELEMENTS]). |

Each backend follows the research for its kind:

- **`gc`** is a conservative, non-moving mark-and-sweep collector in the line of McCarthy, "Recursive Functions of Symbolic Expressions and Their Computation by Machine, Part I" (Communications of the ACM 3(4), 1960, [doi:10.1145/367177.367199](https://doi.org/10.1145/367177.367199)) and Boehm and Weiser, "Garbage Collection in an Uncooperative Environment" (Software: Practice and Experience 18(9), 1988, [doi:10.1002/spe.4380180902](https://doi.org/10.1002/spe.4380180902)). Its guard against false retention, treating a word as a root only when it equals an allocation base, addresses the problem studied in Boehm, "Space Efficient Conservative Garbage Collection" (PLDI 1993, [doi:10.1145/173262.155109](https://doi.org/10.1145/173262.155109)).
- **`arc`** is reference counting, first described in Collins, "A Method for Overlapping and Erasure of Lists" (Communications of the ACM 3(12), 1960, [doi:10.1145/367487.367501](https://doi.org/10.1145/367487.367501)), made precise by Perceus ([GC-ARC-PERCEUS]).
- **`ownership`** follows [MEM-OWNERSHIP-STATIC-BASIS](#research-synthesis-mem-ownership-static-basis).

Tracing and counting are two views of one problem, and a design may combine them: Bacon, Cheng and Rajan, "A Unified Theory of Garbage Collection" (OOPSLA 2004, [doi:10.1145/1028976.1028982](https://doi.org/10.1145/1028976.1028982)). That is the ground for sharing code between the backends.

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

Reinking, Xie, de Moura and Leijen derive precise `dup`/`drop` insertion and reuse from a linear resource calculus. Osprey's ARC implementation follows the insertion algorithm in the extended technical report. [Perceus (PLDI 2021)](https://www.microsoft.com/en-us/research/wp-content/uploads/2021/06/perceus-pldi21.pdf) · [MSR-TR-2020-42](https://www.microsoft.com/en-us/research/wp-content/uploads/2020/11/perceus-tr-v1.pdf)

The ARC backend stores a 16-byte header before each managed body: a layout word,
a signed reference count, and the body size. Retain/release first probe the live
allocation registry, so literals, foreign pointers, and other unmanaged
pointers are safe no-ops.

The layout word identifies raw blocks, pointer masks, list headers, pointer
arrays, and finalized list headers: a header whose body owns a resource beyond
the heap (a GPU buffer's copy on the device, [GPU-BUFFER-RESIDENT](../runtime/0034-GPUComputation.md#device-resident-buffers--gpu-buffer-resident))
passes through the one finalizer the owning runtime unit registers
(`osp_mem_set_finalizer`) before its data is released; the tracing collector
runs the same finalizer at the sweep that reclaims the header. Releases walk
managed child slots non-recursively. Persistent list and
map nodes retain shared structure and release the portion no longer referenced
by any live version. ARC operations start lock-free and become mutex-protected
before a pthread-backed fiber is spawned.

## Compile-Time Ownership [MEM-OWNERSHIP-STATIC]

`--memory=ownership` accepts a program only when the compiler can prove one
owner per managed allocation and keep that allocation alive through all of
its borrows. The ownership ledger ([MEM-OWNERSHIP]) infers owners and borrows
from ordinary Osprey source. Delivery is
tracked by [plan 0045](../../plans/0045-compile-time-ownership.md).

**What is built.** The ledger runs in a checking mode. Wherever it would count
a reference at run time, because it cannot move the one reference that exists,
it reports the value instead, and a program with any report isn't compiled.
An accepted program therefore counts no reference to a source value: each one
is released by the owner that holds it at its last use. Accepted programs
link `memory_ownership.c`. Generated helpers and runtime collection internals
still have sharing cases tracked by plan 0045. The compiler enforces the
source-level sharing rule below:
an operation that derives a new version of a list or map (`listAppend`,
`listPrepend`, `listReverse`, `listSort`, `+`, `mapSet`, `mapRemove`,
`mapMerge`) takes its source by move and releases it as soon as the new
version exists, and a `...rest` pattern binding is a view lent by the matched
list, which takes nothing from its nodes and may not be stored, returned or
used to derive a version. A `fold` step owns the accumulator it is given.

**What is refused.** A value needs a second owner, and the program is
refused, when:

- a value read out of a temporary that only lends it, a `mut` binding or a
  file-scope binding is given a name (anything else borrows or moves, see
  [MEM-OWNERSHIP-STATIC-BORROWS](#inferred-borrows-mem-ownership-static-borrows));
- it is stored in a list, Result, anonymous record, cell or a fiber's closure
  while its owner keeps it (a named record or union constructor and a closure
  literal lend instead, see
  [MEM-OWNERSHIP-STATIC-BORROWS](#inferred-borrows-mem-ownership-static-borrows));
- a value that holds a lent field is returned, stored, bound at file scope or
  yielded by one arm of a `match` where another arm yields a new value;
- a function returns a borrowed parameter, a field that remains owned by
  that parameter's owner, or a file-scope binding;
- one arm of a `match` yields a value something else still owns where another
  arm yields a new value;
- it is given to a function that takes ownership of that parameter while the
  caller reads it again;
- it is handed to a fiber, channel, handler or collection that keeps it;
- a new version of a list or map is derived from it while another owner
  keeps it, or from a `...rest` view;
- a `mut` file-scope binding holds it as well as its owner (a binding that is
  never reassigned hands the module its one reference instead).

The diagnostic gives the statement, the function, the binding that names the
value when one does, which of these applies, and the other use that keeps the
value from moving: a later read with its place, or a second read in the same
statement.

```text
main.osp: 12:4: ownership: `made` needs a second owner in `main`: it is given to a function that takes ownership of it; it is read again at 13:4 [MEM-OWNERSHIP-STATIC]
```

A fresh value passed straight to its use is moved and never refused. A binding
read exactly once by a statement, on a path that runs once, and never read
after it gives its value to the record, list, closure or binding that takes
it there. The caller keeps borrowed arguments alive for the call and owns
the function result. Managed values can have several read-only aliases,
subject to the owner's lifetime. Collection stays unobservable
([MEM-OPAQUE]). A program accepted by multiple backends produces identical
output under each.

### Safety obligations [MEM-OWNERSHIP-STATIC-SAFETY]

- **Lifetime.** A borrow may be used only while its owner keeps the same
  allocation alive. Moving or replacing the owner, or reusing its storage,
  must preserve every live borrow. The checker must account for aliases of
  aggregate fields, borrowed call arguments and closure captures. Marshall
  and Orchard formalize the distinction between uniqueness and temporary
  shared access for functional languages. [Functional Ownership through Fractional Uniqueness (OOPSLA 2024)](https://kar.kent.ac.uk/105880/1/D.%20Orchard%20-%20Functional%20ownership%20through%20fractional%20uniqueness%20-%20PPDF.pdf)
- **Transfer and drop.** On each execution path an owned value is moved or
  freed exactly once by its current owner. At a control-flow join the
  compiler must account for the owners and borrows on each incoming edge.
  Reuse requires proof that no live alias would observe the old storage.
  The completed backend lowers those obligations to direct frees and moves
  without retain/release calls in ownership IR. Perceus supplies the
  last-use resource accounting that motivates these rules. [Perceus (PLDI 2021)](https://www.microsoft.com/en-us/research/wp-content/uploads/2021/06/perceus-pldi21.pdf)
- **Calls.** Every call must agree with the callee on which arguments move
  and which are lent. Indirect calls, partial applications and any call that
  borrows and consumes aliases to the same allocation need that proof too.
  Reject a transfer when the compiler cannot prove it. Ullrich and de Moura
  describe inferred parameter modes and explain the aliasing hazards at
  calls in an eager functional compiler. [Counting Immutable Beans, §5 (IFL 2019)](https://pp.ipd.kit.edu/uploads/publikationen/ullrich19counting.pdf)
- **Runtime boundary.** An allocating runtime or foreign helper must obey
  the ownership and lifetime contract of its interface. Foreign hosts must
  follow the documented transfer and release rules. RustBelt demonstrates
  how unsafe implementations carry a proof obligation at each safe
  interface, which is the model for these review boundaries. [RustBelt (POPL 2018)](https://people.mpi-sws.org/~dreyer/papers/rustbelt/paper.pdf)

These papers motivate Osprey's proof obligations. Verification of Osprey's
lowering and runtime is tracked in plan 0045.

### Verification [MEM-OWNERSHIP-STATIC-VERIFICATION]

On native and WASI targets, `--check --memory=ownership` applies the same ownership rules as LLVM emission and reports the same ownership diagnostics. Check-only compilation ends before linking or execution.

Check ownership and live borrows in the lowered program at transfers, frees,
storage reuse, call boundaries and every continuation exit. At each move,
the analysis must account for aliases of the allocation, including aliases
reachable through fields or captures. An unproved path is rejected before
emission. Exit-time counts cannot detect an earlier use-after-free or
double free. Exercise accepted paths with freed-memory checking and compare
their output with GC and ARC. Rejection tests must cover escaping borrows,
calls that alias an owner with a live borrow, and continuations that would
duplicate an owner. Acceptance tests must cover cleanup when a continuation
is discarded, as well as ordinary returns and successful resumption.

#### The ledger model [MEM-OWNERSHIP-STATIC-MODEL]

The rules of this backend are theorems about one transition system, stated in Lean and replayed by Osprey's proof kernel. [Model and proofs](../../../src/toolchain/benchmarks/proof/tools/OwnershipLedger.lean), [proof gate](../../../scripts/proof/ownership-ledger.sh).

**State.** Let S be the set of slots: a binding, a temporary, a record or union field, a closure capture, a caller. An allocation is a cell c = (live, O, L) with live ∈ 𝔹, owners O ∈ S\*, and loans L ⊆ S × S, where (b, o) ∈ L reads "slot b borrows from owner o". The freed cell is ⊥ = (false, [], ∅) and own(o) = (true, [o], ∅). A heap is a map h : ℕ → Cell.

**Invariant.** Valid(c) ⟺ (live ∧ ∃ o. O = [o] ∧ ∀ (b, o′) ∈ L. o′ = o) ∨ (¬live ∧ O = [] ∧ L = ∅). Safe(h) ⟺ ∀ a. Valid(h(a)). A live allocation has exactly one owner and every loan names it; a freed one has no owner and no loan.

**Transitions.** step : Cell × Action ⇀ Cell is partial. Where the guard fails the compiler refuses the program.

| Action | Guard | Result | Lowering |
| --- | --- | --- | --- |
| alloc(o) | c = ⊥ | own(o) | A constructor, literal or call result registers its owner. |
| borrow(b, o) | live ∧ O = [o] ∧ b ≠ o ∧ ∀ (b′, \_) ∈ L. b′ ≠ b | (live, O, L ∪ {(b, o)}) | A `let` alias, a lent field, a lent capture ([MEM-OWNERSHIP-STATIC-BORROWS]). |
| read(s) | live ∧ (s ∈ O ∨ ∃ o. (s, o) ∈ L) | c | Any use of a value. |
| endBorrow(b) | none | (live, O, L ∖ {(b, o) : o ∈ S}) | The borrower's last read; the death of a holder. |
| move(s, d) | c = own(s) | own(d) | A last-use move, a return, a field taken out of a temporary ([MEM-OWNERSHIP-STATIC-PATHS]). |
| free(o) | c = own(o) | ⊥ | A last-use release or a region exit. |

A path h ⟶\* h′ applies accepted steps to single cells of a heap, in any order, along either incoming edge of a join.

**Theorems.** All 18 are checked by Lean 4.25 and replayed by the GC, ARC and WASI builds of Osprey's proof kernel, which must each answer `PASS` with the listed axioms and no others.

| Theorem | Statement | Rule it discharges | Axioms |
| --- | --- | --- | --- |
| `step_preserves` | Valid(c) ∧ step(c, a) = c′ ⟹ Valid(c′) | Every accepted action keeps one owner and live lenders. | propext, Quot.sound |
| `from_empty` | (λ\_. ⊥) ⟶\* h ⟹ Safe(h) | Every heap an accepted program reaches is safe. | propext, Quot.sound |
| `dead_rejects` | step(⊥, a) = c′ ⟹ a ∈ {alloc(o), endBorrow(b)} | No read, borrow, move or free after a free: no use after free, no double free. | propext, Quot.sound |
| `lent_pins_owner` | step(c, borrow(f, o)) = c′ ⟹ ∀ d. step(c′, free(o)) = ∅ ∧ step(c′, move(o, d)) = ∅ | A lent field or capture pins its lender, so a lending value can't be returned, stored or sent past its lender's region. | propext |
| `lent_field_not_freed` | step(c, borrow(f, o)) = c′ ⟹ step(c′, free(f)) = ∅ | A holder frees nothing through a lent field: the field is left out of its drop walk. | propext |
| `lent_then_released` | f ≠ o ⟹ ⊥ ─alloc(o)·borrow(f, o)·read(f)·endBorrow(f)·free(o)→ ⊥ | A lent field is read, its holder dies, and the owner frees the value once. | propext, Quot.sound, Classical.choice |
| `lent_twice_released` | l ≠ o ∧ r ≠ o ∧ r ≠ l ⟹ ⊥ ─alloc(o)·borrow(l, o)·borrow(r, o)·read(l)·read(r)·endBorrow(l)·endBorrow(r)·free(o)→ ⊥ | One value in two fields, or in two holders, is freed once. | propext, Quot.sound, Classical.choice |
| `take_moves_owner` | step(own(f), move(f, t)) = own(t) | Taking a field changes the value's one owner from the holder's slot to the taker. | propext |
| `taken_field_not_freed` | f ≠ t ⟹ step(own(t), free(f)) = ∅ | After a take the holder must not free through that slot, so the compiler clears it. | propext |
| `take_needs_no_loan` | step(c, borrow(b, f)) = c′ ⟹ ∀ t. step(c′, move(f, t)) = ∅ | A field can't be taken while anything borrows the value. | propext |
| `taken_freed_once` | ⊥ ─alloc(f)·move(f, t)·free(t)→ ⊥ | A taken field is freed once, by its taker. | propext, Quot.sound |
| `edge_frames` | a′ ≠ a ⟹ h[a ↦ c′](a′) = h(a′) | A step on one allocation changes no other, so a holder moved into another holder keeps its loans and the outer holder lends in turn. | propext |
| `only_free_kills` | live(c) ∧ step(c, a) = c′ ∧ ¬live(c′) ⟹ ∃ o. a = free(o) ∧ c = own(o) | Every death is one free, by the owner, of a value nothing borrows ([MEM-OWNERSHIP-STATIC-SAFETY], transfer and drop). | propext |
| `given_is_gone` | g ≠ t ⟹ ∀ s. step(own(t), a) = ∅ for a ∈ {read(g), borrow(s, g), move(g, s), free(g)} | A value given to an owning parameter, a derived version, a channel or a caller can't be used by its giver again ([MEM-OWNERSHIP-STATIC-PARAMETERS], [MEM-OWNERSHIP-STATIC-SHARING]). | propext, Quot.sound, Classical.choice |
| `moved_once` | s ≠ d ⟹ ⊥ ─alloc(s)·move(s, d)·move(s, d′)→ ∅ | A second `await`, a second `recv` and a second version derived from one source are refused ([MEM-OWNERSHIP-STATIC-FIBERS], [MEM-OWNERSHIP-STATIC-SHARING]). | propext, Quot.sound |
| `join_needs_one_owner` | step(own(k), move(k, m)) = own(k) ⟹ m = k | Paths that meet agree on the owner only if both moved the value or neither did ([MEM-OWNERSHIP-STATIC-PATHS]). | propext |
| `borrow_keeps_owner` | step(c, borrow(b, o)) = c′ ⟹ O′ = O ∧ live′ = live | Any number of read-only borrowers share one owner ([MEM-OWNERSHIP-STATIC-BORROWS]). | none |
| `read_needs_live` | step(c, read(s)) = c′ ⟹ live(c) | Every accepted read is of a live allocation: no use after free. | propext |

Here step(c, a) = ∅ means the action is refused, and ⊥ ─a₁·…·aₙ→ ⊥ means the actions are accepted in order from the freed cell and end in it. The axioms are Lean's three standard ones; a proof that needs any other, `sorryAx` included, fails the gate.

**Negative controls.** The gate weakens a guard of the model and requires Lean to reject the proofs: free(o) allowed while a loan stands, move(s, d) allowed while a loan stands, and free(o) allowed from a slot that is not the owner.

**Scope.** The theorems are about the model. They assume the compiler ends each borrow at the borrower's last read and resolves a chain of borrowers to its ultimate lender (`arc/borrows.rs`). That the instructions the compiler emits are a path of this system is proved per emitted function, below, and is open in general.

#### Proofs of compiled ownership code [MEM-OWNERSHIP-STATIC-VERIFICATION-COMPILER]

`make _test_ownership_proof` compiles the tracked `single_owner.test.osp` regression with the Osprey compiler under test and compiles the production C helpers with Clang at `-O1 -fno-inline` and the ownership allocation shim. The following claims apply to those translations. [Proof gate](../../../scripts/proof/ownership-lowering.sh)

| Checked theorem | Property | Assumptions and scope |
| --- | --- | --- |
| `OwnershipLowering.compiledFreeNull` | Passing null to the compiled ownership free entry reaches a void return before any heap operation. | Reflection follows the actual argument through the direct call, pointer comparison and branch. The non-null path and recursive child destruction require separate proofs. |
| `OwnershipLowering.compiledGrow` | For every integer list and element, the emitted specialization of `grow` returns the appended list as its only live list root and frees the input root exactly once. | The caller supplies one owned root. The append helper returns a fresh root with persistent-list contents. |
| `OwnershipLowering.compiledGrowWhen` | For both Boolean inputs, the emitted function follows the corresponding path through the join. Enabling append frees the original root and returns the appended list. Disabling it returns the original root alive. | The reflector follows branches, resolves LLVM phi operands by predecessor and preserves null frees. The execution model checks those calls against the compiled runtime guard. Lean checks the resulting loads, frees and return against the ownership semantics. |
| `OwnershipLowering.compiledPairFields` | For any two supplied children, the emitted `pairUp` writes the left and right arguments at words 1 and 2 of its fresh record. Its allocation layout equals that of the compiled 64-bit C masked walk, which reads byte 8, calls the child drop with that pointer, then does the same at byte 16. | The proof specializes the shipped walk at the record size and layout with Clang `-O2`. It assumes two distinct owned children, successful allocation, dispatch to the masked walk and a child drop that preserves the parent and sibling. Recursive deallocation and other record layouts remain open. |
| `OwnershipLowering.compiledChildDrop` | The compiled child-drop function looks up the incoming address and forwards that child and its resolved header. A registry miss returns immediately. For every header state, the proof checks the read at byte 8 and all subsequent actions. An owned child (1) is passed to the worklist push once. A moving child (2) becomes owned without a push. All other states leave the header and worklist untouched. | The proof follows both compiled C functions and assumes a correct registry lookup, a valid header on a hit and the native header layout. The moving state belongs to the current collection handoff protocol. Its proof must change with that protocol when the runtime becomes allocation/free only. Worklist storage and destruction of queued descendants remain open. |
| `OwnershipLowering.runtimeRootOrigins` | On every defined execution returning normally, the header returned by list append or prepend originates at `osp_alloc_tagged_noinit`; the header returned by map set originates at `osp_arc_calloc`. | The reflector follows SSA definitions, return sites, helper calls and all incoming phi values in the compiled C. The allocator supplies fresh storage. The theorem establishes address origin; header survival, child ownership and collection contents require additional proofs. |
| `OwnershipLowering.renderedStringOrigin` | A non-null return of the Osprey-generated erased-string renderer originates at the tagged allocator. The proof follows the renderer's call into the compiled C string helper. | Null represents an allocation failure. The result concerns pointer origin on a normal return; the allocator contract, string contents and the rest of the descriptor dispatch remain assumptions or separate obligations. |
| `OwnershipLowering.stringResultOrigins` | Every non-null return of concatenation, take, drop, substring, upper/lower case, all three trim operations, reverse, replace, repeat, left/right padding and construction from a code point originates at the tagged allocator. | Lean checks every reflected return alternative for each of the 15 compiled runtime operations. The claim covers address origin on defined, normally returning executions. String contents, bounds safety and allocation lifetime are separate obligations. |

The gate exports each certificate and requires an axiom-free judgement from the GC, ARC and WASI builds of Osprey's proof kernel. These are three hosts replaying the same theorem about the inspected IR. Negative controls remove, duplicate or advance a free, select a freed input at a join, or change a C return to alias the input list, the borrowed string or the left concatenation operand. The gate also inverts the null guard, stores the left child twice, clears one layout bit and removes one child-drop call, duplicates a worklist push and leaves a moving child in its old state. Each control must reach Lean and fail at its violated theorem.

Shipping the ownership backend requires correspondence between the checker and emitted paths for every supported lowering. Outstanding obligations include preservation of live lenders through fields and closures, cleanup on every continuation exit, transfer across fibers, and the last-use space bound. [Ownership delivery plan](../../plans/0045-compile-time-ownership.md)

### Inferred borrows [MEM-OWNERSHIP-STATIC-BORROWS]

A `let` that names a value another binding owns, such as a field of a record
or an alias of a parameter, borrows it: the new binding holds no reference and
frees nothing. Every binding the value was read from stays alive until the
borrower's last read, so the owner is freed after the borrower, never before.

A read from a list or a map lends the same way: the Result that `listGet`,
`mapGet` or an index gives holds the element without owning it, because the
collection still does. Reading it, matching on it and taking its value with
`?:` are accepted; binding or returning the Result itself needs an owner.

A binding can't borrow in two cases, and is refused as needing a second
owner. The first is a value read out of something the statement itself made that
does not own it, as in `let got = Wrap { inner: made }.inner` while `made` is
still read: that temporary dies when the statement ends. A temporary that
owns the field gives it up instead ([MEM-OWNERSHIP-STATIC-PATHS]).
The second is a value read from a `mut` binding or a file-scope binding,
which can be replaced while the borrower still reads it. A borrower can be
read and passed to functions. Storing it, returning it or handing it to a
fiber needs an owner and is refused. Proved in the ledger model
([MEM-OWNERSHIP-STATIC-MODEL]): `borrow_keeps_owner`, `lent_pins_owner`,
`step_preserves`.

**Lent fields.** A record or union constructor whose field is written as a
binding, or a field of one, moves the value in when that is the binding's last
use. Otherwise the new value holds the field without owning it: the binding
keeps its one reference, the new value frees nothing through that field, and
every binding it holds a field of stays alive until the new value's last read.
This is the read-only borrow of [Boyland 2001] and the second-class value of
[Osvald 2016], inferred per field: `Pair { left: made, right: made }` and
`Wrap { inner: made }` followed by a read of `made` are both accepted. A value
with a lent field can be bound in the scope that made it, read, matched,
passed to a function that borrows it and stored in another constructor, which
then lends in turn. It can't leave that scope: returning it, binding it at file
scope, giving it to an owning parameter or a fiber, or yielding it from one arm
of a `match` beside an arm that yields a new value is refused. A field written
as any other expression is a fresh value and moves in. When that fresh value
itself holds a lent field, as in `Outer { wrap: Wrap { inner: made } }`, the
new value owns it and lends in turn. A refusal of a lending value names the
binding it holds. Proved: `lent_pins_owner`, `lent_field_not_freed`,
`lent_then_released`, `lent_twice_released`, `edge_frames`.

A top-level function used as a value is a constant cell: nothing owns it and
nothing frees it, so storing it anywhere needs no owner.

### Owned parameters [MEM-OWNERSHIP-STATIC-PARAMETERS]

Each managed parameter has a compiler-inferred mode. A borrowed parameter
is valid for the call and cannot escape it. A consuming parameter transfers
its owner into the function. Returning a parameter, storing it in an owned
result or using it to derive a new collection can require a consuming mode.
The caller supplies a fresh value or an owner at its last use. The callee
transfers or frees it on every path. A later read by the caller is refused
(proved: `given_is_gone`):

```text
main.osp: 13:4: ownership: `made` needs a second owner in `main`: it is given to a function that takes ownership of it; it is read again at 14:4 [MEM-OWNERSHIP-STATIC]
```

The compiler checks the parameter modes and result ownership at each call
boundary. Every possible target of an indirect call must satisfy the same
contract, including when a closure stores arguments for later application.
If the call cannot establish a consuming contract, it can lend arguments
only to targets proved to keep those borrows within the call. Host calls use
the ownership contract declared by their ABI. A borrow and a move of
aliases to the same allocation may coexist only if the borrow is proved
dead before the callee can free or reuse the allocation.

The current lowering infers consuming modes for eligible functions called
directly by name in an executable. A generic function's modes are inferred
from its body once and hold for every type it is called at; under
`ownership` such a function is emitted as a real function per set of
argument types, when its return type has a concrete representation, so the
callee can take what it is given. A generic function whose return type is a
bare type variable is still expanded in its caller and borrows. A list
literal given to a consuming parameter is moved, never lent. Indirect and
host entry points use borrowed parameters where supported. ARC also uses the inferred modes
to reduce count updates. This contract requires agreement at the call
boundary even when the source function type carries no ownership modes.

### Moves on one path [MEM-OWNERSHIP-STATIC-PATHS]

A binding may be given away where no path through the statement reads it
twice and nothing reads it afterwards. When only some paths give it away, as
when one arm of a `match` stores or returns it, the binding is released on the
other paths and not on that one. Proved: `join_needs_one_owner`,
`only_free_kills`.

**Taking a field.** A value the function made and never named has one owner,
so what it holds has one owner too. A field read out of it can take the
reference the value held: `make(n).name`, `make(n).box.text`, and a field a
`match` on a temporary binds, as in `match attempt(n) { Done { result } =>
result }`. The field's slot is cleared and the value is freed later without
it, so the field can be returned, bound, stored or yielded by an arm with no
second owner. This is the owned projection of [Ullrich 2019] and the drop
specialization of [Reinking 2021], with the count removed. It applies only
where nothing reads that field again: a pattern binding read twice, or read
after the take, keeps borrowing; so does a field of a named binding, a field
of a value that only lends it, and a read inside a loop body of a value made
outside it. Proved: `take_moves_owner`, `taken_field_not_freed`,
`take_needs_no_loan`, `taken_freed_once`.

### Rules for shared values [MEM-OWNERSHIP-STATIC-RULES]

These are decided. The checking mode enforces the ones stated under "What is
refused"; the rest are the contract the remaining phases build.

- **Persistent collections [MEM-OWNERSHIP-STATIC-SHARING].** Two live versions
  of a list or map are refused. An operation that derives a new version takes
  its argument by move when that is the argument's last use, and may then
  update it in place; when the argument is used again afterwards the program
  is refused. The compiler never inserts a copy, because a hidden copy of a
  whole collection would make cost unpredictable. Proved: `given_is_gone`,
  `moved_once`.
- **Fibers and channels [MEM-OWNERSHIP-STATIC-FIBERS].** Ownership transfers to
  exactly one side. `spawn` moves the managed values its body captures into the
  fiber. `await` moves the result to the awaiting side, so a fiber whose result
  is managed is awaited once and a second `await` is refused. `send` moves a
  value into the channel and `recv` moves it out (proved: `moved_once`,
  `given_is_gone`). This replaces the runtime
  co-ownership of [MEM-FIBER-ISOLATION] under this backend. The sender must
  have no surviving local borrow that outlives the handoff. Pony's isolated
  capability provides a precedent for reasoning about aliases when a value
  crosses a concurrency boundary. [Deny Capabilities for Safe, Fast Actors (AGERE! 2015)](https://soft.vub.ac.be/AGERE15/papers/AGERE_2015_paper_10.pdf)
- **Closures and effects [MEM-OWNERSHIP-STATIC-CLOSURES].** A closure that
  outlives the scope it is written in owns the managed values it captures, so
  capturing is a move and must be the captured binding's last use. A closure
  applied only inside that scope borrows them. Built: a closure written as an
  argument of a call to an Osprey function borrows what it captures. Its cell
  dies with the calling statement and the callee can't keep a parameter, so
  the cell owns none of its captures; binding, storing or returning it is
  refused. A list literal written as an argument lends the same way: it holds
  its elements, closures included, without owning them. Built: any other closure
  literal takes each capture at that binding's last use and holds it lent
  otherwise, so a closure bound to a name, captured by a second closure or
  stored in a record can read bindings the scope still uses; it then can't
  leave that scope. A `spawn` body never lends. Built: a handler region
  lends what its arms capture, because the region ends inside the scope that
  owns them; a value `perform` lends that an arm keeps in a cell is copied
  first, as in `fault = "${op}"`.
  `perform` lends its arguments to the handler until it resumes, and the
  resumed value moves to the performer. A continuation that owns managed
  values is resumed at most once. If the handler discards it, the discard
  path drops those owners exactly once. A continuation is eligible for
  repeated resumption only if the compiler can prove that doing so neither
  duplicates an owner nor outlives a borrow. Tang, Hillerström, Lindley and
  Morris formalize the resource obligations for discarding and duplicating
  continuations. [Soundly Handling Linearity (POPL 2024)](https://www.research.ed.ac.uk/files/407801113/Soundly_Handling_TANG_DOA07112023_VOR_CC_BY.pdf)
- **Host libraries [MEM-OWNERSHIP-STATIC-HOSTS].** A managed result returned
  over the C ABI belongs to the host, which hands it back through the
  exported release function exactly once. Arguments a host passes are lent
  for the call. An exported global is owned by the module and lent to every
  caller. The rule is the same for WASM, iOS, Android and Python hosts.
- **Diagnostics [MEM-OWNERSHIP-STATIC-ADVICE].** With no annotation syntax,
  there are two changes a refused program can make: make the use that needs
  ownership the last use of the value, or build a separate value for that
  use. The diagnostic names the value and the use so either change is local.

### Research synthesis [MEM-OWNERSHIP-STATIC-BASIS]

No single published system gives a functional language static memory safety with no annotations, no leaks and predictable space. Each line of research below solved one part. This section states what each established, the rule Osprey takes from it, and what Osprey leaves out. References are listed once, under [MEM-OWNERSHIP-STATIC-REFERENCES](#references-mem-ownership-static-references).

| Line of research | What it established | What Osprey takes | What Osprey leaves out |
| --- | --- | --- | --- |
| Linear and affine types [Wadler 1990; Baker 1992; Bernardy 2018] | A value used at most once needs no collector, and can be updated in place. | One owner per value, moved at its last use ([MEM-OWNERSHIP-STATIC-PATHS]). A value never read is dropped by its owner, so the discipline is affine. | Linearity written in types. Osprey infers it per binding. |
| Uniqueness and in-place update [Barendsen 1996; Lorenzen 2023; Lorenzen 2024a; Marshall 2024] | A unique value can be overwritten without the program observing it. A function can be checked to reuse its argument and allocate nothing. | One live version of a list or map; a derived version takes its source ([MEM-OWNERSHIP-STATIC-SHARING]). | Uniqueness attributes in types, and an `fip` keyword. |
| Alias control [Clarke 1998; Boyland 2001; Boyland 2003] | Aliases are safe when they can't outlive or bypass the owner. A borrowed unique value is unusable by its owner's writers until the borrow ends. Read permissions can be split and rejoined. | Any number of read-only borrows of one owner; the owner is freed after the last of them ([MEM-OWNERSHIP-STATIC-BORROWS]). | Ownership parameters on types, and fractional permissions as syntax. |
| Regions [Tofte 1997; Grossman 2002; Fluet 2006] | Lifetimes can be inferred as nested regions and checked statically. | A borrow's lifetime is an inferred region that ends at the borrower's last read. | Region variables in types, and region-wide deallocation: a region frees late, at its end. |
| Rust and its formal models [Matsakis 2017; Weiss 2019; Pearce 2021; Jung 2018; Jung 2020; Villani 2025; Ho 2022] | Ownership with borrowing is practical at scale, and is sound when proved against a model. Borrows should end at last use, not at scope end. Unsafe code needs an aliasing model of its own. | Moves, last-use borrows, refusal of a borrow that outlives its owner, a proof obligation at each runtime boundary ([MEM-OWNERSHIP-STATIC-SAFETY]). | Lifetime parameters, mutable borrows, `unsafe`, and reference-counted escape hatches. See [MEM-OWNERSHIP-STATIC-BEYOND-RUST](#beyond-rust-mem-ownership-static-beyond-rust). |
| Precise reference counting [Reinking 2021; Ullrich 2019; Lorenzen 2022] | Ownership can be inferred from last uses, and a program can be garbage free: it retains only live values. Parameter modes can be inferred. Borrowing can raise peak memory without bound. | The ledger is the Perceus analysis with `dup` made an error. Owned parameters are inferred ([MEM-OWNERSHIP-STATIC-PARAMETERS]). | Run-time counts. The space bound is not yet met, see below. |
| Second-class values and modes [Osvald 2016; Lorenzen 2024b] | A value that can't be stored or returned can't escape, with no lifetime written. | Lent closures, lent list literals, `...rest` views and collection reads ([MEM-OWNERSHIP-STATIC-CLOSURES]). | Mode annotations on types and bindings. |
| Isolation between threads [Clebsch 2015; Fähndrich 2006] | A value that moves between threads of control, leaving no alias behind, needs no lock and no shared count. | `spawn`, `await`, `send` and `recv` move ([MEM-OWNERSHIP-STATIC-FIBERS]). | Capability annotations and a separate exchange heap. |
| Effects and linearity [Tang 2024] | A handler may drop or re-run a continuation, so a linear value captured by one is unsound unless the operation is known to resume exactly once. | A continuation that owns values is resumed at most once, and frees them when dropped ([MEM-OWNERSHIP-STATIC-CLOSURES]). | A linearity annotation on operations. `control` and `many` already say it. |
| Mutable value semantics [Racordon 2022] | Single-owner collections with no references in the language, copying when a value is used twice. | Values with no observable sharing. | The implicit copy. Osprey refuses the second use so that cost stays visible. |
| Fully inferred reclamation [Proust 2017] | Frees can be inferred with no annotation, scanning at run time where the analysis is imprecise. | No annotations, ever. | The run-time scan. Osprey refuses the program, which still runs under `gc` or `arc`. |

The findings this design rests on most, in their authors' words:

| Published finding | Osprey design consequence |
| --- | --- |
| Reinking et al. describe precise reference counting as freeing an object “as soon as no more references remain.” [Perceus, §2.2 (Microsoft Research)](https://www.microsoft.com/en-us/research/wp-content/uploads/2021/06/perceus-pldi21.pdf) | Account for live borrows when proving a last use. A field or closure can still read an allocation after its owner's last direct read. |
| Ullrich and de Moura describe “a heuristic for automatically inferring borrow annotations.” [Counting Immutable Beans, abstract (Karlsruhe Institute of Technology)](https://pp.ipd.kit.edu/uploads/publikationen/ullrich19counting.pdf) | Infer argument modes and verify each call against its callee's contract. |
| Lorenzen and Leijen show that “borrowing can increase the memory usage of a program by an arbitrary amount.” [Reference Counting with Frame Limited Reuse, §4.2 (Microsoft Research)](https://www.microsoft.com/en-us/research/wp-content/uploads/2023/07/flreuse.pdf) | Measure peak live bytes while a consuming traversal builds its output. A zero exit count alone cannot detect prolonged retention. |
| Tang et al. explain that “effect handlers allow continuations to be discarded (e.g. for exceptions) or invoked more than once (e.g. for backtracking).” [Soundly Handling Linearity, abstract (University of Edinburgh)](https://www.research.ed.ac.uk/files/407801113/Soundly_Handling_TANG_DOA07112023_VOR_CC_BY.pdf) | Dropping a continuation must release its owners. Repeated resumption requires proof that it preserves ownership and borrow lifetimes. |

### Beyond Rust [MEM-OWNERSHIP-STATIC-BEYOND-RUST]

Rust is the reference point for static ownership. This backend has to do better than Rust on each property below. Each states Rust's documented position, Osprey's rule, and whether the rule is built.

- **No annotations. Built.** Rust infers lifetimes inside a function body and requires them in signatures [Matsakis 2017]. Osprey writes none anywhere: owners, borrows and parameter modes are inferred.
- **No leaks. Built for accepted programs, tested, not proved.** Rust does not guarantee that destructors run: `mem::forget` is safe, and a cycle of `Rc` leaks [Rust RFC 1066]. Osprey has no way to forget a value and the heap is acyclic ([MEM-ACYCLIC]), so every owner is released. Each accepted regression must exit with no live object.
- **No unsafe code in the language. Built.** A study of every Rust memory-safety CVE found that "all memory-safety bugs require unsafe code" [Xu 2021], and unsafe features are used extensively in published crates, most of all for interoperation [Astrauskas 2020]. Osprey source has no such construct. What is trusted is the C runtime and each `extern` declaration, held to [MEM-OWNERSHIP-STATIC-SAFETY].
- **No aliasing model to obey. Built.** Rust needs Stacked Borrows or Tree Borrows to say which unsafe programs are defined [Jung 2020; Villani 2025]. Osprey values are immutable, so read-only borrows never conflict, and a `mut` binding, the only thing that changes, can't be borrowed.
- **Freed at the last use. Built for owners.** Rust runs a destructor at the end of the binding's scope. Osprey releases an owner after its last read, the garbage-free property of [Reinking 2021]. A borrowed argument still lives for the whole call, see below.
- **Refusal never changes a program. Built.** A Rust program the borrow checker refuses must be rewritten, often with `Rc<RefCell<T>>`, which moves the check to run time and can panic. A program `ownership` refuses compiles unchanged under `gc` or `arc` and gives the same output ([MEM-OPAQUE]).
- **Effects and continuations under one rule. Built as refusal.** Rust has no effect handlers. Osprey applies [Tang 2024]: today every handler of a `control` or `many` operation is refused; the target is to accept one that resumes at most once.
- **In-place update of persistent collections. Decided, not built.** A derived version whose source is at its last use may overwrite it [Lorenzen 2023]. Rust has no persistent collections in its language to do this for.
- **A proof of the shipped checker. Built for the model, open for the lowering.** Rust's proofs are of models: λRust [Jung 2018], Oxide [Weiss 2019] and Featherweight Rust [Pearce 2021], not of `rustc`. Osprey's ledger is a mechanised transition system whose accepted paths are proved to keep one owner per allocation and a live lender for every borrow, with a theorem for each lowering rule, replayed by Osprey's own proof kernel ([MEM-OWNERSHIP-STATIC-MODEL]). The emitted code of selected functions is proved against it ([MEM-OWNERSHIP-STATIC-VERIFICATION-COMPILER]). Open: that every function the compiler emits is a path of the model. The backend is not declared shipped without it.
- **A space bound. Not built.** [Lorenzen 2022] show that unrestricted borrowing can raise peak memory by an arbitrary amount and define the frame-limited bound that avoids it. Osprey must meet that bound: a value at its last use is given to a callee that can free it earlier. Until then, benchmarks report peak live bytes for `ownership` beside `gc` and `arc`, separately from exit-time leaks.

**Where Rust is ahead today.** Rust accepts far more programs. It has mutable borrows, and it records ownership in types, so functions passed as values and library boundaries are checked against a signature. Osprey infers ownership from the bodies of one executable, so those two keep borrowed parameters ([MEM-OWNERSHIP-STATIC-PARAMETERS]), and `ownership` accepts 59 of the 181 programs in `src/osprey/tests/` (2026-11-24). Rust's models are proved; Osprey's ledger is tested.

### References [MEM-OWNERSHIP-STATIC-REFERENCES]

- [Astrauskas 2020] Astrauskas, Matheja, Poli, Müller, Summers. "How Do Programmers Use Unsafe Rust?" OOPSLA 2020. [doi:10.1145/3428204](https://doi.org/10.1145/3428204)
- [Baker 1992] Baker. "Lively Linear Lisp: 'Look Ma, No Garbage!'" ACM SIGPLAN Notices 27(8), 1992. [doi:10.1145/142137.142162](https://doi.org/10.1145/142137.142162)
- [Barendsen 1996] Barendsen, Smetsers. "Uniqueness Typing for Functional Languages with Graph Rewriting Semantics." Mathematical Structures in Computer Science 6(6), 1996. [doi:10.1017/S0960129500070109](https://doi.org/10.1017/S0960129500070109)
- [Bernardy 2018] Bernardy, Boespflug, Newton, Peyton Jones, Spiwack. "Linear Haskell: Practical Linearity in a Higher-Order Polymorphic Language." POPL 2018. [doi:10.1145/3158093](https://doi.org/10.1145/3158093)
- [Boyland 2001] Boyland. "Alias Burying: Unique Variables without Destructive Reads." Software: Practice and Experience 31(6), 2001. [doi:10.1002/spe.370](https://doi.org/10.1002/spe.370)
- [Boyland 2003] Boyland. "Checking Interference with Fractional Permissions." SAS 2003. [doi:10.1007/3-540-44898-5_4](https://doi.org/10.1007/3-540-44898-5_4)
- [Clarke 1998] Clarke, Potter, Noble. "Ownership Types for Flexible Alias Protection." OOPSLA 1998. [doi:10.1145/286936.286947](https://doi.org/10.1145/286936.286947)
- [Clebsch 2015] Clebsch, Drossopoulou, Blessing, McNeil. "Deny Capabilities for Safe, Fast Actors." AGERE! 2015. [doi:10.1145/2824815.2824816](https://doi.org/10.1145/2824815.2824816)
- [Fähndrich 2006] Fähndrich, Aiken, Hawblitzel, Hodson, Hunt, Larus, Levi. "Language Support for Fast and Reliable Message-Based Communication in Singularity OS." EuroSys 2006. [doi:10.1145/1217935.1217953](https://doi.org/10.1145/1217935.1217953)
- [Fluet 2006] Fluet, Morrisett, Ahmed. "Linear Regions Are All You Need." ESOP 2006. [doi:10.1007/11693024_2](https://doi.org/10.1007/11693024_2)
- [Grossman 2002] Grossman, Morrisett, Jim, Hicks, Wang, Cheney. "Region-Based Memory Management in Cyclone." PLDI 2002. [doi:10.1145/512529.512563](https://doi.org/10.1145/512529.512563)
- [Ho 2022] Ho, Protzenko. "Aeneas: Rust Verification by Functional Translation." ICFP 2022. [doi:10.1145/3547647](https://doi.org/10.1145/3547647)
- [Jung 2018] Jung, Jourdan, Krebbers, Dreyer. "RustBelt: Securing the Foundations of the Rust Programming Language." POPL 2018. [doi:10.1145/3158154](https://doi.org/10.1145/3158154)
- [Jung 2020] Jung, Dang, Kang, Dreyer. "Stacked Borrows: An Aliasing Model for Rust." POPL 2020. [doi:10.1145/3371109](https://doi.org/10.1145/3371109)
- [Lorenzen 2022] Lorenzen, Leijen. "Reference Counting with Frame Limited Reuse." ICFP 2022. [doi:10.1145/3547634](https://doi.org/10.1145/3547634)
- [Lorenzen 2023] Lorenzen, Leijen, Swierstra. "FP²: Fully in-Place Functional Programming." ICFP 2023. [doi:10.1145/3607840](https://doi.org/10.1145/3607840)
- [Lorenzen 2024a] Lorenzen, Leijen, Swierstra, Lindley. "The Functional Essence of Imperative Binary Search Trees." PLDI 2024. [doi:10.1145/3656398](https://doi.org/10.1145/3656398)
- [Lorenzen 2024b] Lorenzen, White, Dolan, Eisenberg, Lindley. "Oxidizing OCaml with Modal Memory Management." ICFP 2024. [doi:10.1145/3674642](https://doi.org/10.1145/3674642)
- [Marshall 2024] Marshall, Orchard. "Functional Ownership through Fractional Uniqueness." OOPSLA 2024. [doi:10.1145/3649848](https://doi.org/10.1145/3649848)
- [Matsakis 2017] Matsakis et al. "Non-Lexical Lifetimes." Rust RFC 2094, 2017. [rust-lang.github.io/rfcs/2094-nll.html](https://rust-lang.github.io/rfcs/2094-nll.html)
- [Osvald 2016] Osvald, Essertel, Wu, González Alayón, Rompf. "Gentrification Gone Too Far? Affordable 2nd-Class Values for Fun and (Co-)Effect." OOPSLA 2016. [doi:10.1145/2983990.2984009](https://doi.org/10.1145/2983990.2984009)
- [Pearce 2021] Pearce. "A Lightweight Formalism for Reference Lifetimes and Borrowing in Rust." ACM TOPLAS 43(1), 2021. [doi:10.1145/3443420](https://doi.org/10.1145/3443420)
- [Proust 2017] Proust. "ASAP: As Static As Possible Memory Management." University of Cambridge Technical Report UCAM-CL-TR-908, 2017. [doi:10.48456/tr-908](https://doi.org/10.48456/tr-908)
- [Racordon 2022] Racordon, Shabalin, Zheng, Abrahams, Saeta. "Implementation Strategies for Mutable Value Semantics." Journal of Object Technology 21(2), 2022. [doi:10.5381/jot.2022.21.2.a2](https://doi.org/10.5381/jot.2022.21.2.a2)
- [Reinking 2021] Reinking, Xie, de Moura, Leijen. "Perceus: Garbage Free Reference Counting with Reuse." PLDI 2021. [doi:10.1145/3453483.3454032](https://doi.org/10.1145/3453483.3454032)
- [Rust RFC 1066] "Safe `mem::forget`." Rust RFC 1066, 2015. [rust-lang.github.io/rfcs/1066-safe-mem-forget.html](https://rust-lang.github.io/rfcs/1066-safe-mem-forget.html)
- [Tang 2024] Tang, Hillerström, Lindley, Morris. "Soundly Handling Linearity." POPL 2024. [doi:10.1145/3632896](https://doi.org/10.1145/3632896)
- [Tofte 1997] Tofte, Talpin. "Region-Based Memory Management." Information and Computation 132(2), 1997. [doi:10.1006/inco.1996.2613](https://doi.org/10.1006/inco.1996.2613)
- [Ullrich 2019] Ullrich, de Moura. "Counting Immutable Beans: Reference Counting Optimized for Purely Functional Programming." IFL 2019. [doi:10.1145/3412932.3412935](https://doi.org/10.1145/3412932.3412935)
- [Villani 2025] Villani, Hostert, Dreyer, Jung. "Tree Borrows." PLDI 2025. [doi:10.1145/3735592](https://doi.org/10.1145/3735592)
- [Wadler 1990] Wadler. "Linear Types Can Change the World!" IFIP TC 2 Working Conference on Programming Concepts and Methods, 1990. [homepages.inf.ed.ac.uk/wadler/topics/linear-logic.html](https://homepages.inf.ed.ac.uk/wadler/topics/linear-logic.html)
- [Weiss 2019] Weiss, Gierczak, Patterson, Ahmed. "Oxide: The Essence of Rust." 2019. [arXiv:1903.00982](https://arxiv.org/abs/1903.00982)
- [Xu 2021] Xu, Chen, Sun, Zhou, Lyu. "Memory-Safety Challenge Considered Solved? An In-Depth Study with All Rust CVEs." ACM TOSEM 31(1), 2021. [doi:10.1145/3466642](https://doi.org/10.1145/3466642)
