// Assertion-driven tests for the sampling CPU profiler [PROF-TEST]
// (docs/specs/0028-Profiler.md). A failed assert aborts the binary.
#include "profiler_runtime.h"

#include <assert.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

bool osp_prof_start(const char *out_path, uint32_t rate_hz);
void osp_prof_stop_and_dump(void);

enum { TEST_RATE_HZ = 2000, FAKE_STACK_WORDS = 64 };

static volatile double g_sink = 0;

// ---- osp_prof_walk unit tests [PROF-COLLECT-UNWIND] --------------------------

// Build a fake AAPCS64-style frame chain inside `mem` and verify the walk
// recovers exactly the planted return addresses, in leaf-first order.
static void test_walk_recovers_planted_chain(void) {
  uint64_t mem[FAKE_STACK_WORDS];
  memset(mem, 0, sizeof(mem));
  uintptr_t lo = (uintptr_t)mem;
  uintptr_t hi = (uintptr_t)(mem + FAKE_STACK_WORDS);
  // Frame records at word 0 -> word 8 -> word 16: [next_fp, return_addr].
  mem[0] = (uint64_t)(uintptr_t)&mem[8];
  mem[1] = 0x100000AAAA;
  mem[8] = (uint64_t)(uintptr_t)&mem[16];
  mem[9] = 0x100000BBBB;
  mem[16] = 0; // chain terminator (fails the bounds check)
  mem[17] = 0x100000CCCC;
  uint64_t out[OSP_PROF_MAX_FRAMES];
  int n = osp_prof_walk(0x100000F000, (uint64_t)lo, 0x100000E000, lo, hi, out,
                        OSP_PROF_MAX_FRAMES);
  assert(n == 5);
  assert(out[0] == 0x100000F000); // precise pc first
  assert(out[1] == 0x100000E000); // lr (differs from first chained ret)
  assert(out[2] == 0x100000AAAA);
  assert(out[3] == 0x100000BBBB);
  assert(out[4] == 0x100000CCCC);
}

// The lr must be deduplicated when it equals the first chained return address.
static void test_walk_dedupes_lr(void) {
  uint64_t mem[FAKE_STACK_WORDS];
  memset(mem, 0, sizeof(mem));
  uintptr_t lo = (uintptr_t)mem;
  uintptr_t hi = (uintptr_t)(mem + FAKE_STACK_WORDS);
  mem[0] = 0;
  mem[1] = 0x100000AAAA;
  uint64_t out[OSP_PROF_MAX_FRAMES];
  int n = osp_prof_walk(0x100000F000, (uint64_t)lo, 0x100000AAAA, lo, hi, out,
                        OSP_PROF_MAX_FRAMES);
  assert(n == 2);
  assert(out[0] == 0x100000F000 && out[1] == 0x100000AAAA);
}

// Out-of-bounds, misaligned, or non-monotonic frame pointers end the walk
// instead of being dereferenced.
static void test_walk_rejects_invalid_fp(void) {
  uint64_t mem[FAKE_STACK_WORDS];
  memset(mem, 0, sizeof(mem));
  uintptr_t lo = (uintptr_t)mem;
  uintptr_t hi = (uintptr_t)(mem + FAKE_STACK_WORDS);
  uint64_t out[OSP_PROF_MAX_FRAMES];
  assert(osp_prof_walk(0x100000F000, (uint64_t)(hi + 64), 0, lo, hi, out,
                       OSP_PROF_MAX_FRAMES) == 1); // fp above hi: pc only
  assert(osp_prof_walk(0x100000F000, (uint64_t)lo + 3, 0, lo, hi, out,
                       OSP_PROF_MAX_FRAMES) == 1); // misaligned fp: pc only
  mem[0] = (uint64_t)lo; // self-referencing fp must not loop forever
  mem[1] = 0x100000AAAA;
  assert(osp_prof_walk(0x100000F000, (uint64_t)lo, 0, lo, hi, out,
                       OSP_PROF_MAX_FRAMES) == 2);
}

// ---- registry & end-to-end [PROF-COLLECT-REGISTRY] [PROF-RAW-FORMAT] ---------

