// Default Osprey memory backend — the swappable allocation boundary.
//
// Implements [MEM-BACKENDS] / [MEM-BACKENDS-CUSTOM] (docs/specs/0018). Compiler
// codegen emits calls to `osp_alloc` / `osp_alloc_tagged` and never names
// `malloc`, so the memory manager is chosen at link time, never baked into the
// IR. This default backend is a `malloc` passthrough with no reclamation —
// matching the current "allocate, never free during a run" semantics, which is
// sound because reclamation is unobservable [MEM-OPAQUE]. A custom manager
// (ARC, tracing GC, arena, pool) replaces this object by linking its own
// implementations of the same symbols.
//
// The full backend ABI (docs/plans/0011-arc-gc-implementation.md §C ABI):
// `osp_alloc_tagged` carries a layout word (kind + pointer mask) that only the
// ARC backend reads; `osp_retain`/`osp_release` are dup/drop hooks that only
// the ARC backend acts on; `osp_collect` is a full-collection hook that only
// tracing backends act on. Defining every hook in every backend keeps the
// emitted IR backend-agnostic [MEM-BACKENDS].
//
// The IR-level allocator attributes on `@osp_alloc` / `@osp_alloc_tagged` (see
// osprey-codegen builder.rs OSP_ALLOC_DECL) let LLVM remove provably
// non-escaping allocations at -O2, so most allocations never reach this
// function at all.

#include <stdint.h>
#include <stdlib.h>

void *osp_alloc(int64_t size) { return malloc((size_t)size); }

// Layout-carrying allocation: the meta word (low 8 bits = kind, upper 56 =
// managed-pointer word bitmask) is meaningful only to the ARC backend.
void *osp_alloc_tagged(int64_t size, int64_t meta) {
  (void)meta;
  return malloc((size_t)size);
}

// dup/drop hooks — no-ops without a counting backend (Bacon/Cheng/Rajan
// duality: under tracing or leak-everything, reference counts carry no work).
void osp_retain(void *o) { (void)o; }
void osp_release(void *o) { (void)o; }

// Full-collection hook — nothing to collect in a passthrough backend.
void osp_collect(void) {}
