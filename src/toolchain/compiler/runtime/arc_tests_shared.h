// Shared by the translation units of the memory_arc suite. memory_arc.c is
// compiled once, into memory_arc_tests.c, behind its refusable allocators; the
// layout and shim halves reach it only through the public hooks and read the
// {int64 meta; int32 rc; uint32 size} header at body-16 with the helpers here.
#ifndef OSPREY_ARC_TESTS_SHARED_H
#define OSPREY_ARC_TESTS_SHARED_H

#include "memory_hooks.h"

#include <assert.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// A literal in read-only data, and the block the last finalizer saw.
extern const char kRodata[];
extern void *g_finalized;

// memory_arc.c's own entry points (it is compiled into memory_arc_tests.c).
void *osp_alloc(int64_t size);
void osp_collect(void);
size_t osp_arc_live_objects(void);
size_t osp_arc_live_bytes(void);
size_t osp_arc_peak_bytes(void);
void osp_arc_peak_reset(void);
void *osp_arc_malloc(size_t size);
void *osp_arc_calloc(size_t n, size_t size);
void osp_arc_free(void *p);
void *osp_arc_realloc(void *old, size_t size);
char *osp_arc_strdup(const char *s);

extern _Atomic long g_checks; // atomic: the mt_churn threads CHECK too
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
#define DIAG_BYTES ((size_t)64) // one diagnostics-test object, a whole number of words

static inline int32_t rc_of(const void *b) {
  int32_t v; memcpy(&v, (const char *)b - HDR_RC_OFF, sizeof v); return v;
}
static inline uint32_t size_of(const void *b) {
  uint32_t v; memcpy(&v, (const char *)b - HDR_SIZE_OFF, sizeof v); return v;
}
static inline int64_t meta_of(const void *b) {
  int64_t v; memcpy(&v, (const char *)b - HDR_META_OFF, sizeof v); return v;
}
static inline void *load(const void *body, size_t word) {
  void *v; memcpy(&v, (const char *)body + word * W, sizeof v); return v;
}
static inline void put(void *body, size_t word, const void *val) {
  memcpy((char *)body + word * W, &val, sizeof val);
}
static inline void put_i64(void *body, size_t word, int64_t v) {
  memcpy((char *)body + word * W, &v, sizeof v);
}
static inline void *mk(size_t bytes, int64_t meta) {
  void *p = osp_alloc_tagged((int64_t)bytes, meta);
  CHECK(p != NULL); CHECK(rc_of(p) == 1);
  CHECK(size_of(p) == (uint32_t)bytes); CHECK(meta_of(p) == meta);
  return p;
}
static inline void *witness(void) { return mk(W, OSP_MEM_RAW); }
// MASK parent holding one reference to `child` at word 0; `dup` picks between
// taking a fresh reference and adopting the caller's.
static inline void *owner(void *child, int dup) {
  void *p = mk(W, OSP_MEM_LAYOUT(OSP_MEM_WORD(0)));
  put(p, 0, child);
  if (dup) { osp_retain(child); }
  return p;
}
// The witness outlived its owner's drop with exactly one reference left.
static inline void survived(void *w) { CHECK(rc_of(w) == 1); osp_release(w); }
static inline uint64_t rnd(uint64_t *s) {
  *s = *s * 6364136223846793005ull + 1442695040888963407ull;
  return *s >> 33;
}

// memory_arc_layout_tests.c
void t_alloc_and_tags(void);
void t_retain_release(void);
void t_release_unique_collect(void);
void t_reclaim_reuse(void);
void t_foreign_stack_static(void);
void t_foreign_heap_null(void);
void t_immortal(void);
void t_set_layout(void);
void t_mask_drop(void);
void t_mask_bounds_and_bits(void);
void t_ptr_array(void);
void t_ptr_array_ragged(void);
void t_list_hdr_kinds(void);
void t_list_hdr_edges(void);
void t_list_hdr_extern(void);
void t_deep_chain(void);
void t_alloc_noinit(void);
void t_mask_direct(void);
void t_wide_fanout(void);

// memory_arc_shim_tests.c
void t_registry_churn(void);
void t_slot_reuse(void);
void t_shim_malloc_calloc_free(void);
void t_shim_realloc_grow(void);
void t_shim_realloc_edges(void);
void t_shim_strdup(void);
void t_diagnostic_counters(void);
void t_multithreaded(void);
void t_leak_report_fixtures(void);
void t_immortal_child(void);
void t_owned_text_lifetime(void);

#endif
