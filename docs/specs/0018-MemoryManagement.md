# Memory Management

Osprey programs do not manage memory. Reclamation is a property of the
*implementation*, never of the language: these rules define semantics under
which a conforming implementation may reclaim memory with reference counting,
a tracing collector, fully static frees, or any mix — with no observable
difference to any program. The developer's only obligation is the one every
garbage-collected language already imposes: don't keep references to values
you no longer need.

> **Flavor layer — shared core (AST and above).**  Memory management is entirely
> below the canonical AST: the `@osp_alloc` boundary, ARC/tracing reclamation,
> ownership inference, and the runtime all consume `osprey_ast::Program` and the
> IR lowered from it, never the CST or its source flavor. Allocation arises from
> the AST nodes that produce heap values (`List`, `Map`, `Object`, `Str`,
> `InterpolatedStr`, closures from `Lambda`, union/record `TypeConstructor`),
> and two programs with identical ASTs exhibit byte-identical memory behaviour
> whether they were written in the Default (`.osp`) or ML (`.ospml`) flavor. See
> [Language Flavors](0023-LanguageFlavors.md) [FLAVOR-BOUNDARY].

## Status

Partially implemented — the *boundary* exists, a first *reclaiming* backend
ships (tracing GC, opt-in via `--memory=gc`), and the Perceus ARC backend is
**counting for real** (`--memory=arc` links the counting runtime — header,
registry, kind/mask drop — and the compiler now inserts the full Perceus
dup/drop discipline: producers own +1, dup-on-store, drops at region end and
at last use, returns transfer +1. Byte-identical on the full differential
harness under all three backends; container-free programs report zero live
language values at exit. Containers are leak-safe pending node-level RC —
[implementation plan 0011](../plans/0011-arc-gc-implementation.md) phase 2,
milestones M4b/M5b/M6b).

- **Swappable backend boundary [MEM-BACKENDS]: done.** All codegen heap
  allocation funnels through a single `@osp_alloc` hook (osprey-codegen
  `builder.rs::heap_alloc` / `OSP_ALLOC_DECL`); the emitted IR names no
  allocator, so a manager is chosen at link time. The default backend
  (`compiler/runtime/memory_runtime.c`) is a `malloc` passthrough that never
  frees during a run — sound because reclamation is unobservable [MEM-OPAQUE].
- **Static reclamation of non-escaping values: done, by the optimizer.** The
  `@osp_alloc` declaration carries allocator attributes, so at `-O2` LLVM proves
  provably-dead allocations (the common case — per-operation `Result` blocks,
  temporaries) non-escaping and removes them entirely. This realises the
  [MEM-OWNERSHIP] "free at last use, statically" ideal for everything whose
  lifetime LLVM can see.
- **Reclaiming *escaping* values: tracing GC ships, ARC pending.** Values that
  genuinely outlive their allocation site (e.g. nodes of a built-and-held tree)
  still leak under the *default* backend, but the opt-in tracing collector
  (`--memory=gc`, `compiler/runtime/memory_gc.c`) reclaims them — a conservative
  mark & sweep linked behind `@osp_alloc`, complete because the heap is acyclic
  [MEM-ACYCLIC]. On `binarytrees` it cuts peak RSS ~80× (905 MiB → ~11 MiB) with
  byte-identical output across every differential example (`make _conformance-gc`).
  The ARC default and a precise copying GC are the remaining work
  ([plan 0011](../plans/0011-arc-gc-implementation.md)); this spec is the
  contract they must satisfy.

## Collection Is Unobservable [MEM-OPAQUE]

No Osprey program can observe when, whether, or how memory is reclaimed.
Concretely:

- There are no finalizers or destructors, and there never will be — no code
  runs because a value died [MEM-OPAQUE-NO-FINALIZERS].
- There are no destruction-order or destruction-timing guarantees.
- No API exposes addresses, object identity beyond structural equality, or
  collector state.

A program whose output depends on reclamation behavior is not a valid Osprey
program; conforming implementations are free to differ on it. This rule is
what makes every backend below interchangeable.

## Debugger Observability [MEM-DEBUG-OBSERVABILITY]

The debugger and memory profiler are allowed to observe implementation memory
state only when the user explicitly starts a debug/profiling session. That
observability is outside the language semantics.

Debugger/profiler surfaces may expose:

- Debug object ids and, in expert/native modes, raw addresses.
- Heap object kinds, Osprey types, source allocation sites, shallow sizes,
  retained sizes, allocation generations, and backend provenance.
- Incoming/outgoing object graph edges and paths from roots to a selected value.
- ARC retain counts, static-ownership classifications, tracing-GC mark state,
  and custom-manager metadata when the active backend supports it.
- Snapshot diffs and allocation/retention timelines.

These facts MUST NOT be available to Osprey source code, MUST NOT affect
program output, and MUST NOT introduce collection-order guarantees. A debugger
may pause the program, request bounded runtime inspection, and pay profiling
overhead, but the inspected program's observable Osprey behavior remains
governed by [MEM-OPAQUE].

Precision depends on the backend:

- ARC/debug-static metadata is precise when the compiler/runtime emit object
  descriptors for all Osprey heap edges.
