// Compile-time ownership backend for Osprey — the swappable `@osp_alloc`
// implementation selected at link time by `osprey --memory=ownership`.
// Implements [MEM-OWNERSHIP-STATIC] under the [MEM-BACKENDS] ABI
// (docs/specs/compiler/0018-MemoryManagement.md).
//
// The compiler refuses every program in which a value would need a second
// owner. Accepted ownership IR issues explicit frees. Persistent collection
// helpers still use the transfer states below. This backend keeps no
<<<<<<< HEAD
// reference counts. A block is in one of these states, held in the header word
=======
// reference counts but a handle's sharers. A block is in one of these states, held in the header word
>>>>>>> origin/fixes
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
<<<<<<< HEAD
=======
//   OSP_OWN_HELD+n  a fiber or channel handle with n sharers beyond its first
//                   ([MEM-OWNERSHIP-STATIC-HANDLES]): fibers exist to be
//                   awaited and channels to be shared, so a handle is the one
//                   block a program may share. Each sharer frees it once; the
//                   last free runs its finalizer and frees the block
>>>>>>> origin/fixes
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
<<<<<<< HEAD
// The owner lets go: an owned block is freed; a moving one passes, unfreed, to
// the version that took it; a shared one stays.
#define OSP_HEAP_DROP_LAST(h)                                                  \
  ((h)->rc == OSP_OWN_OWNED ||                                                 \
   ((h)->rc == OSP_OWN_MOVING && ((h)->rc = OSP_OWN_OWNED, 0)))
=======
#define OSP_OWN_HELD 16
// The owner lets go: an owned block is freed; a moving one passes, unfreed, to
// the version that took it; a shared one stays; a handle loses one sharer.
#define OSP_HEAP_DROP_LAST(h)                                                  \
  ((h)->rc == OSP_OWN_OWNED ||                                                 \
   ((h)->rc == OSP_OWN_MOVING && ((h)->rc = OSP_OWN_OWNED, 0)) ||              \
   ((h)->rc > OSP_OWN_HELD &&                                                  \
    ((h)->rc = (h)->rc == OSP_OWN_HELD + 1 ? OSP_OWN_OWNED : (h)->rc - 1, 0)))
>>>>>>> origin/fixes
#define OSP_HEAP_EXIT_NOTE() own_report_shared()

static void own_report_shared(void);

<<<<<<< HEAD
#include "memory_tracked_heap.h"
#include "gpu_buffer.h"

void osp_mem_set_finalizer(void (*finalize)(void *body)) { (void)finalize; }

// GPU allocations own a device handle or mapping. A plain EXTERN body only
// has length and data, so it must never be interpreted as a GPU header.
static void finalize_external(void *body) {
  OspArcHdr *h = arc_hdr(body);
  if (h->meta == OSP_MEM_GPU_BUFFER && h->size >= sizeof(OspreyGpuBuffer)) {
    osp_gpu_drop_header(body);
=======
#include <stdbool.h>

#include "memory_tracked_heap.h"
#include "gpu_buffer.h"

#include "memory_finalizers.h"

// The explicit free of an external header [MEM-BACKENDS-FINALIZERS]. A GPU
// buffer owns a device handle or mapping and is dropped by its layout,
// whatever is registered. A plain EXTERN body (subtype 0) only has length and
// data: it is never read as a GPU header and owns nothing to finalize. Every
// other subtype, a fiber handle ([CONCURRENCY-FIBER-LIFETIME]), goes to the
// finalizer its unit registered.
static void finalize_external(void *body) {
  OspArcHdr *h = arc_hdr(body);
  uint64_t subtype = OSP_MEM_EXTERN_SUBTYPE(h->meta);
  if (h->meta == OSP_MEM_GPU_BUFFER) {
    if (h->size >= sizeof(OspreyGpuBuffer)) {
      osp_gpu_drop_header(body);
    }
  } else if (subtype != 0) {
    finalize_subtype(subtype, body);
>>>>>>> origin/fixes
  }
}

// Blocks taken twice before a handover.
static size_t g_own_shared = 0;

static void own_report_shared(void) {
  fprintf(stderr, "[osp-ownership]   shared: %zu blocks kept to exit\n",
          g_own_shared);
}

<<<<<<< HEAD
// A new version takes the block: an owned one starts moving; taking a moving
// one again would give it two owners, so it is kept instead.
=======
// Gives a live fiber or channel handle one more sharer; false for any other block.
static bool own_share_handle(OspArcHdr *h) {
  if (h->rc <= 0 || OSP_ARC_KIND(h->meta) != OSP_MEM_LIST_HDR_EXTERN ||
      OSP_MEM_EXTERN_SUBTYPE(h->meta) < OSP_MEM_EXTERN_SUBTYPE(OSP_MEM_FIBER_HANDLE)) {
    return false;
  }
  h->rc = h->rc == OSP_OWN_OWNED ? OSP_OWN_HELD + 1 : h->rc + 1;
  return true;
}

// A block that got a second owner the checker should have refused is kept.
static void own_keep(OspArcHdr *h) {
  if (h->rc > 0 && h->rc != OSP_OWN_SHARED) {
    h->rc = OSP_OWN_SHARED;
    g_own_shared++;
  }
}

// A new version takes the block: an owned one starts moving; taking a moving
// one again would give it two owners, so it is kept instead. A handle gains a
// sharer.
>>>>>>> origin/fixes
void osp_retain(void *o) {
  if (!o) {
    return;
  }
  heap_lock();
  OspArcHdr *h = arc_lookup_body((uintptr_t)o);
<<<<<<< HEAD
  if (h && h->rc == OSP_OWN_OWNED) {
    h->rc = OSP_OWN_MOVING;
  } else if (h && h->rc == OSP_OWN_MOVING) {
    h->rc = OSP_OWN_SHARED;
    g_own_shared++;
=======
  if (h && !own_share_handle(h)) {
    if (h->rc == OSP_OWN_OWNED) {
      h->rc = OSP_OWN_MOVING;
    } else if (h->rc == OSP_OWN_MOVING) {
      own_keep(h);
    }
  }
  heap_unlock();
}

// Compiled code shares a handle [MEM-OWNERSHIP-STATIC-HANDLES]. Any other
// block here is one the checker should have refused: it is kept.
void osp_ownership_share(void *o) {
  if (!o) {
    return;
  }
  heap_lock();
  OspArcHdr *h = arc_lookup_body((uintptr_t)o);
  if (h && !own_share_handle(h)) {
    own_keep(h);
>>>>>>> origin/fixes
  }
  heap_unlock();
}

// The owner's release: the tracked heap's drop walk applies the rule above.
void osp_ownership_free(void *o) { tracked_drop(o); }

// Ownership code emits osp_ownership_free for a proved-unique allocation.
void osp_release_unique(void *body) { (void)body; }
