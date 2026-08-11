// Memory GOLDEN tests — exact live-set checkpoints and hard peak ceilings for
// the ARC backend ([GC-ARC-PERCEUS], [MEM-BACKENDS], docs/specs/0018). Linked
// against memory_arc.c + gpu_runtime.c by the Makefile's _test_c_runtime.
//
// Where memory_arc_tests.c proves the MECHANISM (headers, registry, drop
// walks), this suite pins the NUMBERS: after every workload the live
// object/byte counters must equal an exact expected value, and the peak
// (high-water) bytes must never exceed a stated ceiling. A regression that
// leaks, double-retains, or lets a transient spike grow (e.g. a builder that
// stops releasing its scratch, or recycling that stops reusing) fails these
// assertions even when the program's OUTPUT is still correct — the property
// exit-status tests and RSS eyeballing cannot give.
#include "memory_hooks.h"

#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Not declared in memory_hooks.h: the raw allocator, the shim entry points,
// the ARC diagnostics under test, and the GPU buffer ABI (gpu_runtime.c).
void *osp_alloc(int64_t size);
void *osp_arc_malloc(size_t size);
void *osp_arc_realloc(void *old, size_t size);
void osp_arc_free(void *p);
char *osp_arc_strdup(const char *s);
size_t osp_arc_live_objects(void);
size_t osp_arc_live_bytes(void);
size_t osp_arc_peak_bytes(void);
void osp_arc_peak_reset(void);
void *osprey_gpu_alloc(int64_t length);
int64_t osprey_gpu_len(void *buffer);
void osprey_gpu_set(void *buffer, int64_t index, int64_t word);
int64_t osprey_gpu_get(void *buffer, int64_t index);

static long g_checks = 0;
#define CHECK(c)                                                               \
  do {                                                                         \
    g_checks++;                                                                \
    assert(c);                                                                 \
  } while (0)

#define W ((size_t)8)                  // managed word size
#define NODE_META OSP_MEM_LAYOUT(OSP_MEM_WORD(0)) // one managed child at word 0
#define CHURN_ITERS 100000
#define CHURN_BODY ((size_t)48)
#define CHAIN_LEN ((size_t)50000)
#define GPU_LEN ((int64_t)10)
#define GPU_HDR_BYTES ((size_t)16)     // { i64 len, ptr data }
#define REALLOC_OLD ((size_t)32)
#define REALLOC_NEW ((size_t)128)

// The permanent live floor: immortalised singletons never leave the counters.
static size_t g_floor_objs = 0;
static size_t g_floor_bytes = 0;

static void at_floor(void) {
  CHECK(osp_arc_live_objects() == g_floor_objs);
  CHECK(osp_arc_live_bytes() == g_floor_bytes);
}

static void put(void *body, size_t word, const void *val) {
  memcpy((char *)body + word * W, &val, sizeof val);
}

// Arming: OSPREY_ARC_DEBUG is set before ANY allocation, then osp_mem_boot()
// (the codegen main preamble's call) arms the counters — exactly the sequence
// a compiled program runs, so an unarmed counter here is a boot regression.
static void t_boot_arms_exact_zero(void) {
  osp_mem_boot();
  osp_mem_boot(); // idempotent
  CHECK(osp_arc_live_objects() == 0);
  CHECK(osp_arc_live_bytes() == 0);
  CHECK(osp_arc_peak_bytes() == 0);
}

// Every allocation and release moves the counters by EXACTLY the body size —
// requested bytes, never the pool's rounded capacity.
static void t_exact_checkpoints(void) {
  void *a = osp_alloc(64);
  void *b = osp_alloc(32);
  void *c = osp_alloc((int64_t)W);
  CHECK(a && b && c);
  CHECK(osp_arc_live_objects() == g_floor_objs + 3);
  CHECK(osp_arc_live_bytes() == g_floor_bytes + 64 + 32 + W);
  CHECK(osp_arc_peak_bytes() >= g_floor_bytes + 64 + 32 + W);
  osp_release(b);
  CHECK(osp_arc_live_objects() == g_floor_objs + 2);
  CHECK(osp_arc_live_bytes() == g_floor_bytes + 64 + W);
  osp_release(a);
  osp_release(c);
  at_floor();
}

