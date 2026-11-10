// ARC layouts and lifetimes through the public hooks: tags, retain/release,
// foreign pointers, immortals, layout masks, pointer arrays, list headers, deep
// chains and wide fan-outs [GC-ARC-PERCEUS] [MEM-ACYCLIC]. Part of the memory_arc
// suite; see memory_arc_tests_shared.h.
#include "memory_arc_tests_shared.h"

const char kRodata[] = "osprey-rodata-literal";
static char g_static_buf[64];
void *g_finalized;
static void *g_finalized_data;

// --- allocation --------------------------------------------------------------
void t_alloc_and_tags(void) {
  void *p = osp_alloc(64);
  CHECK(p != NULL && ((uintptr_t)p % W) == 0);
  CHECK(rc_of(p) == 1 && size_of(p) == 64u && meta_of(p) == OSP_MEM_RAW);
  memset(p, 0x5A, 64);
  CHECK(((unsigned char *)p)[0] == 0x5A && ((unsigned char *)p)[63] == 0x5A);
  CHECK(size_of(p) == 64u && rc_of(p) == 1); // body writes miss the header
  void *q = osp_alloc(64); CHECK(q != NULL && q != p);
  osp_release(p); osp_release(q);
  int64_t m = OSP_MEM_LAYOUT(OSP_MEM_WORD(0) | OSP_MEM_WORD(24));
  CHECK((m & 0xFF) == OSP_MEM_MASK);
  CHECK((((uint64_t)m) >> 8) == (OSP_MEM_WORD(0) | OSP_MEM_WORD(24)));
  void *a = mk(32, m), *b = mk(16, OSP_MEM_LIST_HDR_SCALAR);
  void *c = mk(24, OSP_MEM_PTR_ARRAY);
  memset(a, 0, 32); memset(b, 0, 16); memset(c, 0, 24);
  osp_release(a); osp_release(b); osp_release(c);
}

// --- retain / release --------------------------------------------------------
void t_retain_release(void) {
  void *w = witness(), *p = owner(w, 1); CHECK(rc_of(w) == 2);
  osp_retain(p); CHECK(rc_of(p) == 2); osp_retain(p); CHECK(rc_of(p) == 3);
  osp_release(p); CHECK(rc_of(p) == 2 && rc_of(w) == 2); // no walk above zero
  osp_release(p); CHECK(rc_of(p) == 1 && rc_of(w) == 2);
  osp_release(p); // rc 1 -> 0: reclaimed, and walks its layout
  survived(w);
}

void t_release_unique_collect(void) {
  void *w = witness(), *p = owner(w, 1); osp_retain(p); osp_release_unique(p);
  CHECK(rc_of(p) == 1 && rc_of(w) == 2);
  osp_collect(); osp_collect(); // full-collection hook is a no-op under RC
  CHECK(rc_of(p) == 1 && rc_of(w) == 2);
  osp_release_unique(p); // identical to osp_release: drops the child too
  survived(w); osp_release_unique(NULL);
  CHECK(g_checks > 0);
}

// rc==1 release must hand the block back to malloc: assert observable reuse.
void t_reclaim_reuse(void) {
  int reused = 0;
  for (int i = 0; i < 256; i++) {
    void *a = mk(48, OSP_MEM_RAW);
    osp_release(a);
    void *b = mk(48, OSP_MEM_RAW);
    reused += (a == b); // recycled slot is re-headered (mk re-checks rc/size)
    osp_release(b);
  }
  CHECK(reused > 0);
}

static void poke_foreign(void *p) {
  osp_retain(p); osp_retain(p); osp_release(p); osp_release_unique(p);
  osp_mem_immortal(p);
  osp_mem_set_layout(p, OSP_MEM_PTR_ARRAY);
  osp_release(p);
}

void t_foreign_stack_static(void) {
  uint64_t frame[4] = {1, 2, 3, 4};
  poke_foreign(&frame[0]); poke_foreign(&frame[3]);
  CHECK(frame[0] == 1 && frame[3] == 4);
  poke_foreign((void *)(uintptr_t)&kRodata[0]);
  CHECK(kRodata[0] == 'o' && strlen(kRodata) == 21);
  memset(g_static_buf, 0x11, sizeof g_static_buf);
  poke_foreign(g_static_buf);
  CHECK(g_static_buf[0] == 0x11 && g_static_buf[63] == 0x11);
}

