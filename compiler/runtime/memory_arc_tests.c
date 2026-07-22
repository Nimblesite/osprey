// Vanilla-C tests for runtime/memory_arc.c — the Perceus ARC backend
// ([GC-ARC-PERCEUS], docs/plans/0011 phase 2; [MEM-BACKENDS], docs/specs/0018).
//
//   cc -O2 -D_FORTIFY_SOURCE=2 -fstack-protector-strong -Werror -Wall -Wextra \
//      -ftrapv -std=c11 -D_GNU_SOURCE runtime/memory_arc_tests.c \
//      runtime/memory_arc.c -pthread -o bin/memory_arc_tests
//
// Never built with osp_arc_shim.h: the suite needs the real libc malloc/free to
// build the foreign-pointer fixtures the probe-miss paths are tested against.
//
// memory_arc.c exports no live-count symbol, so liveness is asserted
// structurally: the header {int64 meta; int32 rc; uint32 size} at body-16 is
// read back with memcpy (never a struct alias), and reclamation is proved by
// *witness* objects — a leaf whose refcount can only move if its owner was
// really dropped and its layout word really walked, which proves a free
// transitively without the test ever reading freed memory.
#include "memory_hooks.h"

#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Not declared in memory_hooks.h: the raw allocator, the collect hook, and the
// osp_arc_shim.h redirect targets.
void *osp_alloc(int64_t size);
void osp_collect(void);
void *osp_arc_malloc(size_t size);
void *osp_arc_calloc(size_t n, size_t size);
void *osp_arc_realloc(void *old, size_t size);
void osp_arc_free(void *p);
char *osp_arc_strdup(const char *s);

static long g_checks = 0;
#define CHECK(c) do { g_checks++; assert(c); } while (0)

#define W ((size_t)8)             // managed word size
#define HDR_META_OFF ((size_t)16) // header fields, backwards from the body
#define HDR_RC_OFF ((size_t)8)
#define HDR_SIZE_OFF ((size_t)4)
#define ARC_INIT_CAP ((size_t)1u << 14) // mirrors OSP_ARC_INIT_CAP
#define MASK_BITS 56                    // layout-mask width
#define PREVIEW_LEN 24                  // mirrors OSP_ARC_PREVIEW
#define SCALAR_FILL 0xDEADBEEFCAFEF00Dull
#define CHAIN_LEN 100000
#define FANOUT 4096
#define CHURN_LIVE 6000 // > INIT_CAP/4: forces at least one real rehash
#define CHURN_ITERS 60000
#define LONG_LEN 4096

static int32_t rc_of(const void *b) {
  int32_t v; memcpy(&v, (const char *)b - HDR_RC_OFF, sizeof v); return v;
}
static uint32_t size_of(const void *b) {
  uint32_t v; memcpy(&v, (const char *)b - HDR_SIZE_OFF, sizeof v); return v;
}
static int64_t meta_of(const void *b) {
  int64_t v; memcpy(&v, (const char *)b - HDR_META_OFF, sizeof v); return v;
}
static void *load(const void *body, size_t word) {
  void *v; memcpy(&v, (const char *)body + word * W, sizeof v); return v;
}
static void put(void *body, size_t word, const void *val) {
  memcpy((char *)body + word * W, &val, sizeof val);
}
static void put_i64(void *body, size_t word, int64_t v) {
  memcpy((char *)body + word * W, &v, sizeof v);
}
static void *mk(size_t bytes, int64_t meta) {
  void *p = osp_alloc_tagged((int64_t)bytes, meta);
  CHECK(p != NULL); CHECK(rc_of(p) == 1);
  CHECK(size_of(p) == (uint32_t)bytes); CHECK(meta_of(p) == meta);
  return p;
}
static void *witness(void) { return mk(W, OSP_MEM_RAW); }
// MASK parent holding one reference to `child` at word 0; `dup` picks between
// taking a fresh reference and adopting the caller's.
static void *owner(void *child, int dup) {
  void *p = mk(W, OSP_MEM_LAYOUT(OSP_MEM_WORD(0)));
  put(p, 0, child);
  if (dup) { osp_retain(child); }
  return p;
}
// The witness outlived its owner's drop with exactly one reference left.
static void survived(void *w) { CHECK(rc_of(w) == 1); osp_release(w); }
static uint64_t rnd(uint64_t *s) {
  *s = *s * 6364136223846793005ull + 1442695040888963407ull;
  return *s >> 33;
}

