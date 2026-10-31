#ifndef OSPREY_MEMORY_ARC_DROP_H
#define OSPREY_MEMORY_ARC_DROP_H

// Reclaim a dead body: park it on the recycling pool (slot kept, O(1)) when it
// fits, else remove it from the registry and hand the block back to libc. The
// hot path (small nodes) never probes the registry — the worklist already holds
// the body, and a pooled slot is left in place for the next same-sized alloc.
static void arc_free_or_recycle(void *body) {
  OspArcHdr *h = arc_hdr(body);
  arc_live_dec(h->size);
  h->rc = 0; // pooled sentinel (leak report skips it); harmless if freed below
  if (osp_pool_push(&g_arc_pool, body, osp_pool_cap(h->size))) {
    return; // slot stays present; recycled in place for the next same-size
            // alloc
  }
  uintptr_t *slot = arc_find((uintptr_t)body);
  if (slot) {
    arc_remove(slot); // g_present-- (g_live already decremented above)
  }
  free(h);
}

// --- release: worklist drop over the kind/mask layout
// ---------------------------

// Bounded push-down list of bodies whose rc reached zero (no C recursion, no
// re-entrant locking). Worklist exhaustion emits a fatal diagnostic.
static uintptr_t *g_wl = NULL;
static size_t g_wl_cap = 0;
static size_t g_wl_top = 0;

static void arc_wl_push(uintptr_t body) {
  if (g_wl_top == g_wl_cap) {
    size_t cap = g_wl_cap ? g_wl_cap * 2 : 256;
    uintptr_t *wl = (uintptr_t *)realloc(g_wl, cap * sizeof(uintptr_t));
    if (!wl) {
      // Broken-code quarantine: losing this body would silently abandon its
      // owners.
      fputs("osprey: ARC release worklist exhausted\n", stderr);
      abort();
    }
    g_wl = wl;
    g_wl_cap = cap;
  }
  g_wl[g_wl_top++] = body;
}

// Decrement a candidate child; queue it for freeing when it hits zero.
// A probe miss (foreign/rodata/NULL) or an immortal (rc<0) is a no-op.
static void arc_drop_child(void *child) {
  OspArcHdr *h = arc_lookup_body((uintptr_t)child);
  if (!h) {
    return;
  }
  if (h->rc < 0) {
    return;
  }
  if (--h->rc == 0) {
    arc_wl_push((uintptr_t)child);
  }
}

// MASK_DIRECT child drop: codegen proved every masked word of the parent is
// NULL or an ARC body (a declared-union value can only come from a
// constructor, and constructor blocks always allocate through
// osp_alloc_tagged), so the registry probe above — which dominated
// deep-structure drops at ~52% of binarytrees' CPU — collapses to a direct
// header read. NULL (a zeroed never-stored slot) and immortals stay no-ops.
static void arc_drop_child_direct(void *child) {
  if (!child) {
    return;
  }
  OspArcHdr *h = arc_hdr(child);
  if (h->rc < 0) {
    return;
  }
  if (--h->rc == 0) {
    arc_wl_push((uintptr_t)child);
  }
}

// Bodies are 16-byte aligned (16-byte header + malloc's 16-byte guarantee), so
// every 8-byte word is aligned and a pointer field is a direct load. memcpy of
// 8 bytes is worse than it looks: -D_FORTIFY_SOURCE routes it through
// __memcpy_chk, which never inlines and tails into _platform_memmove — ~7% of
// binarytrees wall was spent copying one pointer at a time in the drop walk.
//
// Layout masks address exact pointer-sized slots on each target. Never scan
// neighboring scalar bytes: an integer may equal an unrelated live address.
#if UINTPTR_MAX == UINT32_MAX
static inline void *arc_load_child(const char *at) {
  uint32_t bits;
  memcpy(&bits, at, sizeof bits);
  return (void *)(uintptr_t)bits;
}
#else
static inline void *arc_load_child(const char *at) {
  return *(void *const *)(const void *)at;
}
#endif

static void arc_drop_masked(char *body, uint32_t size, uint64_t mask,
                            int direct) {
  const size_t stride = sizeof(void *);
  for (unsigned i = 0; mask != 0 && ((size_t)i + 1) * stride <= size;
       i++, mask >>= 1) {
    if (mask & 1) {
      void *child = arc_load_child(body + (size_t)i * stride);
      if (direct) {
        arc_drop_child_direct(child);
      } else {
        arc_drop_child(child);
      }
    }
  }
}

// Finalizer of extern-backed list headers (registered by the list runtime).
static void (*g_finalizer)(void *body);

// { i64 len, i8* data }: run an EXTERN header's finalizer while its words are
// intact, release elements (PTR kind only), then the array. The body is
// `sizeof(i64) + sizeof(pointer)` bytes: 16 natively, 12 on wasm32.
static void arc_drop_list_hdr(char *body, uint32_t size, int kind) {
  if (size < sizeof(int64_t) + sizeof(void *)) {
    return;
  }
  if (kind == OSP_MEM_LIST_HDR_EXTERN && g_finalizer) {
    g_finalizer(body);
  }
  int64_t len;
  memcpy(&len, body, sizeof(len));
  char *data = (char *)arc_load_child(body + 8);
  if (data && kind == OSP_MEM_LIST_HDR_PTR) {
    for (int64_t j = 0; j < len; j++) {
      arc_drop_child(arc_load_child(data + (size_t)j * sizeof(void *)));
    }
  }
  arc_drop_child(data);
}

// A flat array of managed pointers whose length the header's `size` carries —
// the out-of-line internals of the value containers (a HAMT node's `children`,
// a trie node's slots). The mask cannot express these: it is 56 bits wide and a
// collision array is unbounded. Zero words (allocation slack) probe-miss, and
// the slots are `int64_t` holds (8 bytes on every target), so a wasm32 4-byte
// load reads the low half — where an address always lands [MEM-BACKENDS].
static void arc_drop_ptr_array(char *body, uint32_t size) {
  for (size_t off = 0; off + 8 <= size; off += 8) {
    arc_drop_child(arc_load_child(body + off));
  }
}

// Free `start`'s object and everything transitively released by it. Holds
// g_lock. Bodies enter the worklist already confirmed managed (the initial
// caller confirmed membership; arc_drop_child pushes only a registered body
// it just decremented to zero), so no per-node re-probe is needed — the drop
// walk reads the header directly and reclaims via arc_free_or_recycle.
static void arc_release_zero_locked(uintptr_t start) {
  g_wl_top = 0;
  arc_wl_push(start);
  while (g_wl_top > 0) {
    uintptr_t body = g_wl[--g_wl_top];
    OspArcHdr *h = arc_hdr((void *)body);
    int kind = OSP_ARC_KIND(h->meta);
    if (kind == OSP_MEM_MASK || kind == OSP_MEM_MASK_DIRECT) {
      arc_drop_masked((char *)body, h->size, OSP_ARC_MASK_BITS(h->meta),
                      kind == OSP_MEM_MASK_DIRECT);
    } else if (kind == OSP_MEM_LIST_HDR_PTR ||
               kind == OSP_MEM_LIST_HDR_SCALAR ||
               kind == OSP_MEM_LIST_HDR_EXTERN) {
      arc_drop_list_hdr((char *)body, h->size, kind);
    } else if (kind == OSP_MEM_PTR_ARRAY) {
      arc_drop_ptr_array((char *)body, h->size);
    }
    arc_free_or_recycle((void *)body);
  }
}

#endif
