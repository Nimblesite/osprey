// Shared by the translation units of the memory_gc suite: the backend ABI
// under test, the assertion counter and the word helpers, and the threaded
// cases memory_gc_thread_tests.c contributes.
#ifndef OSPREY_GC_TESTS_SHARED_H
#define OSPREY_GC_TESTS_SHARED_H

#include "memory_hooks.h"
#include <assert.h>
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// --- backend ABI under test (memory_gc.c) beyond memory_hooks.h -------------
void *osp_alloc(int64_t size);
void osp_collect(void);
void *osp_gc_malloc(size_t size);
void *osp_gc_calloc(size_t n, size_t size);
void *osp_gc_realloc(void *old, size_t size);
void osp_gc_free(void *p);
size_t osp_gc_live_objects(void);
size_t osp_gc_collections(void);
size_t osp_gc_table_capacity(void);

extern long g_checks;
#define CHECK(c)                                                               \
  do {                                                                         \
    g_checks++;                                                                \
    assert(c);                                                                 \
  } while (0)

static const size_t WORD = sizeof(void *);

// Store / load one pointer-sized word at `slot` (no aliasing UB).
static inline void put(void *body, size_t slot, const void *val) {
  memcpy((char *)body + slot * WORD, &val, sizeof(val));
}
static inline void *load(const void *body, size_t slot) {
  void *v;
  memcpy(&v, (const char *)body + slot * WORD, sizeof(v));
  return v;
}

// Force enough allocation to cross the collection budget, keeping no roots to
// the churn: each result overwrites the last, so only the final block stays
// reachable (returned to the caller). osp_alloc has side effects, so the calls
// are never elided even though the intermediate values are dead.
static inline void *alloc_churn(int rounds, size_t size) {
  void *sink = NULL;
  for (int i = 0; i < rounds; i++) {
    sink = osp_alloc(size);
  }
  return sink;
}

// MEMORY GOLDEN / spike guard: with the budget floored at 1 MiB
// (OSPREY_GC_HEAP_MB=1, set in main before the first allocation), unrooted
// churn past the budget MUST fire collections BY ITSELF — no explicit
// osp_collect — and the tracked set must stay bounded. This is the test that
// fails when memory spikes: if the adaptive budget stops triggering, live
// objects grow with the churn count instead of staying under the ceiling.
// Runs FIRST: it also pins that the very first allocation reads the env knob.

// memory_gc_thread_tests.c
void t_scanned_blocks_are_roots(void);
void t_threads_collect_together(void);
void t_parked_thread_stays_a_root(void);
void t_busy_thread_cannot_block_an_allocator(void);
void t_unregistered_thread_exit_is_a_no_op(void);
void t_multithread_disables_collection(void);
void debug_stats_child(void);
void t_debug_reports(const char *self);

#endif