// Hooks must be safe no-ops while the profiler is inactive.
static void test_hooks_inactive_noop(void) {
  assert(!osp_prof_is_active());
  osp_prof_thread_register(1, "fiber");
  osp_prof_thread_unregister();
  OspProfSnap snaps[OSP_PROF_MAX_THREADS];
  assert(osp_prof_snapshot(snaps, OSP_PROF_MAX_THREADS) == 0);
}

// A dropped sample must return the sentinel, never stack index 0.
static void test_record_drop_returns_sentinel(void) {
  assert(osp_prof_record_sample(0, 0, NULL, 0, 0) == OSP_PROF_STACK_NONE);
}

__attribute__((noinline)) static double busy_work(long n) {
  double acc = 0;
  for (long i = 1; i <= n; i++) {
    acc += (double)i / (double)(i + 1);
  }
  return acc;
}

static void *busy_thread(void *arg) {
  (void)arg;
  osp_prof_thread_register(42, "fiber");
  g_sink += busy_work(60000000);
  osp_prof_thread_unregister();
  return NULL;
}

static char *slurp(const char *path) {
  FILE *f = fopen(path, "r");
  assert(f != NULL);
  assert(fseek(f, 0, SEEK_END) == 0);
  long size = ftell(f);
  assert(size > 0);
  rewind(f);
  char *buf = malloc((size_t)size + 1);
  assert(buf != NULL);
  assert(fread(buf, 1, (size_t)size, f) == (size_t)size);
  buf[size] = '\0';
  fclose(f);
  return buf;
}

static void test_end_to_end_capture(void) {
  const char *out = "/tmp/osprey_profiler_test.json";
  unlink(out);
  assert(osp_prof_start(out, TEST_RATE_HZ));
  assert(osp_prof_is_active());
  assert(osp_prof_rate_hz() == TEST_RATE_HZ);
  osp_prof_thread_register(0, "main");
  pthread_t t;
  assert(pthread_create(&t, NULL, busy_thread, NULL) == 0);
  g_sink += busy_work(30000000);
  assert(pthread_join(t, NULL) == 0);
  osp_prof_thread_unregister();
  osp_prof_stop_and_dump();
  assert(!osp_prof_is_active());

  char *json = slurp(out);
  assert(strstr(json, "\"version\":1") != NULL);
  assert(strstr(json, "\"rate_hz\":2000") != NULL);
  assert(strstr(json, "\"label\":\"main\"") != NULL);
  assert(strstr(json, "\"label\":\"fiber\"") != NULL);
  assert(strstr(json, "\"images\":[{") != NULL);
  // Real samples were captured: a non-empty samples array with a stack row.
  assert(strstr(json, "\"samples\":[[") != NULL);
  assert(strstr(json, "\"stacks\":[[") != NULL);
  free(json);
  unlink(out);
}

static void *churn_thread(void *arg) {
  (void)arg;
  osp_prof_thread_register(99, "fiber");
  g_sink += busy_work(20000);
  osp_prof_thread_unregister();
  return NULL;
}

// Registration churn under max-rate sampling: hammers the snapshot-vs-
// unregister window (a slot can be unregistered, joined, and recycled between
// a sampler snapshot and the sample). The generation-validated locking in
// sample_thread must make this safe [PROF-COLLECT-REGISTRY].
static void test_churn_under_max_rate_sampling(void) {
  enum { BATCH = 8, ROUNDS = 40 };
  const char *out = "/tmp/osprey_profiler_churn_test.json";
  unlink(out);
  assert(osp_prof_start(out, 10000));
  for (int round = 0; round < ROUNDS; round++) {
    pthread_t threads[BATCH];
    for (int i = 0; i < BATCH; i++) {
      assert(pthread_create(&threads[i], NULL, churn_thread, NULL) == 0);
    }
    for (int i = 0; i < BATCH; i++) {
      assert(pthread_join(threads[i], NULL) == 0);
    }
  }
  osp_prof_stop_and_dump();
  char *json = slurp(out);
  assert(strstr(json, "\"label\":\"fiber\"") != NULL);
  free(json);
  unlink(out);
}

int main(void) {
  test_walk_recovers_planted_chain();
  test_walk_dedupes_lr();
  test_walk_rejects_invalid_fp();
  test_hooks_inactive_noop();
  test_record_drop_returns_sentinel();
  test_end_to_end_capture();
  test_churn_under_max_rate_sampling();
  printf("profiler_runtime_tests: all tests passed (sink=%f)\n", g_sink);
  return 0;
}