// --- allocation --------------------------------------------------------------
static void t_alloc_and_tags(void) {
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
static void t_alloc_sizes_oom(void) {
  void *zero = osp_alloc(0); // malloc(HDR+1): still a distinct live address
  void *neg = osp_alloc(-8); // size <= 0 clamps to 0
  void *min = osp_alloc(INT64_MIN), *one = osp_alloc(1);
  CHECK(zero && neg && min && one);
  CHECK(size_of(zero) == 0u && size_of(neg) == 0u && size_of(min) == 0u);
  CHECK(size_of(one) == 1u && rc_of(zero) == 1 && rc_of(min) == 1);
  CHECK(zero != neg && neg != min && min != one);
  osp_release(zero); osp_release(neg); osp_release(min); osp_release(one);
  CHECK(osp_alloc((int64_t)1 << 62) == NULL); // no such address space
  CHECK(osp_alloc_tagged(INT64_MAX, OSP_MEM_RAW) == NULL);
  CHECK(osp_arc_calloc((size_t)1 << 40, 4096) == NULL);
  // uint32_t `size` in the header is the hard per-object ceiling: oversize is
  // REJECTED, never truncated (truncation misdirects every drop walk, and
  // OSP_ARC_HDR + SIZE_MAX wraps to a malloc(15) that SUCCEEDS).
  CHECK(osp_alloc_tagged((int64_t)UINT32_MAX + 1, OSP_MEM_RAW) == NULL);
  CHECK(osp_arc_malloc((size_t)-1) == NULL);
  CHECK(osp_arc_calloc((size_t)-1, 2) == NULL); // n * size overflows first
  osp_release(mk(32, OSP_MEM_RAW)); // allocator state survives OOM
}
// --- retain / release --------------------------------------------------------
static void t_retain_release(void) {
  void *w = witness(), *p = owner(w, 1); CHECK(rc_of(w) == 2);
  osp_retain(p); CHECK(rc_of(p) == 2); osp_retain(p); CHECK(rc_of(p) == 3);
  osp_release(p); CHECK(rc_of(p) == 2 && rc_of(w) == 2); // no walk above zero
  osp_release(p); CHECK(rc_of(p) == 1 && rc_of(w) == 2);
  osp_release(p); // rc 1 -> 0: reclaimed, and walks its layout
  survived(w);
}
static void t_release_unique_collect(void) {
  void *w = witness(), *p = owner(w, 1); osp_retain(p); osp_release_unique(p);
  CHECK(rc_of(p) == 1 && rc_of(w) == 2);
  osp_collect(); osp_collect(); // full-collection hook is a no-op under RC
  CHECK(rc_of(p) == 1 && rc_of(w) == 2);
  osp_release_unique(p); // identical to osp_release: drops the child too
  survived(w); osp_release_unique(NULL);
  CHECK(g_checks > 0);
}
// rc==1 release must hand the block back to malloc: assert observable reuse.
static void t_reclaim_reuse(void) {
  int reused = 0;
  for (int i = 0; i < 256; i++) {
    void *a = mk(48, OSP_MEM_RAW);
    osp_release(a);
    void *b = mk(48, OSP_MEM_RAW);
    reused += (a == b);
    CHECK(rc_of(b) == 1 && size_of(b) == 48u); // recycled slot is re-headered
    osp_release(b);
  }
  CHECK(reused > 0);
}
// --- probe-miss safety (foreign pointers) ------------------------------------
static const char kRodata[] = "osprey-rodata-literal";
static char g_static_buf[64];
static void poke_foreign(void *p) {
  osp_retain(p); osp_retain(p); osp_release(p); osp_release_unique(p);
  osp_mem_immortal(p);
  osp_mem_set_layout(p, OSP_MEM_PTR_ARRAY);
  osp_release(p);
}
static void t_foreign_stack_static(void) {
  uint64_t frame[4] = {1, 2, 3, 4};
  poke_foreign(&frame[0]); poke_foreign(&frame[3]);
  CHECK(frame[0] == 1 && frame[3] == 4);
  poke_foreign((void *)(uintptr_t)&kRodata[0]);
  CHECK(kRodata[0] == 'o' && strlen(kRodata) == 21);
  memset(g_static_buf, 0x11, sizeof g_static_buf);
  poke_foreign(g_static_buf);
  CHECK(g_static_buf[0] == 0x11 && g_static_buf[63] == 0x11);
}
static void t_foreign_heap_null(void) {
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
static void t_immortal(void) {
  void *w = witness(), *p = owner(w, 1); osp_mem_immortal(p);
  CHECK(rc_of(p) == -1);
  for (int i = 0; i < 1000; i++) {
    osp_retain(p); osp_release(p); osp_release_unique(p);
  }
  CHECK(rc_of(p) == -1 && rc_of(w) == 2); // dup/drop skip rc<0 forever
  osp_mem_immortal(p);
  CHECK(rc_of(p) == -1 && load(p, 0) == w); // still live: never reclaimed
  // Manual free is authoritative for MORTAL objects only: rc < 0 means
  // unreclaimable by construction (many call sites return one singleton), so
  // osp_arc_free skips it rather than hand the next caller freed memory.
  osp_arc_free(p);
  CHECK(rc_of(p) == -1 && load(p, 0) == w && rc_of(w) == 2);
  osp_release(w); // p keeps its own reference to w forever, by design
}
static void t_set_layout(void) {
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
static void t_mask_drop(void) {
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
static void t_mask_bounds_and_bits(void) {
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
static void t_ptr_array(void) {
  void *ws[3];
  void *p = mk(8 * W, OSP_MEM_PTR_ARRAY); // slack words stay NULL below
  memset(p, 0, 8 * W);
  for (size_t i = 0; i < 3; i++) {
    ws[i] = witness(); osp_retain(ws[i]); put(p, i, ws[i]);
  }
  osp_release(p);
  for (size_t i = 0; i < 3; i++) { survived(ws[i]); }
}
static void t_ptr_array_ragged(void) {
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
static void t_list_hdr_kinds(void) {
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
static void t_list_hdr_edges(void) {
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
// --- worklist: depth and width -----------------------------------------------
static void t_deep_chain(void) {
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
static void t_wide_fanout(void) {
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
// --- registry growth, tombstone churn, slot reuse ----------------------------
static void churn_slot(void **live, size_t i, uint64_t *seed) {
  if (live[i]) { CHECK(rc_of(live[i]) == 1); osp_release(live[i]); }
  size_t bytes = (size_t)(rnd(seed) % 64) + W;
  live[i] = mk(bytes, OSP_MEM_RAW);
  memset(live[i], (int)(i & 0xFF), bytes);
}
static void churn_probe(void **live, size_t i) {
  if (!live[i]) { return; }
  osp_retain(live[i]); // probes travel through tombstoned chains
  CHECK(rc_of(live[i]) == 2); osp_release(live[i]);
  CHECK(rc_of(live[i]) == 1);
}
static void t_registry_churn(void) {
  void **live = (void **)calloc(CHURN_LIVE, sizeof(void *));
  CHECK(live != NULL); uint64_t seed = 0x0DDBA11ull;
  for (int n = 0; n < CHURN_ITERS; n++) {
    churn_slot(live, (size_t)(rnd(&seed) % CHURN_LIVE), &seed);
    churn_probe(live, (size_t)(rnd(&seed) % CHURN_LIVE));
  }
  for (size_t i = 0; i < CHURN_LIVE; i++) {
    CHECK(live[i] != NULL && rc_of(live[i]) == 1);
    osp_release(live[i]);
  }
  free(live);
  CHECK((size_t)CHURN_LIVE * 4 > ARC_INIT_CAP);
}
static void t_slot_reuse(void) {
  void *a = mk(24, OSP_MEM_LAYOUT(OSP_MEM_WORD(0)));
  memset(a, 0, 24); osp_release(a);
  for (int i = 0; i < 32; i++) { // re-registering freed (tombstoned) slots
    void *b = mk(24, OSP_MEM_PTR_ARRAY);
    memset(b, 0, 24);
    CHECK(meta_of(b) == OSP_MEM_PTR_ARRAY); // never a stale header
    osp_retain(b); CHECK(rc_of(b) == 2);
    osp_release(b); osp_release(b);
  }
  CHECK(1);
}
// --- osp_arc_shim.h entry points ---------------------------------------------
static void t_shim_malloc_calloc_free(void) {
  void *m = osp_arc_malloc(0);
  CHECK(m != NULL && size_of(m) == 0u && rc_of(m) == 1);
  osp_arc_free(m);
  unsigned char *c = (unsigned char *)osp_arc_calloc(4, 8);
  CHECK(c != NULL && size_of(c) == 32u && meta_of(c) == OSP_MEM_RAW);
  for (size_t i = 0; i < 32; i++) { CHECK(c[i] == 0); } // calloc zeroes
  osp_arc_free(c);
  CHECK(osp_arc_calloc((size_t)-1, 2) == NULL); // overflow guard
  CHECK(osp_arc_calloc(2, (size_t)-1) == NULL);
  void *z = osp_arc_calloc(0, 8); // n == 0 short-circuits the guard
  CHECK(z != NULL && size_of(z) == 0u); osp_arc_free(z);
  void *w = witness(), *p = owner(w, 1);
  osp_retain(p); osp_retain(p); CHECK(rc_of(p) == 3);
  osp_arc_free(p); // authoritative: frees at any rc, and walks the layout
  survived(w);
}
static void t_shim_realloc_grow(void) {
  unsigned char *p = (unsigned char *)osp_arc_malloc(32);
  CHECK(p != NULL);
  for (size_t i = 0; i < 32; i++) { p[i] = (unsigned char)(i + 1); }
  unsigned char *q = (unsigned char *)osp_arc_realloc(p, 128);
  CHECK(q != NULL && size_of(q) == 128u && rc_of(q) == 1);
  for (size_t i = 0; i < 32; i++) {
    CHECK(q[i] == (unsigned char)(i + 1)); // growth preserves the old bytes
  }
  unsigned char *r = (unsigned char *)osp_arc_realloc(q, 8);
  CHECK(r != NULL && size_of(r) == 8u);
  for (size_t i = 0; i < 8; i++) {
    CHECK(r[i] == (unsigned char)(i + 1)); // shrink preserves the prefix
  }
  osp_arc_free(r);
}
static void t_shim_realloc_edges(void) {
  void *fresh = osp_arc_realloc(NULL, 48); // realloc(NULL) == malloc
  CHECK(fresh != NULL && size_of(fresh) == 48u && rc_of(fresh) == 1);
  void *zero = osp_arc_realloc(fresh, 0);
  CHECK(zero != NULL && size_of(zero) == 0u); osp_arc_free(zero);
  void *up = osp_arc_realloc(osp_arc_malloc(0), 16); // oldsize 0: copy skipped
  CHECK(up != NULL && size_of(up) == 16u); osp_arc_free(up);
  // Realloc of a TAGGED object: the copy aliases the children, so ownership
  // MOVES — the new block inherits the layout word and the old one is stripped
  // to RAW before it dies, or the drop walk would decrement `w` twice.
  void *w = witness();
  void **tagged = (void **)mk(16, OSP_MEM_PTR_ARRAY);
  tagged[0] = w; tagged[1] = NULL; osp_retain(w);
  void **grown = (void **)osp_arc_realloc(tagged, 32);
  CHECK(grown && meta_of(grown) == OSP_MEM_PTR_ARRAY && grown[0] == w);
  CHECK(rc_of(w) == 2); // moved, not double-dropped
  osp_arc_free(grown); survived(w); // grown owned it until IT died
  unsigned char *fgn = (unsigned char *)malloc(16);
  CHECK(fgn != NULL); memset(fgn, 0x33, 16);
  unsigned char *g = (unsigned char *)osp_arc_realloc(fgn, 64); // libc owns it
  CHECK(g != NULL);
  for (size_t i = 0; i < 16; i++) { CHECK(g[i] == 0x33); }
  free(g);
}
static void t_shim_strdup(void) {
  char *e = osp_arc_strdup("");
  CHECK(e != NULL && e[0] == '\0' && size_of(e) == 1u && rc_of(e) == 1);
  osp_arc_free(e); char *s = osp_arc_strdup(kRodata);
  CHECK(s != NULL && strcmp(s, kRodata) == 0);
  CHECK(size_of(s) == (uint32_t)(strlen(kRodata) + 1));
  osp_arc_free(s);
  char *big = (char *)malloc(LONG_LEN + 1);
  CHECK(big != NULL);
  memset(big, 'x', LONG_LEN); big[LONG_LEN] = '\0';
  char *d = osp_arc_strdup(big);
  CHECK(d != NULL && strlen(d) == LONG_LEN && memcmp(d, big, LONG_LEN) == 0);
  CHECK(size_of(d) == (uint32_t)(LONG_LEN + 1));
  osp_arc_free(d); free(big);
}
// Deliberate leaks, one per kind, so the OSPREY_ARC_DEBUG=2 atexit report walks
// every histogram bucket and dumps a survivor of each. Contents are inert:
// nothing walks them at exit. The RAW body mixes printable and control bytes so
// both arms of the preview sanitiser run, and the 8-byte bodies land under the
// preview window while the 1 KiB one overruns it.
static void t_leak_report_fixtures(void) {
  void *raw = mk(1024, OSP_MEM_RAW), *msk = mk(W, OSP_MEM_LAYOUT(0));
  void *lp = mk(16, OSP_MEM_LIST_HDR_PTR), *ls = mk(16, OSP_MEM_LIST_HDR_SCALAR);
  void *pa = mk(W, OSP_MEM_PTR_ARRAY);
  memset(raw, 0, 1024); memset(msk, 0, W); memset(pa, 0, W);
  memset(lp, 0, 16); memset(ls, 0, 16);
  for (size_t i = 0; i < PREVIEW_LEN; i++) {
    ((unsigned char *)raw)[i] = (i % 2) ? (unsigned char)'A' : (unsigned char)1;
  }
  void *immortal = mk(W, OSP_MEM_RAW); // excluded from the leak count
  osp_mem_immortal(immortal);
  CHECK(rc_of(immortal) == -1 && rc_of(raw) == 1 && size_of(raw) == 1024u);
}

// An immortal reached AS A CHILD of a dying parent: rc < 0 means unreclaimable
// on every path, not just the top-level release, so the walk must skip it.
static void t_immortal_child(void) {
  void *forever = witness(); osp_mem_immortal(forever);
  void *p = mk(W, OSP_MEM_LAYOUT(OSP_MEM_WORD(0))); put(p, 0, forever);
  osp_release(p); // parent dies, walks word 0 (p is freed: do not read it)
  CHECK(rc_of(forever) == -1); // untouched, live for its other holders
}

int main(void) {
  // Must precede the first allocation: arc_arm_debug() latches once, forever.
  // Level 2 also exercises the per-survivor dump.
  (void)setenv("OSPREY_ARC_DEBUG", "2", 1);
  t_alloc_and_tags(); t_alloc_sizes_oom();
  t_retain_release(); t_release_unique_collect(); t_reclaim_reuse();
  t_foreign_stack_static(); t_foreign_heap_null();
  t_immortal(); t_immortal_child(); t_set_layout();
  t_mask_drop(); t_mask_bounds_and_bits();
  t_ptr_array(); t_ptr_array_ragged();
  t_list_hdr_kinds(); t_list_hdr_edges();
  t_deep_chain(); t_wide_fanout(); t_registry_churn(); t_slot_reuse();
  t_shim_malloc_calloc_free(); t_shim_realloc_grow(); t_shim_realloc_edges();
  t_shim_strdup(); t_leak_report_fixtures();
  printf("[ok] memory_arc: %ld assertions\n", g_checks);
  return 0;
}