- The conservative GC may report approximate native roots because stack/register
  scanning can mistake non-pointers for roots. Such roots must be labelled
  conservative/approximate in the debugger.
- Custom managers either implement the optional debug introspection interface or
  report object graph/retention data as unsupported. They must not let the
  editor infer private memory layouts ad hoc.

## Resources Are Effects, Not Destructors [MEM-RESOURCES]

External resources (files, sockets, processes, handles) MUST be released by
*scoped* constructs — an effect handler that brackets acquire/release around
the code that uses the resource — never by tying release to a value's death.
This is forced by [MEM-OPAQUE]: value death has no observable timing, so it
can never be a release point.

## The Value Heap Is Acyclic [MEM-ACYCLIC]

Immutable values cannot reference values created after them, so reference
cycles cannot be constructed. Consequences:

- Reference counting is *complete* — no cycle collector is required, and a
  refcounting backend and a tracing backend are observationally identical.
- `mut` does not break this: reassignment rebinds the *name* to a new value;
  it never mutates a heap value in place (closure captures snapshot at
  creation per [TYPE-FN-CLOSURE]).

This is a constraint on language evolution: any future feature that allows a
heap value to be mutated to point at a younger value either preserves
acyclicity by construction or is rejected.

## Fibers Share Nothing [MEM-FIBER-ISOLATION]

Values cross fiber boundaries — `spawn` captures and channel `send` — by
move or by copy, never by sharing. No value is ever co-owned by two fibers.
Consequences:

- Each fiber's heap is single-threaded, so all reference counts are
  non-atomic.
- A fiber's values are reclaimable when the fiber completes, independent of
  other fibers.

## Ownership and the Shared Residue [MEM-OWNERSHIP]

Every heap value has an owner. The compiler infers ownership and statically
places the free wherever a value's last use is provable — the common case in
an immutable language.

The single construct that defeats static placement is **sharing**: two or
more live references to one value whose last use depends on runtime control
flow [MEM-OWNERSHIP-SHARED]. Canonical forms: structural sharing in
persistent data (`prepend(x, xs)` leaves `xs` and the result sharing a
spine), and aliased escaping closures. Shared values carry a non-atomic
reference count at runtime; everything else is freed statically. Sharing is
inferred — the developer never annotates it.

## Static Mode [MEM-STATIC-MODE]

Under `--static-memory`, compilation FAILS at every point where the
ownership analysis would insert a reference count, with a diagnostic naming
the shared value and the conflicting owners. A program accepted in static
mode contains **zero** runtime memory-management operations (no refcounts,
no collector) — Rust-class output without a borrow checker the developer
fights — and behaves byte-for-byte identically under the default mode.
Static-mode programs are a strict subset of Osprey, not a dialect.

### Barred Constructs [MEM-STATIC-MODE-BARRED]

Static mode bars exactly the constructs that create a shared residue:

1. **Live aliasing** — holding two or more references to one heap value
   past the point where a unique last owner is provable: `let g = f` where
   both escape, storing a value into a record or closure capture while the
   original binding stays live with divergent control flow.
2. **Built-in persistent collections** — `List` and `Map` (their spine/HAMT
   nodes share structure internally in the runtime); barred in static mode
   v1.

Everything else stays available: escaping closures with a unique owner,
records, unions, strings, `Result`, pattern matching, algebraic effects —
and fibers, because [MEM-FIBER-ISOLATION] moves or copies across the
boundary rather than sharing.

## Backend Conformance [MEM-BACKENDS]

Two backends ship out of the box, chosen at build time and invisible in
source code:

- **ARC (default)** — non-atomic reference counting on the shared residue,
  statically elided wherever ownership is provable. Complete without a
  cycle collector because the heap is acyclic [MEM-ACYCLIC]. The algorithm
  is **Perceus** (Reinking, Xie, de Moura, Leijen), implemented from the
  extended technical report
  [MSR-TR-2020-42](https://www.microsoft.com/en-us/research/wp-content/uploads/2020/11/perceus-tr-v1.pdf):
  precise, garbage-free reference counting (every value freed at its last
  dynamic use), with drop specialization and drop-guided reuse.
- **Tracing GC** — the conformance oracle that keeps [MEM-OPAQUE] honest.

**Backend portability.** The two reclaiming backends need different things from
the host. The conservative tracing GC finds roots by scanning the native stack,
machine registers and data/BSS segments, so it runs on native targets only. ARC
is *precise* (the compiler inserts retain/release) and non-atomic, so it carries
to every target — including `wasm32`, where it is the *only* reclaiming option:
the conservative GC cannot scan a wasm stack, and the WebAssembly-GC proposal is
a separate, untargeted mechanism. See
[spec 0022](0022-WebAssemblyTarget.md) [WASM-TARGET-MEMORY].

A reclamation backend is conforming iff every differential-harness example
produces byte-identical output and reports zero leaked language values under
it.

### Custom Managers [MEM-BACKENDS-CUSTOM]

The backend boundary is a small C interface (alloc/retain/release/collect
hooks), and anyone may link their own manager against it — arenas, pools,
debugging allocators. Soundness of a custom manager is the supplier's
responsibility: the language's memory-safety guarantee covers only the
shipped backends, and a build linking a custom manager must say so visibly
(e.g. in `--version` output).
