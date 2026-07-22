// [MEM-BACKENDS] conformance for the tracing GC backend (memory_gc.c).
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

#include <assert.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// --- backend ABI under test (memory_gc.c) ------------------------------------
void *osp_alloc(int64_t size);
void *osp_alloc_tagged(int64_t size, int64_t meta);
void *osp_alloc_tagged_noinit(int64_t size, int64_t meta);
void osp_collect(void);
void osp_retain(void *o);
void osp_release(void *o);
void osp_mem_notify_multithreaded(void);
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
  t_multithread_disables_collection(); // LAST: disables collection forever
  printf("[ok] memory_gc: %ld assertions\n", g_checks);
  return 0;
}
