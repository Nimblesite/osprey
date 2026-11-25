// Compile-time ownership backend for Osprey — the swappable `@osp_alloc`
// implementation selected at link time by `osprey --memory=ownership`.
// Implements [MEM-OWNERSHIP-STATIC] under the [MEM-BACKENDS] ABI
// (docs/specs/compiler/0018-MemoryManagement.md).
//
// The compiler refuses every program in which a value would need a second
// owner. Accepted ownership IR issues explicit frees. Persistent collection
// helpers still use the transfer states below. This backend keeps no
// reference counts. A block is in one of these states, held in the header word
// the tracked heap shares with ARC (memory_tracked_heap.h):
//
//   OSP_OWN_OWNED   one owner; its release frees the block and walks its
//                   layout word, freeing every child it owns the same way
//   OSP_OWN_MOVING  a persistent list or map is deriving a new version from
//                   a moved source ([MEM-OWNERSHIP-STATIC-SHARING]): the new
//                   version took the node (osp_retain) and the source's
//                   release, which follows the call at once, hands it over
//                   unfreed, so the new version owns it alone
//   OSP_OWN_SHARED  a second take before the handover: a program the checker
//                   should have refused. No release frees the block, so no
//                   holder can read freed memory; it stays allocated to the
//                   end of the run and the exit report counts it
//   immortal (< 0)  a runtime singleton; never freed
//   parked (0)      on the recycling pool
//
// The value-container units (lists, maps, strings, JSON) link through the
// same allocation shim as under ARC, so their blocks carry headers and live in
// the registry; foreign pointers miss the registry and every hook ignores them.

#define OSP_HEAP_TAG "ownership"
#define OSP_HEAP_DEBUG_ENV "OSPREY_OWNERSHIP_DEBUG"
#define OSP_OWN_OWNED 1
#define OSP_OWN_MOVING 2
#define OSP_OWN_SHARED 3
// The owner lets go: an owned block is freed; a moving one passes, unfreed, to
// the version that took it; a shared one stays.
#define OSP_HEAP_DROP_LAST(h)                                                  \
  ((h)->rc == OSP_OWN_OWNED ||                                                 \
   ((h)->rc == OSP_OWN_MOVING && ((h)->rc = OSP_OWN_OWNED, 0)))
#define OSP_HEAP_EXIT_NOTE() own_report_shared()

static void own_report_shared(void);

#include "memory_tracked_heap.h"
#include "gpu_buffer.h"

#include "memory_finalizers.h"

// GPU allocations own a device handle or mapping, dropped by their layout. A
// plain EXTERN body (subtype 0) only has length and data: it is never read as
// a GPU header and owns nothing to finalize. Every other subtype, a fiber
// handle, goes to its registered finalizer.
static void finalize_external(void *body) {
  OspArcHdr *h = arc_hdr(body);
  uint64_t subtype = OSP_MEM_EXTERN_SUBTYPE(h->meta);
  if (h->meta == OSP_MEM_GPU_BUFFER) {
    if (h->size >= sizeof(OspreyGpuBuffer)) {
      osp_gpu_drop_header(body);
    }
  } else if (subtype != 0) {
    finalize_subtype(subtype, body);
  }
}

// Blocks taken twice before a handover.
static size_t g_own_shared = 0;

static void own_report_shared(void) {
  fprintf(stderr, "[osp-ownership]   shared: %zu blocks kept to exit\n",
          g_own_shared);
}

// A new version takes the block: an owned one starts moving; taking a moving
// one again would give it two owners, so it is kept instead.
void osp_retain(void *o) {
  if (!o) {
    return;
  }
  heap_lock();
  OspArcHdr *h = arc_lookup_body((uintptr_t)o);
  if (h && h->rc == OSP_OWN_OWNED) {
    h->rc = OSP_OWN_MOVING;
  } else if (h && h->rc == OSP_OWN_MOVING) {
    h->rc = OSP_OWN_SHARED;
    g_own_shared++;
  }
  heap_unlock();
}

// The owner's release: the tracked heap's drop walk applies the rule above.
void osp_ownership_free(void *o) { tracked_drop(o); }

// Ownership code emits osp_ownership_free for a proved-unique allocation.
void osp_release_unique(void *body) { (void)body; }
