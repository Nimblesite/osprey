#ifndef OSPREY_MEMORY_ARC_ALLOC_H
#define OSPREY_MEMORY_ARC_ALLOC_H

// --- allocation
// ----------------------------------------------------------------

// The header records `size` in a uint32_t, so that is the hard per-object
// ceiling. Rejecting above it does double duty: it keeps h->size EXACT (a
// truncated size would make the drop walk read the wrong number of words and
// g_live_bytes drift), and it makes `OSP_ARC_HDR + size` unable to wrap —
// without the check, size = SIZE_MAX gives malloc(15), which SUCCEEDS and
// returns a body sitting entirely outside the block.
#define OSP_ARC_MAX_BODY ((size_t)UINT32_MAX)

// A body the drop walk will READ as pointers must start zeroed: a field the
// producer never stores would otherwise be walked as a garbage address. RAW
// bodies are never walked, so they keep malloc's speed — and a caller that
// PROVES it stores every masked word before the block can drop (`zero == 0`,
// e.g. a constructor) skips the pre-zero too: ~12% of binarytrees was memset
// re-zeroing recycled Node bodies the constructor overwrites in full.
static void arc_init_body(void *body, int64_t meta, size_t size, int zero) {
  OspArcHdr *h = arc_hdr(body);
  h->meta = meta;
  h->rc = 1;
  h->size = (uint32_t)size;
  if (zero && OSP_ARC_KIND(meta) != OSP_MEM_RAW) {
    memset(body, 0, size);
  }
  arc_live_inc(size);
}

// Header'd, registered allocation. Callers hold g_lock. Recycles a same-sized
// pooled block when one is parked (no malloc, no registry insert); otherwise
// mallocs the ROUNDED capacity so the block is reusable by any request in its
// bucket, and registers it.
static void *arc_alloc_locked(size_t size, int64_t meta, int zero) {
  arc_arm_debug();
  if (size > OSP_ARC_MAX_BODY) {
    return NULL;
  }
  size_t cap = osp_pool_cap(size);
  if (cap <= OSP_POOL_MAX_CAP) {
    void *body = osp_pool_pop(&g_arc_pool, cap);
    if (body) {
      arc_init_body(body, meta, size,
                    zero); // arc_live_inc: slot already present
      return body;
    }
  }
  OspArcHdr *h = (OspArcHdr *)malloc(OSP_ARC_HDR + cap);
  if (!h) {
    return NULL;
  }
  void *body = (char *)h + OSP_ARC_HDR;
  arc_insert((uintptr_t)body);
  arc_init_body(body, meta, size, zero);
  return body;
}

#endif