void t_foreign_heap_null(void) {
  unsigned char *m = (unsigned char *)malloc(96);
  CHECK(m != NULL); memset(m, 0xAB, 96);
  poke_foreign(m); poke_foreign(m + 8);
  for (size_t i = 0; i < 96; i++) {
    CHECK(m[i] == 0xAB); // a probe miss must never stamp a header
  }
  osp_arc_free(m); // unregistered: falls through to libc free
  osp_retain(NULL); osp_release(NULL); osp_release_unique(NULL);
  osp_mem_immortal(NULL); osp_mem_set_layout(NULL, OSP_MEM_MASK);
  osp_arc_free(NULL);
  void *live = mk(32, OSP_MEM_RAW); // allocator still healthy afterwards
  CHECK(rc_of(live) == 1); osp_release(live);
}

// --- immortality / layout stamping -------------------------------------------
void t_immortal(void) {
  void *w = witness(), *p = owner(w, 1); osp_mem_immortal(p);
  CHECK(rc_of(p) == -1);
  for (int i = 0; i < 1000; i++) {
    osp_retain(p); osp_release(p); osp_release_unique(p);
  }
  CHECK(rc_of(p) == -1 && rc_of(w) == 2); // dup/drop skip rc<0 forever
  osp_mem_immortal(p);
  CHECK(rc_of(p) == -1 && load(p, 0) == w); // still live: never reclaimed
  // Manual free is authoritative for MORTAL objects only: rc < 0 is unreclaim-
  // able by construction, so osp_arc_free skips it (no freed-memory handout).
  osp_arc_free(p);
  CHECK(rc_of(p) == -1 && load(p, 0) == w && rc_of(w) == 2);
  osp_release(w); // p keeps its own reference to w forever, by design
}

void t_set_layout(void) {
  void *w = witness(), *p = mk(W, OSP_MEM_RAW);
  put(p, 0, w); osp_retain(w);
  osp_mem_set_layout(p, OSP_MEM_LAYOUT(OSP_MEM_WORD(0)));
  CHECK(meta_of(p) == OSP_MEM_LAYOUT(OSP_MEM_WORD(0)));
  osp_mem_set_layout(p, OSP_MEM_RAW);
  CHECK(meta_of(p) == OSP_MEM_RAW && rc_of(p) == 1);
  osp_mem_set_layout(p, OSP_MEM_LAYOUT(OSP_MEM_WORD(0)));
  osp_release(p); // walks the stamped layout, not the allocation-time one
  survived(w);
}

// --- MASK drop ---------------------------------------------------------------
void t_mask_drop(void) {
  uint64_t frame = 0;
  void *w = witness(), *child = owner(w, 1); // child rc 1, owns w
  void *keep = witness();                    // rc 1, in an UNMARKED word
  void *p = mk(32, OSP_MEM_LAYOUT(OSP_MEM_WORD(0) | OSP_MEM_WORD(16)));
  put(p, 0, child);
  put_i64(p, 1, (int64_t)SCALAR_FILL); // unmarked scalar: never dereferenced
  put(p, 2, &frame);                   // MARKED foreign pointer: probe miss
  put(p, 3, keep); osp_release(p);
  CHECK(frame == 0);
  CHECK(rc_of(w) == 1);    // child died and walked its own layout
  CHECK(rc_of(keep) == 1); // unmarked word untouched
  osp_release(w); osp_release(keep);
}

void t_mask_bounds_and_bits(void) {
  void *w = witness();
  // Word 0 is in range, word 5 is past `size`: the walk must stop early.
  void *p = mk(W, OSP_MEM_LAYOUT(OSP_MEM_WORD(0) | OSP_MEM_WORD(40)));
  put(p, 0, w); osp_retain(w); osp_release(p); survived(w);
  void *keep = witness(), *nomask = mk(W, OSP_MEM_LAYOUT(0)); // empty mask
  put(nomask, 0, keep); osp_release(nomask);
  CHECK(rc_of(keep) == 1); osp_release(keep);
  void *hi = witness();
  void *top = mk(MASK_BITS * W, OSP_MEM_LAYOUT(OSP_MEM_WORD((MASK_BITS - 1) * 8)));
  memset(top, 0, MASK_BITS * W);
  put(top, MASK_BITS - 1, hi); osp_retain(hi);
  CHECK(size_of(top) == (uint32_t)(MASK_BITS * W));
  osp_release(top); // the 56th and last representable mask bit
  survived(hi);
}

// --- PTR_ARRAY drop ----------------------------------------------------------
void t_ptr_array(void) {
  void *ws[3];
  void *p = mk(8 * W, OSP_MEM_PTR_ARRAY); // slack words stay NULL below
  memset(p, 0, 8 * W);
  for (size_t i = 0; i < 3; i++) {
    ws[i] = witness(); osp_retain(ws[i]); put(p, i, ws[i]);
  }
  osp_release(p);
  for (size_t i = 0; i < 3; i++) { survived(ws[i]); }
}

