// [MEM-BACKENDS] [GC-TRACE-CONSERVATIVE] conformance for memory_gc.c.
//
// Mirrors memory_arc_tests.c: this program is LINKED directly against
// memory_gc.c (see the Makefile _test_c_runtime recipe) and drives the backend
// through its public C ABI plus the two read-only diagnostics
// (osp_gc_live_objects / osp_gc_collections). A conservative, non-moving
// collector exposes no per-object header, so the assertions are behavioural:
// reachable objects survive a collection byte-for-byte, unreachable churn is
// reclaimed (the tracked set stays bounded), interior pointers are followed,
// the value-container shim allocators (malloc/calloc/realloc/free) behave, and a
// second allocating thread latches collection off forever.
//
// Built under the hardened unit-test warning core (Makefile WARN): no shadowing,
// no VLAs, prototypes, const-correct.

#include "memory_hooks.h"
#include "memory_parked_wait.h"
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

static long g_checks = 0;
#define CHECK(c)                                                               \
  do {                                                                         \
    g_checks++;                                                                \
    assert(c);                                                                 \
  } while (0)

static const size_t WORD = sizeof(void *);

// Store / load one pointer-sized word at `slot` (no aliasing UB).
static void put(void *body, size_t slot, const void *val) {
  memcpy((char *)body + slot * WORD, &val, sizeof(val));
}
static void *load(const void *body, size_t slot) {
  void *v;
  memcpy(&v, (const char *)body + slot * WORD, sizeof(v));
  return v;
}