// A failed (oversize) allocation must leave every counter untouched.
static void t_failed_alloc_no_drift(void) {
  size_t objs = osp_arc_live_objects();
  size_t bytes = osp_arc_live_bytes();
  size_t peak = osp_arc_peak_bytes();
  CHECK(osp_alloc_tagged((int64_t)UINT32_MAX + 1, OSP_MEM_RAW) == NULL);
  CHECK(osp_arc_malloc((size_t)-1) == NULL);
  CHECK(osp_arc_live_objects() == objs);
  CHECK(osp_arc_live_bytes() == bytes);
  CHECK(osp_arc_peak_bytes() == peak);
}

// THE spike golden: 100k alloc/free churn of one body must hold the peak at
// EXACTLY one body — a regression in release or in pool reuse shows up as a
// growing live set long before it would OOM anything.
static void t_churn_peak_stays_flat(void) {
  at_floor();
  osp_arc_peak_reset();
  CHECK(osp_arc_peak_bytes() == g_floor_bytes);
  for (size_t i = 0; i < CHURN_ITERS; i++) {
    void *p = osp_alloc((int64_t)CHURN_BODY);
    CHECK(p != NULL);
    osp_release(p);
  }
  CHECK(osp_arc_peak_bytes() == g_floor_bytes + CHURN_BODY);
  at_floor();
}

// Two live bodies at once doubles the ceiling — no more, no less. Proves the
// peak actually tracks overlap rather than allocation count.
static void t_churn_overlap_ceiling(void) {
  osp_arc_peak_reset();
  for (size_t i = 0; i < CHURN_ITERS / 10; i++) {
    void *p = osp_alloc((int64_t)CHURN_BODY);
    void *q = osp_alloc((int64_t)CHURN_BODY);
    CHECK(p && q);
    osp_release(p);
    osp_release(q);
  }
  CHECK(osp_arc_peak_bytes() == g_floor_bytes + 2 * CHURN_BODY);
  at_floor();
}

// Build a 50k-node ownership chain, tear it down with ONE release: the live
// set must peak at exactly CHAIN_LEN nodes and return to the floor. A drop
// walk that strands any subtree leaves the live counter above the floor.
static void t_chain_build_teardown(void) {
  osp_arc_peak_reset();
  void *cur = osp_alloc_tagged((int64_t)W, NODE_META);
  CHECK(cur != NULL);
  put(cur, 0, NULL);
  for (size_t i = 1; i < CHAIN_LEN; i++) {
    void *n = osp_alloc_tagged((int64_t)W, NODE_META);
    CHECK(n != NULL);
    put(n, 0, cur);
    cur = n;
  }
  CHECK(osp_arc_live_objects() == g_floor_objs + CHAIN_LEN);
  CHECK(osp_arc_live_bytes() == g_floor_bytes + CHAIN_LEN * W);
  CHECK(osp_arc_peak_bytes() == g_floor_bytes + CHAIN_LEN * W);
  osp_release(cur);
  at_floor();
  // The peak REMAINS at the high-water mark after teardown: that is the point.
  CHECK(osp_arc_peak_bytes() == g_floor_bytes + CHAIN_LEN * W);
}

// realloc's transient double-residency is bounded: old + new live at once,
// then exactly the new body remains.
static void t_realloc_transient_bounded(void) {
  osp_arc_peak_reset();
  void *p = osp_arc_malloc(REALLOC_OLD);
  CHECK(p != NULL);
  void *q = osp_arc_realloc(p, REALLOC_NEW);
  CHECK(q != NULL);
  CHECK(osp_arc_peak_bytes() == g_floor_bytes + REALLOC_OLD + REALLOC_NEW);
  CHECK(osp_arc_live_objects() == g_floor_objs + 1);
  CHECK(osp_arc_live_bytes() == g_floor_bytes + REALLOC_NEW);
  osp_arc_free(q);
  at_floor();
}

