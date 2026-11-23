// Compile-time ownership backend for Osprey — the swappable `@osp_alloc`
// implementation selected at link time by `osprey --memory=ownership`.
// Implements [MEM-OWNERSHIP-STATIC] under the [MEM-BACKENDS] ABI
// (docs/specs/compiler/0018-MemoryManagement.md).
//
// The compiler refuses every program in which a value would need a second
// owner, so an accepted program emits no retain and exactly one release per
// value: the release of its one owner. This backend therefore keeps no
// reference counts. A block is in one of four states, held in the header word
// the tracked heap shares with ARC (memory_tracked_heap.h):
//
//   OSP_OWN_OWNED   one owner; its release frees the block and walks its
//                   layout word, freeing every child it owns the same way
//   OSP_OWN_SHARED  a C runtime unit handed out a second reference
//                   (osp_retain); no release frees it, so neither holder can
//                   ever read freed memory. It stays allocated to the end of
//                   the run and the exit report counts it
//   immortal (< 0)  a runtime singleton; never freed
//   parked (0)      on the recycling pool
//
// The value-container units (lists, maps, strings, JSON) link through the
// same allocation shim as under ARC, so their blocks carry headers and live in
// the registry; foreign pointers miss the registry and every hook ignores them.

#define OSP_HEAP_TAG "ownership"
#define OSP_HEAP_DEBUG_ENV "OSPREY_OWNERSHIP_DEBUG"
#define OSP_OWN_OWNED 1
#define OSP_OWN_SHARED 2
// The drop walk holds the one reference an owned child has: that is its last.
#define OSP_HEAP_DROP_LAST(h) ((h)->rc == OSP_OWN_OWNED)
#define OSP_HEAP_EXIT_NOTE() own_report_shared()

static void own_report_shared(void);

#include "memory_tracked_heap.h"

// Blocks a runtime unit gave a second reference to.
static size_t g_own_shared = 0;

static void own_report_shared(void) {
  fprintf(stderr, "[osp-ownership]   shared: %zu blocks kept to exit\n",
          g_own_shared);
}

// A second reference to a block: it can no longer be freed by either holder.
void osp_retain(void *o) {
  if (!o) {
    return;
  }
  heap_lock();
  OspArcHdr *h = arc_lookup_body((uintptr_t)o);
  if (h && h->rc == OSP_OWN_OWNED) {
    h->rc = OSP_OWN_SHARED;
    g_own_shared++;
  }
  heap_unlock();
}

// The owner's release: an owned block is freed with everything it owns.
void osp_release(void *o) {
  if (!o) {
    return;
  }
  heap_lock();
  OspArcHdr *h = arc_lookup_body((uintptr_t)o);
  if (h && h->rc == OSP_OWN_OWNED) {
    arc_release_zero_locked((uintptr_t)o);
  }
  heap_unlock();
}

// Codegen-proved-unique drop (memory_hooks.h): the one owner's release.
void osp_release_unique(void *o) { osp_release(o); }
