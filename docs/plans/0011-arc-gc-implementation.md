# Plan 0011 — Swappable reclaiming memory backends (ARC + tracing GC)

**Status:** Phase 1 (conservative tracing GC) is **shipped and green** —
`--memory=gc` links a mark & sweep archive, the differential harness passes
byte-identically under it (`make _conformance-gc`), and the benchmark suite
carries an `Osprey (GC)` column (binarytrees 2.5 GB → ~11 MB). Phase 2
(the Perceus ARC default) is **in progress — M0–M3 landed and green, plus
M6's last-use drops and the soundness cores of M4/M5**: `--memory=arc`
links a real counting backend (16-byte header, probe-first registry
retain/release, kind/mask drop walk, leak stats); codegen threads a full
ownership ledger (`crates/osprey-codegen/src/arc.rs` — producers own +1,
dup-on-store, slot-based drops at region end, returns transfer +1, drop at
last use); the harness passes byte-identically under **all three backends**
(PASS=148 FAIL=0 each), and `OSPREY_ARC_DEBUG=1` reports **zero live
language values** for container-free programs. Remaining in phase 2:
container *node-level* RC (M4b — containers are currently leak-safe:
headers reclaim, interior trie/HAMT nodes and stored elements do not),
the M5 drain/atomic tail, and M6 borrow inference / drop-guided reuse.
Phases 3–4 (Cheney oracle, `--static-memory`) are **not started**. See
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
Leijen, *Perceus: Garbage Free Reference Counting with Reuse*, PLDI 2021).
The **normative implementation reference is the extended technical report**
[MSR-TR-2020-42 v1](https://www.microsoft.com/en-us/research/wp-content/uploads/2020/11/perceus-tr-v1.pdf) —
it carries what the conference paper compresses: the full λ¹ linear resource
calculus, the complete syntax-directed dup/drop insertion algorithm (Fig. 6–7),
the reference-counted heap semantics with the precision theorem ("garbage
free": a value is freed at its last dynamic use), drop specialization, and the
drop-guided reuse-token rules. Every phase-2 algorithm below names the TR
section it implements:

- **Borrow inference** `[GC-ARC-BORROW]` — owned vs borrowed parameters via the
  `collectO` fix-point of Ullrich & de Moura, *Counting Immutable Beans* (IFL
  2019). Inspectors compile reference-count-free.
- **dup/drop insertion** with borrowing to delay `dup` to the actual last use
  (Perceus λ¹ rules — TR §2.2–2.4, the syntax-directed algorithm of TR Fig. 6
  over owned environments Δ and borrowed environments); **drop specialization**
  (the `is-unique` test expanded inline so the unique path frees with no
  recursive entry — TR §2.5.2) on the hot path.
- **Reuse analysis** `[GC-ARC-REUSE]` — `drop-reuse` tokens (TR §2.5.1,
  drop-guided reuse) turn a unique matched cell into an in-place write (FBIP),
  so a functional `map`/`tree-map` runs as in-place mutation when uniquely
  owned.
- **Object header** (Koka model, one 8-byte word, TR §2.5): a **signed**
  non-atomic refcount (`<0` = cross-fiber / persistent ⇒ the only atomic
  path), a 16-bit `kind`, and 16 bits of flags at `body−8`. Child layout does
  NOT come from a `scan_fsize`-with-pointers-first convention — existing
  ABIs forbid reordering (see Phase 2, Amendment 1) — but from a per-site
  pointer-map word codegen passes to `osp_alloc_tagged(size, meta)`.

This requires the codegen work the conservative GC avoids: per-allocation
layout info at the alloc call, and dup/drop insertion — landed sound-first
(scope-based naive RC) and made precise after (TR Fig. 6–7), because the
backend is a direct AST→LLVM-text lowering with no SSA IR (see Phase 2,
Amendment 3).

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
void* osp_alloc_tagged(int64_t size, int64_t meta); // layout-carrying alloc:
                                        // ARC stores kind+pointer-mask in the
                                        // header; default/gc ignore meta
void  osp_retain(void* o);              // dup  — no-op under tracing; probe-first under ARC
void  osp_release(void* o);             // drop — no-op under tracing; probe-first under ARC
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

### Phase 2 — Perceus ARC (the default backend) — 🟨 IN PROGRESS

The big one, and the only phase that touches codegen. A full-code survey
(2026-07) resolved two questions the original sketch left open, and both
**amend the design**:

**Amendment 1 — the header is `{rc, kind}` + a caller-supplied pointer map,
NOT `scan_fsize` with pointers-first layout.** Every existing aggregate
violates "pointer fields laid out first": records/variants are
`{ i64 tag, fields… }` (tag first), `Result` is `{ T, i8, i8* }` (the errmsg
pointer *last*), closure cells lead with a *code* pointer that must never be
traced, and `HttpResponse` is a fixed C ABI (`http_shared.h`). Reordering
would break the match ABI and the C contracts. Instead codegen — which knows
the exact layout at each of the 16 alloc sites, including per-instantiation
generic boxing (`cast.rs` ptrtoint into `i64` slots) — passes the layout down
at allocation time through one new shared-IR primitive:

```c
void *osp_alloc_tagged(int64_t size, int64_t meta); // meta: low 8 bits = kind,
                                                    // upper 56 = word bitmask
                                                    // (bit i ⇒ the 8-byte word
                                                    // at offset 8·i is a
                                                    // managed pointer)
```

`default`/`gc` backends ignore `meta` (malloc / managed-table passthrough);
the ARC backend stores it in the 8-byte header
(`{ int32_t rc; uint16_t kind; uint16_t flags }` at `body−8`, TR §2.5's Koka
header adapted). Kinds cover the non-mask shapes: `RAW` (opaque bytes),
`MASK` (bitmask-driven drop), `LIST_HDR_PTR`/`LIST_HDR_SCALAR` (flat list
literal header: drop walks `data[0..len]` releasing elements iff PTR, then
releases `data`). `rc` stays **signed**: `<0` is reserved for the
cross-fiber atomic path exactly as originally planned.

**Amendment 2 — provenance is dynamic: an allocation registry, not static
knowledge.** Pointer slots at runtime carry, besides ARC-heap pointers:
rodata string literals (`@.str.N`), constant capture-free closure cells,
static C-runtime error strings (`string_runtime.c` cursor messages), foreign
`malloc`/`strdup` memory (`read_file`, `input`, `termReadKey`, JSON gets),
**borrowed** FFI pointers (`sqlite3_column_text`), opaque FFI handles
(sqlite `Ptr`s), and NULL. No `Value` field or type distinguishes them, and
`extern fn` returns carry no ownership annotation. Therefore
`osp_retain`/`osp_release` **probe an open-addressing registry of live ARC
allocations first** (the exact table discipline `memory_gc.c` already
proves): probe miss ⇒ not ours ⇒ no-op. This makes every non-ARC pointer
safe by construction — no IR special-casing, no FFI annotations needed for
soundness (annotations become a later *precision* upgrade). Registry probes
also protect the type-blind container slots: a raw `i64` that merely looks
like an address can still false-retain (leak-safe) but is never blindly
written through — and codegen-side dup/drop only ever passes values it knows
are pointer-typed, so the collision surface is the container path only.

**Amendment 3 — sound-then-precise sequencing.** The TR's *garbage-free*
precision (drop at last dynamic use, Fig. 6–7), borrow inference, drop
specialization (§2.5.2), and reuse (§2.5.1) are **performance/peak-memory
tiers, not correctness gates**. Conformance (byte-identical output + zero
leaked language values) is met by *complete naive RC*: constructors own
(+1), stores/captures/sends dup, every owner drops at its region end
(function epilogue, match-arm exit before the `br` to the join, mut-cell
rebind, statement end for unbound temporaries). Perceus precision then moves
those drops earlier and elides refcounts without observable change —
verified by the same harness, and later cross-checked by the phase-3 Cheney
oracle. This de-risks the schedule: the codebase's direct AST→text emitter
(specialization-by-inlining *during* emission means position-keyed
annotations are unsound) gets ownership as a ledger threaded through the
existing walk's clean choke points (`bind`/`finish_phi`/single-`ret`), not a
new IR — the thin-ANF rewrite stays an option for the precision tier, not a
prerequisite for soundness.

Milestones, in landing order — **every one keeps the full differential
harness green under BOTH `default` and `arc`**:

- [x] **M0 — symbols everywhere.** ✅ No-op `osp_retain`/`osp_release`/
      `osp_collect`/`osp_alloc_tagged` in the default backend
      (`memory_runtime.c` — and thus the wasm archive, `WASM_RT_SRC` includes
      `memory_runtime`); `osp_alloc_tagged` passthrough in the GC backend.
      Shared IR may reference them under any backend.
- [x] **M1 — the ARC backend core.** ✅ `memory_arc.c`: header'd `osp_alloc`/
      `osp_alloc_tagged` (16-byte header `{meta, rc, size}`, `rc=1`), the
      live-allocation registry (open addressing, tombstone deletion),
      probe-first `osp_retain`/`osp_release`, mask/kind worklist drop (no C
      recursion, no re-entrant locking), `OSPREY_ARC_DEBUG=1` leak stats to
      **stderr** at exit. `osp_arc_shim.h` redirects `malloc`/`calloc`/
      `realloc`/`free`/`strdup` in list/map/hamt/string/string-list/json.
      `FIB_OBJ_ARC`/`HTTP_OBJ_ARC` archives (17 objects incl.
      `coverage_runtime.o`), `--memory=arc` accepted, `make _conformance-arc`.
      **Conformance: PASS=148 FAIL=0 byte-identical** with the ARC archives
      linked — the seam is validated end-to-end exactly as phase 1's GC was.
- [x] **M2 — tagged allocation.** ✅ Codegen's `malloc_struct` takes a
      per-site meta word (`crates/osprey-codegen/src/meta.rs`: kind + mask
      from a struct-layout calculator mirroring LLVM's rules for the
      i64/double/i8*/i8/i1/i32 field set; `Result`'s errmsg word index
      depends on `T`, pinned by unit tests). Records/objects/updates mark
      `Str`/`Ptr` fields; closure cells mark captures but not the leading
      code pointer; effect envs mark cell pointers; list-literal headers
      carry `LIST_HDR_PTR`/`LIST_HDR_SCALAR`; `HttpResponse` keeps its C ABI
      (mask marks words 1, 2, 5). Boxed-generic `i64` slots stay unmarked —
      leak-safe by design. Raw buffers stay plain `osp_alloc` (kind RAW).
      Harness green under both `default` and `arc`.
- [x] **M3 — sound naive RC** ✅ (conformance milestone): dup-on-store /
      drop-at-region-end per Amendment 3, ownership ledger in
      `crates/osprey-codegen/src/arc.rs` (swapped wholesale across
      `enter_nested_fn`/`exit_nested_fn`). Callee borrows arguments; every
      producer owns (+1); returns transfer +1 (function/closure/handler-arm
      epilogues retain-return-then-release-owners). Mut-cell rebind releases
      the old value (`gen_cell_store`). Match arms / guard diamonds / loops
      need **no special drop placement**: every owner spills to a
      null-initialized entry-block slot (`%arc.sN`), region-end drops load
      the slot, release, and re-null it — untaken paths leave slots null and
      `osp_release(null)`/probe-miss is a no-op, so drops are dominance-free
      by construction (this replaces the planned before-join-`br` and
      `finish_phi` special cases). Per-statement and per-loop-iteration
      regions (`gen_local_stmt`, `iter.rs` loop bodies); fold accumulators
      retain-new-before-release-old keyed on static type. C-runtime
      `Ret::Str` returns owned (+1 via the M1 shim). Leak gate **met**:
      container-free harness examples report **zero live language values at
      exit** under `OSPREY_ARC_DEBUG=1`.
- [x] **M4a — container soundness under codegen ownership.** ✅ The pieces
      without which M3's owned handles corrupt under `--memory=arc`:
      alias-returning ops **retain-on-return** (`osprey_list_concat`/`_drop`,
      `osprey_map_remove`/`_merge` — every return path now carries +1);
      the empty-list singleton is **immortal** (`osp_mem_immortal`, rc<0,
      declared backend-neutrally in `memory_hooks.h` with no-ops in
      `memory_runtime.c`/`memory_gc.c`); elements/keys are **dup'd at
      insertion** on the codegen side, keyed on the static HM type
      (`stored_boxed_arg` — append/prepend, `mapSet`, map literals, built
      lists' mapped elements); `osprey_map_merge` releases its unaliased
      intermediate headers. Containers are **leak-safe, not garbage-free**:
      dead headers reclaim; interior trie/HAMT nodes and stored elements
      intentionally leak (a container never releases, so sharing can never
      dangle). Flat list-literal headers (`LIST_HDR_PTR`/`SCALAR`) drop
      precisely, elements included.
- [ ] **M4b — container node RC** (garbage-free containers): element-kind
      flags at container creation, node refcounts inside the C ops
      (`clone_node`'s 31 shared children = 31 dups; path-copy sharing,
      `merge_leaves` node reuse), leaf-element release keyed on the flag,
      immortal/out-of-line internal arrays (`children`, `coll_*`,
      string-list `items`) freed with their owning node, map string *keys*
      released like values. This is where "zero leaked language values"
      extends to container programs.
- [x] **M5a — boundary dups + the spawn-cell release.** ✅ Every
      pointer-erasing boundary dups before `box_to_i64` (single choke point
      `box_codegen_value` for effects; `thunk_body`/`gen_send` for fibers;
      fold-accumulator slots), so the receiving side always holds +1.
      Spawn-capture cells: the spawn transfers its cell (+1) to the runtime
      and `run_fiber_fn` releases it when the thunk returns (backend-neutral
      via `memory_hooks.h`). Handler env cells release at handler close
      (`gen_handler`) / after `coro_free` (resuming handlers). Effect
      `perform` results and resuming-handle results are owned, balanced by
      arm/body escape-retains.
- [ ] **M5b — boundary drains + the atomic tier.** Channel buffers drained
      at channel end, coro mailboxes (`args[16]`/`resume_value`/`result`)
      drained in `__osprey_coro_abort`, `test_runtime.c`'s `skip_reason`,
      the handler-returned `HttpResponse` (runtime releases after send),
      `recv`'s untracked unboxed result. Until then these edges are
      leak-safe (values arrive +1 and are never released). The **atomic
      refcount branch** (`rc<0`) flips at the *syntactic* boundary
      (spawn/send/snapshot) — never off runtime threading — so deterministic
      and threaded fiber modes refcount identically ([MEM-BACKENDS]
      byte-identical rule); v1 soundness comes from the global mutex.
- [x] **M6a — drop at last use.** ✅ TR Fig. 6's owned-environment insertion:
      after each function-level statement, `release_dead_after` computes the
      continuation's free identifiers (`freevars::free_idents_of_stmts`) and
      drops `let`-bound owners the rest of the block never references —
      gated to ledger depth 1 so a nested block/loop region can never free
      an enclosing scope's names (a tail block IS the continuation, so the
      gate composes). Applies to `main`'s top level too.
- [ ] **M6b — Perceus precision tier** `[GC-ARC-BORROW]` `[GC-ARC-REUSE]`:
      `collectO` borrow inference (Ullrich & de Moura), drop specialization
      (TR §2.5.2), drop-guided reuse/FBIP (§2.5.1). Byte-identical by
      construction; peak-RSS deltas land in the benchmark table. Revisit the
      thin-ANF IR here if in-walk insertion proves too entangled.
- [ ] **Conformance & benchmarks**: `--memory=arc` passes the full harness
      byte-identically with zero leaked language values; `Osprey (ARC)`
      benchmark column (binarytrees peak RSS next to default's 2.5 GB and
      GC's ~11 MB). *Current state: harness byte-identical under all three
      backends (PASS=148 each); zero live values holds for container-free
      programs, container programs await M4b.*
- [ ] Decide whether ARC *replaces* `default` as the shipped default or stays
      opt-in until the Cheney oracle (phase 3) validates it.

Known perf note: `retain`/`release` calls on an allocation defeat LLVM's
dead-allocation elimination (the `OSP_ALLOC_DECL` attributes) until drop
specialization / borrow inference (M6) removes them from non-escaping paths;
`osp_alloc_tagged` carries the same allocator attributes so the default
backend keeps today's `-O2` behaviour throughout.

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
  form richer than the current AST→text lowering; the sound-first sequencing
  (Amendment 3) bounds that risk, but the M6 precision tier may still force
  the thin-ANF IR — underestimating that work is the main schedule risk.
- Every new backend must clear the same conformance bar phase 1 already
  meets: byte-identical harness output and zero leaked language values. The
  Cheney oracle exists specifically to catch ARC bugs the harness output
  alone would not.
- The kind+mask meta must agree between codegen (emit) and the runtime
  (drop/trace); a mismatch corrupts the heap silently — pin it with a shared
  layout test (the Rust layout calculator vs C `offsetof` asserts).
- Container slots are type-blind `i64`s; the registry probe makes a scalar
  that collides with a live ARC address *leak-safe* (false retain) but the
  container milestone (M4) must never blindly release raw slots — element
  releases key off the creation-time element-kind flag, nothing else.
- FFI (`extern fn`) returns are foreign/borrowed and stay unmanaged via
  registry probe-miss; a later precision upgrade may add per-extern
  ownership annotations, but soundness never depends on them.

## References

- **Reinking, Xie, de Moura, Leijen. *Perceus: Garbage Free Reference Counting
  with Reuse.* Microsoft Research Technical Report MSR-TR-2020-42 v1, Nov 2020.
  <https://www.microsoft.com/en-us/research/wp-content/uploads/2020/11/perceus-tr-v1.pdf>
  — the normative phase-2 reference: λ¹ calculus and heap semantics (§2.2–2.3),
  syntax-directed dup/drop insertion (Fig. 6–7), the garbage-free precision
  theorem, drop specialization (§2.5.2), drop-guided reuse (§2.5.1), and the
  Koka object header layout this plan adopts.** The PLDI 2021 paper of the same
  name is the archival version; where they differ in detail, follow the TR.
- Bacon, Cheng, Rajan. *A Unified Theory of Garbage Collection.* OOPSLA 2004.
- Ullrich, de Moura. *Counting Immutable Beans.* IFL 2019.
- Cheney. *A Nonrecursive List Compacting Algorithm.* CACM 13(11), 1970.
- Blackburn, McKinley. *Immix: A Mark-Region Garbage Collector.* PLDI 2008.
- Boehm, Weiser. *Garbage Collection in an Uncooperative Environment.* SP&E 1988.