// Force enough allocation to cross the collection budget, keeping no roots to
// the churn: each result overwrites the last, so only the final block stays
// reachable (returned to the caller). osp_alloc has side effects, so the calls
// are never elided even though the intermediate values are dead.
static void *alloc_churn(int rounds, size_t size) {
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
#define GOLDEN_CHURN_ROUNDS 40000
#define GOLDEN_CHURN_SIZE ((size_t)64) /* 40000 * 64 = ~2.4x the 1 MiB budget */
#define GOLDEN_LIVE_CEILING ((size_t)(GOLDEN_CHURN_ROUNDS / 2))
static void t_budget_spike_guard(void) {
  osp_mem_boot(); // link anchor: callable before/after any allocation
  CHECK(osp_gc_collections() == 0); // nothing may have collected pre-alloc
  void *last = alloc_churn(GOLDEN_CHURN_ROUNDS, GOLDEN_CHURN_SIZE);
  CHECK(last != NULL);
  size_t fired = osp_gc_collections();
  CHECK(fired >= 1); // the budget alone forced at least one collection
  CHECK(osp_gc_live_objects() <= GOLDEN_LIVE_CEILING);
  // A second identical round must NOT stack on top of the first: the live set
  // stays under the same ceiling, proving reclamation keeps pace with churn.
  (void)alloc_churn(GOLDEN_CHURN_ROUNDS, GOLDEN_CHURN_SIZE);
  CHECK(osp_gc_collections() > fired);
  CHECK(osp_gc_live_objects() <= GOLDEN_LIVE_CEILING);
  osp_mem_boot(); // still a harmless no-op with a live heap
}

// A fresh block is non-NULL, distinct, and fully writable across its length.
static void t_alloc_distinct_writable(void) {
  void *a = osp_alloc(32);
  void *b = osp_alloc(32);
  void *c = osp_alloc(64);
  CHECK(a && b && c);
  CHECK(a != b && b != c && a != c);
  memset(a, 0xAB, 32);
  memset(b, 0xCD, 32);
  memset(c, 0xEF, 64);
  CHECK(((unsigned char *)a)[0] == 0xAB && ((unsigned char *)a)[31] == 0xAB);
  CHECK(((unsigned char *)b)[0] == 0xCD && ((unsigned char *)b)[31] == 0xCD);
  CHECK(((unsigned char *)c)[0] == 0xEF && ((unsigned char *)c)[63] == 0xEF);
}

// The layout word is ARC-only; the tracing GC ignores it and both tagged forms
// still hand back usable, writable storage.
static void t_tagged_and_noinit(void) {
  void *p = osp_alloc_tagged(24, 0x12345678);
  void *q = osp_alloc_tagged_noinit(24, 0x5);
  CHECK(p && q && p != q);
  memset(p, 1, 24);
  memset(q, 2, 24);
  CHECK(((unsigned char *)p)[23] == 1 && ((unsigned char *)q)[0] == 2);
}

// Reference-count hooks are no-ops under tracing and never disturb the object.
static void t_refcount_hooks_noop(void) {
  unsigned char *p = osp_alloc(16);
  memset(p, 3, 16);
  osp_retain(p);
  osp_release(p);
  osp_retain(NULL);
  osp_release(NULL);
  for (int i = 0; i < 16; i++) {
    CHECK(p[i] == 3);
  }
}

// calloc zeroes the whole request and rejects a multiply that would overflow.
static void t_calloc(void) {
  unsigned char *z = osp_gc_calloc(8, 16);
  CHECK(z);
  for (int i = 0; i < 128; i++) {
    CHECK(z[i] == 0);
  }
  CHECK(osp_gc_calloc((size_t)-1, 2) == NULL); // n*size overflow guard
}

// realloc preserves the min(old,new) prefix; a NULL `old` behaves as malloc.
static void t_realloc(void) {
  unsigned char *p = osp_gc_malloc(16);
  for (int i = 0; i < 16; i++) {
    p[i] = (unsigned char)(i + 1);
  }
  unsigned char *q = osp_gc_realloc(p, 64);
  CHECK(q);
  for (int i = 0; i < 16; i++) {
    CHECK(q[i] == (unsigned char)(i + 1)); // prefix carried across the grow
  }
  void *r = osp_gc_realloc(NULL, 8);
  CHECK(r);
}

// free is a no-op: the collector, not libc free(), reclaims, so the block stays
// valid and readable after the call.
static void t_free_noop(void) {
  unsigned char *p = osp_gc_malloc(16);
  p[0] = 7;
  p[15] = 9;
  osp_gc_free(p);
  CHECK(p[0] == 7 && p[15] == 9);
}

// Unreachable churn is reclaimed: after collecting, the tracked set is a tiny
// fraction of everything ever allocated (not a monotone pile), and a collection
// demonstrably ran.
static void t_reclaims_unrooted(void) {
  size_t before = osp_gc_collections();
  void *last = alloc_churn(400000, 32);
  CHECK(last != NULL);
  osp_collect();
  CHECK(osp_gc_collections() > before);   // at least one collection fired
  CHECK(osp_gc_live_objects() < 1000);    // 400k unrooted blocks did not survive
}

// A stack-rooted block survives arbitrary collection pressure byte-for-byte.
static void t_root_keeps_alive(void) {
  unsigned char *keep = osp_alloc(48);
  memset(keep, 0x5A, 48);
  (void)alloc_churn(300000, 32);
  osp_collect();
  for (int i = 0; i < 48; i++) {
    CHECK(keep[i] == 0x5A); // reachable via `keep` -> never reclaimed
  }
}

// An OSP_MEM_LIST_HDR_EXTERN block runs the registered finalizer at the sweep
// that reclaims it, over its still-intact words; a rooted one is left alone,
// and with nothing registered nothing runs.
static unsigned g_finalized;
static void note_finalized(void *body) {
  CHECK(load(body, 1) == body); // the words are intact when the finalizer runs
  g_finalized++;
}
static void *extern_churn(int rounds) {
  void *sink = NULL;
  for (int i = 0; i < rounds; i++) {
    sink = osp_alloc_tagged(24, OSP_MEM_LIST_HDR_EXTERN);
    put(sink, 1, sink);
  }
  return sink;
}
static void t_finalizer_at_sweep(void) {
  osp_mem_set_finalizer(note_finalized);
  void *keep = osp_alloc_tagged(24, OSP_MEM_LIST_HDR_EXTERN);
  put(keep, 1, keep);
  (void)extern_churn(100000);
  osp_collect();
  CHECK(g_finalized > 99000);   // the unrooted blocks were finalized as they were swept
  CHECK(load(keep, 1) == keep); // the rooted one was not
  unsigned seen = g_finalized;
  osp_mem_set_finalizer(NULL);
  (void)extern_churn(1000);
  osp_collect();
  CHECK(g_finalized == seen);
}

// Interior pointers are followed: a child reachable ONLY through a rooted
// parent's word survives, and the parent still names it after collection.
static void t_interior_pointer(void) {
  void *parent = osp_alloc(2 * WORD);
  unsigned char *child = osp_alloc(32);
  memset(child, 0x77, 32);
  put(parent, 0, child);
  put(parent, 1, NULL);
  child = NULL; // drop the direct root; only parent -> child remains
  (void)alloc_churn(300000, 32);
  osp_collect();
  unsigned char *reached = load(parent, 0);
  CHECK(reached != NULL);
  for (int i = 0; i < 32; i++) {
    CHECK(reached[i] == 0x77); // marked through the interior pointer
  }
}

// Volume: many rooted blocks all survive one collection intact — exercises the
// mark/sweep table rebuild at scale and contributes the bulk of the assertions.
static void t_many_rooted_survive(void) {
  enum { N = 20000 };
  static void *ptrs[N];
  for (int i = 0; i < N; i++) {
    ptrs[i] = osp_alloc(24);
    CHECK(ptrs[i] != NULL);
    memset(ptrs[i], i & 0xFF, 24);
  }
  osp_collect();
  for (int i = 0; i < N; i++) {
    const unsigned char *p = ptrs[i];
    unsigned char want = (unsigned char)(i & 0xFF);
    CHECK(p[0] == want && p[11] == want && p[23] == want);
  }
}

static void *mt_alloc(void *arg) {
  for (int i = 0; i < 2000; i++) {
    void *p = osp_alloc(32);
    if (p) {
      memset(p, 0x11, 32);
    }
  }
  return arg;
}

// --- [MEM-GC-SAFEPOINT] collecting while other threads run -------------------
#define THREAD_CHURN_ROUNDS 200000
#define THREAD_CHURN_SIZE ((size_t)64)
#define THREAD_COUNT 4
#define THREAD_LIVE_CEILING ((size_t)(THREAD_CHURN_ROUNDS / 2))
#define MARK_BYTE 0xAB
#define MARK_SIZE ((size_t)64)

static void *marked_object(void) {
  void *object = osp_alloc((int64_t)MARK_SIZE);
  CHECK(object != NULL);
  memset(object, MARK_BYTE, MARK_SIZE);
  return object;
}

static void check_marked(const void *object) {
  const unsigned char *bytes = (const unsigned char *)object;
  for (size_t i = 0; i < MARK_SIZE; i++) {
    CHECK(bytes[i] == MARK_BYTE);
  }
}

// A scanned block is a root, keeps its bytes when it grows, and stops being a
// root when it is freed. Blocks are unlinked from the head, middle and tail.
static void t_scanned_blocks_are_roots(void) {
  void *first = osp_mem_scanned_alloc(2 * WORD);
  void *middle = osp_mem_scanned_alloc(2 * WORD);
  void *last = osp_mem_scanned_alloc(2 * WORD);
  CHECK(first && middle && last);
  put(middle, 1, marked_object());
  (void)alloc_churn(1000, 32);
  osp_collect();
  check_marked(load(middle, 1));
  void *grown = osp_mem_scanned_realloc(middle, 8 * WORD);
  CHECK(grown != NULL && load(grown, 0) == NULL);
  check_marked(load(grown, 1));
  CHECK(load(grown, 7) == NULL);
  void *fresh = osp_mem_scanned_realloc(NULL, WORD);
  CHECK(fresh != NULL && load(fresh, 0) == NULL);
  osp_mem_scanned_free(NULL);
  osp_mem_scanned_free(fresh);
  osp_mem_scanned_free(first);
  osp_mem_scanned_free(last);
  osp_mem_scanned_free(grown);
  osp_collect();
}

static void *churning_thread(void *arg) {
  osp_mem_baton_thread_enter();
  void *volatile kept = marked_object();
  (void)alloc_churn(THREAD_CHURN_ROUNDS, THREAD_CHURN_SIZE);
  check_marked(kept);
  osp_mem_baton_thread_exit();
  return arg;
}

// Its frame sits below the caller's, so the caller's locals stay in what is scanned.
__attribute__((noinline)) static void parked_join(pthread_t *threads, int count) {
  char lowest = 0;
  osp_mem_baton_park(&lowest);
  for (int i = 0; i < count; i++) {
    CHECK(pthread_join(threads[i], NULL) == 0);
  }
  osp_mem_baton_unpark();
}

// Threads that all allocate stop for each other: garbage is reclaimed, and what
// each one and the parked main thread still hold survives.
static void t_threads_collect_together(void) {
  osp_mem_notify_multithreaded();
  osp_mem_baton_prepare();
  void *volatile mine = marked_object();
  size_t collections = osp_gc_collections();
  pthread_t threads[THREAD_COUNT];
  for (int i = 0; i < THREAD_COUNT; i++) {
    CHECK(pthread_create(&threads[i], NULL, churning_thread, NULL) == 0);
  }
  parked_join(threads, THREAD_COUNT);
  CHECK(osp_gc_collections() > collections);
  check_marked(mine);
  osp_collect();
  CHECK(osp_gc_live_objects() < THREAD_LIVE_CEILING);
  check_marked(mine);
}

static pthread_mutex_t g_park_lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t g_park_cond = PTHREAD_COND_INITIALIZER;
static int g_park_ready = 0;
static int g_park_released = 0;

static void *parked_thread(void *arg) {
  osp_mem_baton_thread_enter();
  void *volatile kept = marked_object();
  pthread_mutex_lock(&g_park_lock);
  g_park_ready = 1;
  pthread_cond_broadcast(&g_park_cond);
  while (!g_park_released) {
    (void)osp_parked_wait(&g_park_cond, &g_park_lock, NULL);
  }
  pthread_mutex_unlock(&g_park_lock);
  check_marked(kept);
  osp_mem_baton_thread_exit();
  return arg;
}

// A thread blocked in a parked wait holds nothing up, and its stack stays a root.
static void t_parked_thread_stays_a_root(void) {
  pthread_t thread;
  CHECK(pthread_create(&thread, NULL, parked_thread, NULL) == 0);
  pthread_mutex_lock(&g_park_lock);
  while (!g_park_ready) {
    pthread_cond_wait(&g_park_cond, &g_park_lock);
  }
  pthread_mutex_unlock(&g_park_lock);
  size_t collections = osp_gc_collections();
  osp_collect();
  (void)alloc_churn(THREAD_CHURN_ROUNDS, THREAD_CHURN_SIZE);
  CHECK(osp_gc_collections() > collections);
  pthread_mutex_lock(&g_park_lock);
  g_park_released = 1;
  pthread_cond_broadcast(&g_park_cond);
  pthread_mutex_unlock(&g_park_lock);
  CHECK(pthread_join(thread, NULL) == 0);
}

static _Atomic int g_busy_done = 0;

static void *allocating_past_a_busy_main(void *arg) {
  osp_mem_baton_thread_enter();
  (void)alloc_churn(THREAD_CHURN_ROUNDS, THREAD_CHURN_SIZE);
  osp_mem_baton_thread_exit();
  atomic_store(&g_busy_done, 1);
  return arg;
}

// Main runs without allocating or parking, so it can never reach a safepoint. The
// other thread must finish regardless, without collecting under main, and
// collecting must resume once main allocates.
static void t_busy_thread_cannot_block_an_allocator(void) {
  size_t collections = osp_gc_collections();
  pthread_t thread;
  CHECK(pthread_create(&thread, NULL, allocating_past_a_busy_main, NULL) == 0);
  while (!atomic_load(&g_busy_done)) {
    sched_yield();
  }
  CHECK(osp_gc_collections() == collections);
  CHECK(pthread_join(thread, NULL) == 0);
  (void)alloc_churn(THREAD_CHURN_ROUNDS, THREAD_CHURN_SIZE);
  CHECK(osp_gc_collections() > collections);
}

// A thread that never registered has nothing to leave.
static void t_unregistered_thread_exit_is_a_no_op(void) {
  size_t collections = osp_gc_collections();
  osp_mem_baton_thread_exit();
  osp_collect();
  CHECK(osp_gc_collections() == collections + 1);
}

// Once a SECOND thread allocates, collection latches off for the whole process
// (the collector can only scan the sole registered stack safely). Must run LAST
// — it permanently disables reclamation.
static void t_multithread_disables_collection(void) {
  osp_mem_notify_multithreaded();
  pthread_t th;
  CHECK(pthread_create(&th, NULL, mt_alloc, NULL) == 0);
  CHECK(pthread_join(th, NULL) == 0);
  size_t frozen = osp_gc_collections();
  (void)alloc_churn(400000, 32); // would normally cross the budget many times
  CHECK(osp_gc_collections() == frozen); // but collection never fires again
}

int main(void) {
  // Floor the collection budget at 1 MiB BEFORE the first allocation so the
  // spike guard exercises the budget with a deterministic knob. gc_init_main
  // reads the env exactly once, on the first allocation.
  CHECK(setenv("OSPREY_GC_HEAP_MB", "1", 1) == 0);
  t_budget_spike_guard(); // FIRST: needs the pristine pre-allocation state
  t_alloc_distinct_writable();
  t_tagged_and_noinit();
  t_refcount_hooks_noop();
  t_calloc();
  t_realloc();
  t_free_noop();
  t_reclaims_unrooted();
  t_root_keeps_alive();
  t_interior_pointer();
  t_many_rooted_survive();
  t_finalizer_at_sweep();
  t_scanned_blocks_are_roots();
  t_threads_collect_together();
  t_parked_thread_stays_a_root();
  t_busy_thread_cannot_block_an_allocator();
  t_unregistered_thread_exit_is_a_no_op();
  t_multithread_disables_collection(); // LAST: disables collection forever
  printf("[ok] memory_gc: %ld assertions\n", g_checks);
  return 0;
}
