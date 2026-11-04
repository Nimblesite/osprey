// Size-classed intrusive recycling free-list for a reclaiming allocator.
// Implements [MEM-BACKENDS] / [GC-ARC-PERCEUS]. Factored out of memory_arc.c so
// the pool mechanics stand apart from ARC's registry bookkeeping and can be
// unit-tested (memory_pool_tests.c) in isolation.
//
// A dead body is not returned to libc: it is pushed onto a per-size-class list
// whose links live INSIDE the freed bodies (the first word of a parked body is
// the next-free pointer), so a later same-rounded-size allocation pops the SAME
// block — no malloc, no free, no allocator metadata churn. binarytrees alone
// runs ~19.6M alloc/free pairs through here; recycling turns each into a stack
// push/pop and keeps the resident set (one depth-13 tree) hot.
//
// Used by the ARC backend, whose immediate (refcount-driven) reclamation keeps
// the reused set cache-hot. NOT used by the tracing GC: a collector cannot
// reclaim a body until its next collection — a whole heap budget of allocation
// later — so pooled reuse there cycles a cold, multi-MiB working set and
// measured ~12x SLOWER than just leaning on libc's own hot per-size free-lists.
// A caller owns a private `OspPool` plus its surrounding logic and holds its own
// lock: this structure is not internally synchronised.
#ifndef OSP_MEMORY_POOL_H
#define OSP_MEMORY_POOL_H

#include <stddef.h>

#define OSP_POOL_GRAIN ((size_t)16)    // capacity granularity (keeps 16-align)
#define OSP_POOL_BUCKETS ((size_t)256) // ceiling = 256 * 16 = 4096 body bytes
#define OSP_POOL_MAX_CAP (OSP_POOL_BUCKETS * OSP_POOL_GRAIN)
#define OSP_POOL_DEFAULT_CAP_BYTES ((size_t)64u << 20) // retention ceiling

typedef struct {
  void *heads[OSP_POOL_BUCKETS + 1]; // free-list heads, indexed by cap/GRAIN
  size_t bytes;                      // bytes currently parked on the lists
  size_t cap_bytes;                  // retention ceiling; 0 selects the default
} OspPool;

// The rounded capacity (>= GRAIN, a multiple of GRAIN) a body of `size`
// occupies. Deterministic, so a block freed to bucket k always holds exactly
// k*GRAIN bytes and a request mapping to bucket k needs no more — reuse can
// never under-allocate.
static inline size_t osp_pool_cap(size_t size) {
  size_t s = size ? size : 1;
  return (s + (OSP_POOL_GRAIN - 1)) & ~(OSP_POOL_GRAIN - 1);
}

// Pop a reusable body of exactly `cap` bytes, or NULL when the class is empty.
static inline void *osp_pool_pop(OspPool *pool, size_t cap) {
  size_t b = cap / OSP_POOL_GRAIN;
  void *body = pool->heads[b];
  if (body) {
    pool->heads[b] = *(void **)body; // unlink: first word is the next-free link
    pool->bytes -= cap;
  }
  return body;
}

// Park a dead body of `cap` bytes on its free-list. Returns 0 when oversized or
// past the retention ceiling, so the caller hands it back to libc instead.
static inline int osp_pool_push(OspPool *pool, void *body, size_t cap) {
  size_t ceiling = pool->cap_bytes ? pool->cap_bytes : OSP_POOL_DEFAULT_CAP_BYTES;
  if (cap > OSP_POOL_MAX_CAP || pool->bytes + cap > ceiling) {
    return 0;
  }
  size_t b = cap / OSP_POOL_GRAIN;
  *(void **)body = pool->heads[b];
  pool->heads[b] = body;
  pool->bytes += cap;
  return 1;
}

#endif // OSP_MEMORY_POOL_H
