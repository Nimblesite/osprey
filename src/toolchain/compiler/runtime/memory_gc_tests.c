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
#include "gc_tests_shared.h"

long g_checks = 0;

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

// Pointer layout masks are ARC-only; both tagged forms
// still hand back usable, writable storage.
static void t_tagged_and_noinit(void) {
  void *p = osp_alloc_tagged(24, 0x12345678);
  void *q = osp_alloc_tagged_noinit(24, 0x5);
  CHECK(p && q && p != q);
  memset(p, 1, 24);
  memset(q, 2, 24);
  CHECK(((unsigned char *)p)[23] == 1 && ((unsigned char *)q)[0] == 2);
}

// [PORT-STRING-NUL-OWNERSHIP] Retain the allocation's length across collection and re-stamping.
static void t_owned_text_length(void) {
  char *text = osp_alloc_tagged_noinit(10, OSP_MEM_STRING);
  CHECK(text != NULL);
  memcpy(text, "head\0tail", 10);
  CHECK(osp_mem_string_length(text) == 9);
  CHECK(osp_mem_string_length(NULL) == -1 && osp_mem_string_length("borrowed") == -1);
  osp_collect();
  CHECK(osp_mem_string_length(text) == 9 && memcmp(text, "head\0tail", 10) == 0);
  osp_mem_set_layout(text, OSP_MEM_RAW);
  CHECK(osp_mem_string_length(text) == -1);
  osp_mem_set_layout(text, OSP_MEM_STRING);
  CHECK(osp_mem_string_length(text) == 9);
  CHECK(osp_alloc_tagged(0, OSP_MEM_STRING) == NULL && osp_alloc_tagged(INT64_MAX, OSP_MEM_STRING) == NULL);
  osp_release(text);
  CHECK(osp_mem_string_length(text) == 9); // GC retains a still-reachable value.
}