void t_ptr_array_ragged(void) {
  void *a = witness(), *b = witness();
  void *p = mk(20, OSP_MEM_PTR_ARRAY); // 2 whole words + 4 trailing bytes
  put(p, 0, a); put(p, 1, b); osp_retain(a); osp_retain(b);
  memset((char *)p + 16, 0xFF, 4); // partial word: never read as a pointer
  osp_release(p); survived(a); survived(b);
  void *tiny = mk(4, OSP_MEM_PTR_ARRAY); // under one word: nothing walked
  memset(tiny, 0xFF, 4); osp_release(tiny);
  CHECK(1);
}

// --- LIST_HDR drop -----------------------------------------------------------
// { i64 len, ptr data } over data = [e0,e1,e2, marker]. `marker` hangs off
// data's OWN mask, so its refcount proves `data` itself was released.
static void mk_list(void **hdr, void **elems, void **marker, int64_t kind) {
  void *data = mk(4 * W, OSP_MEM_LAYOUT(OSP_MEM_WORD(24)));
  *marker = witness(); osp_retain(*marker);
  for (size_t i = 0; i < 3; i++) {
    elems[i] = witness(); osp_retain(elems[i]); put(data, i, elems[i]);
  }
  put(data, 3, *marker); *hdr = mk(16, kind);
  put_i64(*hdr, 0, 3); put(*hdr, 1, data);
}

void t_list_hdr_kinds(void) {
  void *hdr, *marker, *elems[3];
  mk_list(&hdr, elems, &marker, OSP_MEM_LIST_HDR_PTR);
  osp_release(hdr);
  for (size_t i = 0; i < 3; i++) { survived(elems[i]); } // data[0..len) dropped
  survived(marker); // data dropped too, and its own layout walked
  mk_list(&hdr, elems, &marker, OSP_MEM_LIST_HDR_SCALAR);
  osp_release(hdr);
  for (size_t i = 0; i < 3; i++) {
    CHECK(rc_of(elems[i]) == 2); // SCALAR: elements are NOT released
    osp_release(elems[i]); osp_release(elems[i]);
  }
  survived(marker); // ...but the data array itself still is
}

void t_list_hdr_edges(void) {
  uint64_t frame[2] = {7, 8};
  void *runt = mk(W, OSP_MEM_LIST_HDR_PTR); // size < 16: early return
  put_i64(runt, 0, 99); osp_release(runt);
  void *empty = mk(16, OSP_MEM_LIST_HDR_PTR); // len 0 with a NULL data
  put_i64(empty, 0, 0); put(empty, 1, NULL); osp_release(empty);
  void *w = witness(), *neg = mk(16, OSP_MEM_LIST_HDR_PTR); // len < 0: no loop
  put_i64(neg, 0, -1); put(neg, 1, w); osp_retain(w);
  osp_release(neg); survived(w);
  void *fgn = mk(16, OSP_MEM_LIST_HDR_PTR); // data is a stack address
  put_i64(fgn, 0, 2); put(fgn, 1, &frame[0]);
  osp_release(fgn); // elements and data all probe-miss
  CHECK(frame[0] == 7 && frame[1] == 8);
}

static void note_finalized(void *body) { g_finalized = body; g_finalized_data = load(body, 1); }

void t_list_hdr_extern(void) {
  void *w = witness(), *hdr = mk(4 * W, OSP_MEM_LIST_HDR_EXTERN);
  put_i64(hdr, 0, 1); put(hdr, 1, w); put_i64(hdr, 2, 0x1234); osp_retain(w);
  osp_mem_set_finalizer(note_finalized);
  osp_release(hdr);
  CHECK(g_finalized == hdr && g_finalized_data == w); // the words were intact when it ran
  survived(w); // ...and data was released after it
  osp_mem_set_finalizer(NULL);
  g_finalized = NULL;
  void *plain = mk(4 * W, OSP_MEM_LIST_HDR_EXTERN); // no finalizer: drops like a scalar header
  w = witness(); put_i64(plain, 0, 0); put(plain, 1, w); osp_retain(w);
  osp_release(plain); survived(w);
  CHECK(g_finalized == NULL);
}

// --- worklist: depth and width -----------------------------------------------
void t_deep_chain(void) {
  void *tail = mk(W, OSP_MEM_LAYOUT(OSP_MEM_WORD(0)));
  put(tail, 0, NULL); osp_retain(tail); void *cur = tail;
  for (int i = 1; i < CHAIN_LEN; i++) {
    void *n = osp_alloc_tagged((int64_t)W, OSP_MEM_LAYOUT(OSP_MEM_WORD(0)));
    CHECK(n != NULL);
    put(n, 0, cur); cur = n;
  }
  CHECK(rc_of(cur) == 1 && rc_of(tail) == 2);
  osp_release(cur); // 100k-deep cascade: worklist, never the C stack
  CHECK(rc_of(tail) == 1);
  osp_retain(tail); CHECK(rc_of(tail) == 2); // still registered afterwards
  osp_release(tail); survived(tail);
}