// Runtime-minted strings account byte-for-byte (strlen + NUL).
static void t_strdup_exact(void) {
  char *s = osp_arc_strdup("golden");
  CHECK(s != NULL);
  CHECK(osp_arc_live_objects() == g_floor_objs + 1);
  CHECK(osp_arc_live_bytes() == g_floor_bytes + strlen("golden") + 1);
  osp_arc_free(s);
  at_floor();
}

// A GPU buffer is exactly two managed objects — { len, data } header plus the
// word array — and ONE release reclaims both via the LIST_HDR_SCALAR layout.
// Pins the gpu_runtime.c/[GPU-BUFFER] contract that device staging buffers
// need no GPU-specific drop code under ARC.
static void t_gpu_buffer_accounting(void) {
  osp_arc_peak_reset();
  void *buf = osprey_gpu_alloc(GPU_LEN);
  CHECK(buf != NULL && osprey_gpu_len(buf) == GPU_LEN);
  size_t data_bytes = (size_t)GPU_LEN * W;
  CHECK(osp_arc_live_objects() == g_floor_objs + 2);
  CHECK(osp_arc_live_bytes() == g_floor_bytes + GPU_HDR_BYTES + data_bytes);
  osprey_gpu_set(buf, 0, 41);
  CHECK(osprey_gpu_get(buf, 0) == 41);
  osp_release(buf);
  at_floor();
  // Empty and clamped-negative buffers still cost header + one guard word.
  void *empty = osprey_gpu_alloc(0);
  void *neg = osprey_gpu_alloc(-7);
  CHECK(empty && neg);
  CHECK(osprey_gpu_len(empty) == 0 && osprey_gpu_len(neg) == 0);
  CHECK(osp_arc_live_objects() == g_floor_objs + 4);
  CHECK(osp_arc_live_bytes() == g_floor_bytes + 2 * (GPU_HDR_BYTES + W));
  osp_release(empty);
  osp_release(neg);
  at_floor();
}

// GPU churn holds its ceiling exactly like scalar churn: map-style pipelines
// (alloc, fill, release per stage) must not accrete buffers.
static void t_gpu_churn_flat(void) {
  osp_arc_peak_reset();
  size_t one = GPU_HDR_BYTES + (size_t)GPU_LEN * W;
  for (size_t i = 0; i < CHURN_ITERS / 100; i++) {
    void *buf = osprey_gpu_alloc(GPU_LEN);
    CHECK(buf != NULL);
    osprey_gpu_set(buf, (int64_t)(i % (size_t)GPU_LEN), (int64_t)i);
    osp_release(buf);
  }
  CHECK(osp_arc_peak_bytes() == g_floor_bytes + one);
  at_floor();
}

// Immortal singletons are a PERMANENT, EXACT floor — they never decay, and
// later workloads sit on top of the floor without disturbing it. Runs last so
// every earlier test could assert the zero floor.
static void t_immortal_floor(void) {
  void *singleton = osp_alloc((int64_t)W);
  CHECK(singleton != NULL);
  osp_mem_immortal(singleton);
  g_floor_objs += 1;
  g_floor_bytes += W;
  at_floor();
  for (int i = 0; i < 1000; i++) {
    osp_retain(singleton);
    osp_release(singleton);
  }
  at_floor(); // dup/drop on an immortal moves nothing
  void *p = osp_alloc(64);
  CHECK(osp_arc_live_bytes() == g_floor_bytes + 64);
  osp_release(p);
  at_floor();
}

int main(void) {
  // Precede the first allocation, exactly like a compiled program's preamble.
  (void)setenv("OSPREY_ARC_DEBUG", "1", 1);
  t_boot_arms_exact_zero();
  t_exact_checkpoints();
  t_failed_alloc_no_drift();
  t_churn_peak_stays_flat();
  t_churn_overlap_ceiling();
  t_chain_build_teardown();
  t_realloc_transient_bounded();
  t_strdup_exact();
  t_gpu_buffer_accounting();
  t_gpu_churn_flat();
  t_immortal_floor();
  printf("[ok] memory_golden: %ld assertions\n", g_checks);
  return 0;
}