// Reference-count hooks are no-ops under tracing and never disturb the object.
static void t_refcount_hooks_noop(void) {
  unsigned char *p = osp_alloc(16);
  memset(p, 3, 16);
  osp_retain(p);
  osp_release(p);
  osp_retain(NULL);
  osp_release(NULL);
  osp_release_unique(p); // a proved-unique drop is a no-op too: the collector owns reclamation
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

// [MEM-BACKENDS] osp_mem_immortal pins: a runtime singleton (the empty list)
// whose only reference is a global must survive every collection even where the
// collector cannot scan the data segment (Windows had no data-segment scan, so
// it freed the singleton and every later empty list read reused memory). The
// pointers here are hidden from every scan by XOR, so nothing but the pin can
// keep them; the pinned block's child is reached through the pin as a root.
#define HIDE_MASK ((uintptr_t)0x5A5A5A5A5A5A5A5Aull)
typedef struct {
  uintptr_t pin, child, unpinned;
} HiddenRefs;

// Allocate in a frame of its own and hand back only masked words, so no live
// frame holds a real pointer once it returns.
__attribute__((noinline)) static void make_hidden(volatile HiddenRefs *out) {
  char *child = osp_alloc_tagged(6, OSP_MEM_STRING);
  void **pinned = osp_alloc(sizeof(void *) * 2);
  char *unpinned = osp_alloc_tagged(4, OSP_MEM_STRING);
  CHECK(child != NULL && pinned != NULL && unpinned != NULL);
  memcpy(child, "child", 6);
  pinned[0] = child;
  pinned[1] = NULL;
  osp_mem_immortal(pinned);
  osp_mem_immortal(NULL); // a NULL pin is a harmless no-op
  out->pin = (uintptr_t)pinned ^ HIDE_MASK;
  out->child = (uintptr_t)child ^ HIDE_MASK;
  out->unpinned = (uintptr_t)unpinned ^ HIDE_MASK;
}

// Overwrite the dead frames below the caller so stale pointer copies are gone.
__attribute__((noinline)) static void scrub_stack(void) {
  volatile unsigned char pad[1 << 16];
  for (size_t i = 0; i < sizeof pad; i++) {
    pad[i] = 0;
  }
}

static void t_immortal_pins_without_roots(void) {
  volatile HiddenRefs refs;
  make_hidden(&refs);
  scrub_stack();
  (void)alloc_churn(100000, 48);
  scrub_stack();
  osp_collect();
  osp_collect();
  char *kept = (char *)(refs.child ^ HIDE_MASK);
  CHECK(osp_mem_string_length(kept) == 5 && strcmp(kept, "child") == 0);
  CHECK(((void **)(refs.pin ^ HIDE_MASK))[0] == kept);
  CHECK(osp_mem_string_length((char *)(refs.unpinned ^ HIDE_MASK)) == -1); // unpinned is reclaimed
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
    osp_mem_set_layout(sink, OSP_MEM_RAW); // GC layout stamps must preserve its existing finalizer.
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

// The sweep frees dead blocks and reseats survivors in the table where it
// stands [GC-TRACE-CONSERVATIVE]. A string's length lives in its table entry,
// so a survivor the reseat lost would read -1. Each round keeps a different
// stride of the blocks, leaving survivors between garbage of this round and
// the last, and the budget fires collections of its own mid-round.
#define SWEEP_BLOCKS 60000
#define SWEEP_ROUNDS 6
static char *g_swept[SWEEP_BLOCKS];
static size_t sweep_size(int i) { return 9 + (size_t)(i % 7) * 8; }
static char sweep_byte(int i, int round) { return (char)(1 + (i + round) % 120); }

static void sweep_fill(int round) {
  for (int i = 0; i < SWEEP_BLOCKS; i++) {
    size_t size = sweep_size(i);
    char *text = osp_alloc_tagged_noinit((int64_t)size, OSP_MEM_STRING);
    CHECK(text != NULL);
    memset(text, sweep_byte(i, round), size - 1);
    text[size - 1] = '\0';
    g_swept[i] = i % (round + 2) == 0 ? text : NULL;
  }
}

static size_t sweep_verify(int round) {
  size_t kept = 0;
  for (int i = 0; i < SWEEP_BLOCKS; i += round + 2) {
    const char *text = g_swept[i];
    size_t size = sweep_size(i);
    CHECK(osp_mem_string_length(text) == (int64_t)size - 1);
    CHECK(text[0] == sweep_byte(i, round) && text[size - 2] == sweep_byte(i, round));
    CHECK(text[size - 1] == '\0');
    kept++;
  }
  return kept;
}

static void t_sweep_reseats_survivors(void) {
  for (int round = 0; round < SWEEP_ROUNDS; round++) {
    sweep_fill(round);
    CHECK(sweep_verify(round) > 0); // through the budget's own collections
    osp_collect();
    size_t kept = sweep_verify(round);
    CHECK(osp_gc_live_objects() >= kept && osp_gc_live_objects() < SWEEP_BLOCKS);
    osp_collect(); // a second sweep over an already-swept table moves nothing wrongly
    CHECK(sweep_verify(round) == kept);
  }
  memset(g_swept, 0, sizeof g_swept);
  osp_collect();
  CHECK(osp_gc_live_objects() < 1000 && osp_mem_string_length("borrowed") == -1);
}

// The table keeps the capacity a steady cycle needs: cutting it back to the
// live set at each collection made the next cycle regrow it, rehashing every
// block at each doubling. It is given back once a cycle peaks below it. Each
// cycle allocates under the 1 MiB budget, so only osp_collect collects here.
#define STEADY_BLOCKS 30000
#define STEADY_CYCLES 5
static void t_table_keeps_capacity(void) {
  osp_collect();
  (void)alloc_churn(STEADY_BLOCKS, 16);
  osp_collect(); // warm-up cycle: the table now fits one cycle's blocks
  size_t steady = osp_gc_table_capacity();
  CHECK(steady * 3 > (size_t)STEADY_BLOCKS * 4);
  for (int cycle = 0; cycle < STEADY_CYCLES; cycle++) {
    size_t collections = osp_gc_collections();
    (void)alloc_churn(STEADY_BLOCKS, 16);
    CHECK(osp_gc_collections() == collections && osp_gc_table_capacity() == steady);
    osp_collect();
    CHECK(osp_gc_table_capacity() == steady && osp_gc_live_objects() < 1000);
  }
  osp_collect(); // an idle cycle: nothing was allocated since the last sweep
  CHECK(osp_gc_table_capacity() < steady && osp_gc_live_objects() < 1000);
}

// Volume: many rooted blocks all survive one collection intact — exercises the
// mark and the in-place sweep at scale.
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

int main(int argc, char **argv) {
  if (argc == 2 && strcmp(argv[1], "--finalizer-stamp") == 0) {
    t_finalizer_at_sweep();
    return 0;
  }
  if (argc == 2 && strcmp(argv[1], "--debug-stats") == 0) {
    debug_stats_child();
    return 0;
  }
  CHECK(argc == 1);
  // Floor the collection budget at 1 MiB BEFORE the first allocation so the
  // spike guard exercises the budget with a deterministic knob. gc_init_main
  // reads the env exactly once, on the first allocation.
  CHECK(setenv("OSPREY_GC_HEAP_MB", "1", 1) == 0);
  t_budget_spike_guard(); // FIRST: needs the pristine pre-allocation state
  t_alloc_distinct_writable();
  t_tagged_and_noinit();
  t_owned_text_length();
  t_refcount_hooks_noop();
  t_calloc();
  t_realloc();
  t_free_noop();
  t_reclaims_unrooted();
  t_immortal_pins_without_roots();
  t_root_keeps_alive();
  t_interior_pointer();
  t_sweep_reseats_survivors();
  t_table_keeps_capacity();
  t_many_rooted_survive();
  t_finalizer_at_sweep();
  t_debug_reports(argv[0]);
  t_scanned_blocks_are_roots();
  t_threads_collect_together();
  t_parked_thread_stays_a_root();
  t_busy_thread_cannot_block_an_allocator();
  t_unregistered_thread_exit_is_a_no_op();
  t_multithread_disables_collection(); // LAST: disables collection forever
  printf("[ok] memory_gc: %ld assertions\n", g_checks);
  return 0;
}