// osp_alloc_tagged_noinit skips the drop-safety pre-zero: a recycled block
// keeps its prior body bytes (proving no memset ran) while the header — meta,
// rc, size — is still stamped correctly. The zeroing twin, by contrast, hands
// back a clean body. Both must reclaim normally afterwards.
void t_alloc_noinit(void) {
  int64_t mask = OSP_MEM_LAYOUT(OSP_MEM_WORD(1)); // {word0 scalar, word1 child}
  void *a = mk(3 * W, mask);
  memset(a, 0xA5, 3 * W); // dirty the body, then free it to the pool
  osp_release(a);
  // Same-sized noinit request recycles a's slot; the body past word 0 must be
  // UNTOUCHED (word 0 is the recycling pool's intrusive free-list link, so it
  // alone is clobbered — which is why a noinit block MUST store word 0, as a
  // constructor's tag always does).
  void *b = osp_alloc_tagged_noinit((int64_t)(3 * W), OSP_MEM_RAW);
  CHECK(b == a); // recycled the very block we just freed
  CHECK(rc_of(b) == 1 && size_of(b) == (uint32_t)(3 * W) && meta_of(b) == OSP_MEM_RAW);
  for (size_t i = W; i < 3 * W; i++) {
    CHECK(((unsigned char *)b)[i] == 0xA5); // no pre-zero touched words 1..2
  }
  osp_release(b);
  // The zeroing twin over the same slot DOES clear a masked body in full.
  void *c = osp_alloc_tagged((int64_t)(3 * W), mask);
  CHECK(c == a);
  for (size_t i = 0; i < 3 * W; i++) { CHECK(((unsigned char *)c)[i] == 0); }
  put(c, 1, NULL); osp_release(c); // masked child NULL: a clean drop
  // noinit with a real masked child still reclaims the child when fully stored.
  void *w = witness();
  void *p = osp_alloc_tagged_noinit((int64_t)(2 * W), OSP_MEM_LAYOUT(OSP_MEM_WORD(0)));
  CHECK(rc_of(p) == 1);
  put(p, 0, w); put_i64(p, 1, 0); osp_retain(w); // store EVERY word before drop
  CHECK(rc_of(w) == 2);
  osp_release(p); survived(w);
}

// MASK_DIRECT drops children by reading their headers, never the registry —
// the binarytrees fast path. A binary parent (two managed children at words 0
// and 1) over a witness proves the cascade still reaches leaves, and a NULL
// child slot (a never-stored union field) stays a no-op.
void t_mask_direct(void) {
  int64_t direct = ((int64_t)(uint64_t)(OSP_MEM_WORD(0) | OSP_MEM_WORD(W)) << 8) |
                   OSP_MEM_MASK_DIRECT;
  CHECK((direct & 0xFF) == OSP_MEM_MASK_DIRECT);
  void *w = witness();
  void *root = mk(2 * W, direct);
  put(root, 0, owner(w, 1)); // left: a MASK parent holding a reference to w
  put(root, 1, NULL);        // right: an unstored union field — a NULL no-op
  CHECK(rc_of(w) == 2);
  osp_release(root); // header-direct cascade frees left, which drops w
  CHECK(rc_of(w) == 1);
  osp_release(w);
  // An immortal child (rc < 0 — an interned nullary singleton) survives a
  // direct drop untouched.
  void *imm = mk(W, OSP_MEM_RAW); osp_mem_immortal(imm);
  void *p = mk(W, ((int64_t)(uint64_t)OSP_MEM_WORD(0) << 8) | OSP_MEM_MASK_DIRECT);
  put(p, 0, imm);
  osp_release(p); CHECK(rc_of(imm) == -1);
}

void t_wide_fanout(void) {
  void *w = witness();
  void *p = mk(FANOUT * W, OSP_MEM_PTR_ARRAY);
  memset(p, 0, FANOUT * W);
  for (size_t i = 0; i < FANOUT; i++) {
    put(p, i, owner(w, 1)); // every child holds one reference to w
  }
  CHECK(rc_of(w) == FANOUT + 1);
  osp_release(p); // the worklist must grow 256 -> 8192 entries mid-drop
  CHECK(rc_of(w) == 1); osp_release(w);
}
