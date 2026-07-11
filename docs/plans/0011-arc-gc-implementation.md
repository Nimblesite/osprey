# Plan 0011 — Swappable reclaiming memory backends (ARC + tracing GC)

**Status:** Phase 1 (conservative tracing GC) is **shipped and green** —
`--memory=gc` links a mark & sweep archive, the differential harness passes
byte-identically under it (`make _conformance-gc`), and the benchmark suite
carries an `Osprey (GC)` column (binarytrees 2.5 GB → ~11 MB). Phases 2–4
(the Perceus ARC default, the Cheney copying oracle, and `--static-memory`)
are **not started** — `osp_retain`/`osp_release` are no-op stubs, there is
no per-allocation object header, and no ownership analysis exists. See
[§What is left](#what-is-left-detailed).

Realises the [MEM-BACKENDS] contract of
[spec 0018](../specs/0018-MemoryManagement.md): two robust, swappable memory
managers behind the existing `@osp_alloc` link-time boundary, plus a static
`--static-memory` mode (the "borrow-checker" subset). Reclamation stays
unobservable [MEM-OPAQUE], so every backend is observationally identical and
selected at link time, never in source.

## The governing facts (and the papers that justify them)

Three properties of the Osprey value heap collapse the usual GC design space:

1. **The heap is acyclic** [MEM-ACYCLIC]. Immutable values cannot reference
   values created after them, so cycles are unconstructable. Bacon, Cheng &
   Rajan, *A Unified Theory of Garbage Collection* (OOPSLA 2004) prove tracing
   and reference counting are duals computing the least / greatest fix-point of
   the same reference-count equation, and their difference is **exactly the
   cyclic garbage**. Acyclic ⇒ the fix-points coincide ⇒ **naive reference
   counting is complete** — no cycle collector, no trial deletion, no backup
   trace. This is the licence for ARC as the primary backend.
2. **Fibers share nothing** [MEM-FIBER-ISOLATION]. Each fiber's heap is
   single-threaded, so reference counts are **non-atomic** and a fiber's heap is
   collectable independently when it completes.
3. **Reclamation is unobservable** [MEM-OPAQUE]. No finalizers, no timing. Any
   two conforming backends produce byte-identical output — the conformance
   oracle (below).

## Backends

### `[GC-TRACE-CONSERVATIVE]` — tracing GC, **shipped first** (this plan, phase 1)

A conservative, non-moving **mark & sweep** over the managed heap reachable from
the C stack, machine registers, and the program's data/BSS segments —
Boehm & Weiser, *Garbage Collection in an Uncooperative Environment* (SP&E
1988), specialised to the acyclic heap so a single mark pass is complete
(Bacon/Cheng/Rajan 2004). A machine word is treated as a root iff it equals the
**base address of a known managed allocation**; false positives (integers that
look like pointers) only *retain* an object — they never corrupt it, because the
collector never moves. This needs **zero codegen changes**: it slots in behind
`@osp_alloc` purely at link time, which is why it ships first and why it is the
safe way to validate the whole boundary end-to-end.

- **Soundness scope v1:** collection runs only while the process is effectively
  single-threaded (the main thread is the sole allocator); the first allocation
  from any other thread permanently disables collection (a fiber's isolated heap
  is future work — precise per-fiber GC, phase 3). Every allocation and the
  whole collection run hold one mutex, so disabling is race-free.
- **Managed heap:** codegen allocations (`@osp_alloc`) plus the value-container
  runtime units (`list_runtime`, `map_runtime`, `map_runtime_hamt`) whose nodes
  store boxed Osprey values; recompiled in the GC archive with `malloc`/`calloc`/
  `realloc`/`free` redirected to the collector (`osp_gc_shim.h`). Fiber / HTTP /
  effect runtime keep libc `malloc` (never collected — status quo, sound).

### `[GC-ARC-PERCEUS]` — reference counting, **default backend** (phase 2)

Precise reference counting following **Perceus** (Reinking, Xie, de Moura,
Leijen, *Perceus: Garbage Free Reference Counting with Reuse*, PLDI 2021):

- **Borrow inference** `[GC-ARC-BORROW]` — owned vs borrowed parameters via the
  `collectO` fix-point of Ullrich & de Moura, *Counting Immutable Beans* (IFL
  2019). Inspectors compile reference-count-free.
- **dup/drop insertion** with borrowing to delay `dup` to the actual last use
  (Perceus λ¹ rules); **drop specialization** (the `is-unique` test) on the hot
  path.
- **Reuse analysis** `[GC-ARC-REUSE]` — `drop-reuse` tokens turn a unique
  matched cell into an in-place write (FBIP), so a functional `map`/`tree-map`
  runs as in-place mutation when uniquely owned.
- **Object header** (Koka model, one 8-byte word): pointer fields laid out
  first, a `scan_fsize` count for the generic drop/trace fallback, a 16-bit
  tag, and a **signed** non-atomic refcount (`0` = unique ⇒ cheapest free test,
  `<0` = cross-fiber / persistent ⇒ the only atomic path).

This requires the codegen work the conservative GC avoids: per-allocation type
info in the header, and a dup/drop insertion pass between type-checking and
codegen (which is today a direct AST→LLVM-text lowering with no SSA IR — the
pass introduces the structured form Perceus needs).

### `[GC-TRACE-CHENEY]` — precise copying GC (phase 3, conformance oracle)

Cheney semi-space copying (Cheney, *A Nonrecursive List Compacting Algorithm*,
CACM 1970) with **precise roots** via an LLVM shadow stack
(`llvm.gcroot`/`"shadow-stack"`) made per-fiber, reusing the phase-2 header type
info for tracing. Bump allocation, free compaction, GC cost ∝ live data. Immix
(Blackburn & McKinley, PLDI 2008) is the later upgrade. Primary role: the
oracle that keeps [MEM-OPAQUE] honest — must be byte-identical to ARC.

## The C ABI (uniform across backends)

```c
void* osp_alloc(int64_t size);          // the existing hook (all backends)
void  osp_retain(void* o);              // dup  — no-op under tracing
void  osp_release(void* o);             // drop — no-op under tracing
void  osp_collect(void);                // full GC — no-op under ARC (acyclic ⇒ complete)
```

`osp_retain`/`osp_release` are no-ops in the tracing backends; `osp_collect` is
a no-op under ARC. That asymmetry is exactly what makes the backends drop-in
swappable while observationally identical.

## Backend selection

Link-time, never in the IR (the IR names only `@osp_alloc`). `--memory=gc`
(default `--memory=default`, future `--memory=arc`) selects the runtime archive
(`libfiber_runtime_<backend>.a` / `libhttp_runtime_<backend>.a`) in the CLI's
`link_args`. The Makefile builds one archive set per backend; the default set is
untouched, so the default build/test path carries zero risk.

## Conformance `[MEM-BACKENDS]`

A backend is conforming iff every differential-harness example produces
byte-identical output and leaks zero language values under it. `make
_conformance-gc` runs `crates/diff_examples.sh` with the backend selected; the
benchmark suite adds an `Osprey (GC)` column so `binarytrees` (905 MiB → a few
MiB) is visible next to the default.

## `[MEM-STATIC-MODE]` — the static "borrow-checker" subset (phase 4)

`--static-memory` fails compilation at every point the ownership analysis would
insert a reference count, naming the shared value and the conflicting owners —
Rust-class output with no runtime memory management, a strict subset of Osprey
that behaves byte-for-byte identically under the default mode. Built on the
phase-2 borrow/ownership analysis (a program is static-mode-clean iff that
analysis inserts no `dup`/`drop` on a shared residue).

## Phasing

1. **Conservative tracing GC** + link-time selection + benchmark column +
   conformance target. ✅ **DONE.**
2. Header type-info in codegen; Perceus borrow inference + dup/drop insertion +
   reuse ⇒ the ARC default backend. ⬜ Not started.
3. Precise Cheney copying GC (per-fiber shadow-stack roots) as the oracle. ⬜
   Not started (depends on phase 2's header).
4. `--static-memory`. ⬜ Not started (depends on phase 2's ownership analysis).

## What is left (detailed)

### Phase 1 — Conservative tracing GC — ✅ DONE

Shipped in `compiler/runtime/memory_gc.c`: a conservative, non-moving mark &
sweep over the managed heap, sound while single-threaded (the first
allocation from a second thread permanently disables collection under one
mutex). `--memory=gc` links `libfiber_runtime_gc.a` / `libhttp_runtime_gc.a`
(built with the `osp_gc_shim.h` malloc→collector redirect for the
value-container units); zero codegen changes. `make _conformance-gc` runs the
differential harness under it; the benchmark README has the `Osprey (GC)`
column. **No remaining work in phase 1.**

- [x] `memory_gc.c` mark & sweep + single-thread-disable soundness.
- [x] GC runtime archives + `osp_gc_shim.h` redirect for list/map units.
- [x] `--memory=gc` CLI link-arg selection; `libfiber/http_runtime_gc.a`.
- [x] `make _conformance-gc` differential harness target.
- [x] `Osprey (GC)` benchmark column.

### Phase 2 — Perceus ARC (the default backend) — ⬜ NOT STARTED

The big one, and the only phase that touches codegen. Prerequisites and steps,
in dependency order:

- [ ] **Per-allocation object header** (Koka/Perceus model, one 8-byte word):
      pointer fields laid out first, a `scan_fsize` count for the generic
      drop/trace fallback, a 16-bit tag, and a **signed non-atomic** refcount
      (`0` = unique ⇒ cheapest free test, `<0` = cross-fiber/persistent ⇒ the
      only atomic path). Codegen must emit this header at every `@osp_alloc`
      site and populate `scan_fsize`/tag from the allocated type. **Nothing
      like this exists today** — the conservative GC needs no header, so this
      is greenfield in `osprey-codegen` and the runtime.
- [ ] **A structured IR (or SSA-ish form) between type-check and codegen.**
      Codegen is a direct AST→LLVM-text lowering with no intermediate form;
      Perceus dup/drop insertion needs explicit binding/last-use structure.
      Decide: a thin CPS/ANF pass, or an owned/borrowed-annotated AST walk.
- [ ] **Borrow inference** `[GC-ARC-BORROW]` — owned vs borrowed parameters
      via the `collectO` fix-point (Ullrich & de Moura, *Counting Immutable
      Beans*). Inspectors compile refcount-free.
- [ ] **dup/drop insertion** (Perceus λ¹ rules) with borrowing to delay `dup`
      to the actual last use; **drop specialization** (the `is-unique` test)
      on the hot path.
- [ ] **Reuse analysis** `[GC-ARC-REUSE]` — `drop-reuse` tokens turn a unique
      matched cell into an in-place write (FBIP) so functional `map`/tree
      updates run in place when uniquely owned.
- [ ] **Wire `osp_retain`/`osp_release`** (currently no-op stubs in
      `memory_gc.c`) to the real signed-refcount dup/drop; add
      `libfiber/http_runtime_arc.a` archives and `--memory=arc`.
- [ ] **Cross-fiber / persistent path**: the atomic refcount branch for
      values escaping a fiber (`<0` refcount), reusing
      `[MEM-FIBER-ISOLATION]` so the common case stays non-atomic.
- [ ] **Conformance**: `--memory=arc` must pass the full differential harness
      byte-identically and leak zero language values; add an `Osprey (ARC)`
      benchmark column.
- [ ] Decide whether ARC *replaces* `default` as the shipped default or stays
      opt-in until the Cheney oracle (phase 3) validates it.

### Phase 3 — Precise Cheney copying GC (conformance oracle) — ⬜ NOT STARTED

Depends on the phase-2 header for precise tracing.

- [ ] Precise roots via an LLVM shadow stack (`llvm.gcroot` /
      `"shadow-stack"`), made **per-fiber**.
- [ ] Cheney semi-space copying (bump alloc, free compaction, cost ∝ live
      data), reusing the phase-2 header type info for tracing.
- [ ] `--memory=cheney` archive; **byte-identical to ARC** on the whole
      harness — this is the oracle that keeps [MEM-OPAQUE] honest.
- [ ] (Later) Immix mark-region upgrade (Blackburn & McKinley).

### Phase 4 — `--static-memory` (the "borrow-checker" subset) — ⬜ NOT STARTED

Built on phase-2's ownership analysis; no runtime component.

- [ ] `--static-memory` fails compilation at every point the ownership
      analysis would insert a `dup`/`drop` on a shared residue, naming the
      shared value and the conflicting owners (Rust-class diagnostics).
- [ ] A program is static-mode-clean iff the phase-2 analysis inserts no
      reference count on a shared residue; verify it behaves byte-for-byte
      identically under the default mode.
- [ ] failscompilation cases for the ownership violations the mode rejects.

### Cross-cutting risks for phases 2–4

- The header + dup/drop pass is the first thing in the compiler that needs a
  form richer than the current AST→text lowering; underestimating that IR
  work is the main schedule risk.
- Every new backend must clear the same conformance bar phase 1 already
  meets: byte-identical harness output and zero leaked language values. The
  Cheney oracle exists specifically to catch ARC bugs the harness output
  alone would not.
- `scan_fsize`/tag must agree between codegen (emit) and the runtime
  (trace/drop); a mismatch corrupts the heap silently — pin it with a shared
  layout test.

## References

- Bacon, Cheng, Rajan. *A Unified Theory of Garbage Collection.* OOPSLA 2004.
- Reinking, Xie, de Moura, Leijen. *Perceus: Garbage Free Reference Counting
  with Reuse.* PLDI 2021.
- Ullrich, de Moura. *Counting Immutable Beans.* IFL 2019.
- Cheney. *A Nonrecursive List Compacting Algorithm.* CACM 13(11), 1970.
- Blackburn, McKinley. *Immix: A Mark-Region Garbage Collector.* PLDI 2008.
- Boehm, Weiser. *Garbage Collection in an Uncooperative Environment.* SP&E 1988.
