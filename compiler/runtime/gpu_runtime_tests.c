// Assertion-driven tests for gpu_runtime.c — the [GPU-BACKEND-HOST] staging
// buffer behind toGpu/fromGpu/gpuMap/gpuFold ([GPU-BUFFER], [GPU-BUFFER-ELEM],
// [GPU-BUFFER-LENGTH], [GPU-GET], [GPU-FILTER], docs/specs/0034). Linked with
// memory_runtime.c (the default backend) by the Makefile's _test_c_runtime.
//
// The load-bearing contract is TOTALITY: a negative length, an overflowing
// length, and a failed data allocation must all yield an observably EMPTY
// buffer — never a trap, never a partial buffer, never an out-of-bounds
// write. Every accessor must be NULL-safe and bounds-checked, because these
// are the no-trap backstops behind codegen's counted loops.
#include <assert.h>
#include <stdint.h>
#include <stdio.h>

void *osprey_gpu_alloc(int64_t length);
int64_t osprey_gpu_len(void *buffer);
int64_t osprey_gpu_get(void *buffer, int64_t index);
void osprey_gpu_set(void *buffer, int64_t index, int64_t word);
int32_t osprey_gpu_in_bounds(void *buffer, int64_t index);
void osprey_gpu_take(void *buffer, int64_t count);

static long g_checks = 0;
#define CHECK(c)                                                               \
  do {                                                                         \
    g_checks++;                                                                \
    assert(c);                                                                 \
  } while (0)

// Elements are 8-byte words; this mirrors gpu_runtime.c's GPU_MAX_LEN.
#define GPU_MAX_LEN (INT64_MAX / 8)
#define ROUND_LEN ((int64_t)4096)
#define SMALL_LEN ((int64_t)16)

// A fresh buffer reports its exact length and reads zero at every index.
static void t_alloc_zero_filled(void) {
  void *b = osprey_gpu_alloc(SMALL_LEN);
  CHECK(b != NULL);
  CHECK(osprey_gpu_len(b) == SMALL_LEN);
  for (int64_t i = 0; i < SMALL_LEN; i++) {
    CHECK(osprey_gpu_get(b, i) == 0);
    CHECK(osprey_gpu_in_bounds(b, i) == 1);
  }
  CHECK(osprey_gpu_in_bounds(b, SMALL_LEN) == 0);
  CHECK(osprey_gpu_in_bounds(b, -1) == 0);
}

// Out-of-range lengths clamp to the EMPTY buffer — allocation is total.
static void t_alloc_clamps_to_empty(void) {
  int64_t bad[] = {-1, INT64_MIN, GPU_MAX_LEN + 1, INT64_MAX};
  for (unsigned i = 0; i < sizeof(bad) / sizeof(bad[0]); i++) {
    void *b = osprey_gpu_alloc(bad[i]);
    CHECK(b != NULL);
    CHECK(osprey_gpu_len(b) == 0);
    CHECK(osprey_gpu_in_bounds(b, 0) == 0);
    CHECK(osprey_gpu_get(b, 0) == 0);
    osprey_gpu_set(b, 0, 99); // ignored, not a wild store
    CHECK(osprey_gpu_get(b, 0) == 0);
  }
  void *zero = osprey_gpu_alloc(0);
  CHECK(zero != NULL && osprey_gpu_len(zero) == 0);
}

// An IN-range length whose data allocation fails (no machine has 2^63-aligned
// bytes) still yields the empty buffer, never NULL-deref or a partial length.
static void t_alloc_failure_is_empty(void) {
  void *b = osprey_gpu_alloc(GPU_MAX_LEN);
  CHECK(b != NULL);
  CHECK(osprey_gpu_len(b) == 0);
  CHECK(osprey_gpu_in_bounds(b, 0) == 0);
  osprey_gpu_set(b, 0, 7);
  CHECK(osprey_gpu_get(b, 0) == 0);
}

// Every accessor tolerates NULL — the empty-buffer degenerate case.
static void t_null_safety(void) {
  CHECK(osprey_gpu_len(NULL) == 0);
  CHECK(osprey_gpu_get(NULL, 0) == 0);
  CHECK(osprey_gpu_in_bounds(NULL, 0) == 0);
  osprey_gpu_set(NULL, 0, 1);
  osprey_gpu_take(NULL, 3);
  CHECK(osprey_gpu_len(NULL) == 0);
}

// Full write/read round trip, and out-of-bounds stores land NOWHERE: after
// poking every invalid index the whole payload is still byte-identical.
static void t_set_get_roundtrip(void) {
  void *b = osprey_gpu_alloc(ROUND_LEN);
  CHECK(b != NULL && osprey_gpu_len(b) == ROUND_LEN);
  for (int64_t i = 0; i < ROUND_LEN; i++) {
    osprey_gpu_set(b, i, i * i - 3);
  }
  osprey_gpu_set(b, ROUND_LEN, 0x7A7A);     // one past the end
  osprey_gpu_set(b, -1, 0x7A7A);            // below zero
  osprey_gpu_set(b, INT64_MAX, 0x7A7A);     // absurdly far
  osprey_gpu_set(b, INT64_MIN, 0x7A7A);
  for (int64_t i = 0; i < ROUND_LEN; i++) {
    CHECK(osprey_gpu_get(b, i) == i * i - 3);
  }
}

// Extreme word values survive storage exactly (raw 8-byte words, no coercion).
static void t_boundary_words(void) {
  void *b = osprey_gpu_alloc(2);
  CHECK(b != NULL);
  osprey_gpu_set(b, 0, INT64_MIN);
  osprey_gpu_set(b, 1, INT64_MAX);
  CHECK(osprey_gpu_get(b, 0) == INT64_MIN);
  CHECK(osprey_gpu_get(b, 1) == INT64_MAX);
}

// take publishes a compaction prefix: it only ever SHRINKS, the surviving
// prefix is untouched, and the amputated tail becomes unaddressable.
static void t_take_shrinks_only(void) {
  void *b = osprey_gpu_alloc(10);
  CHECK(b != NULL);
  for (int64_t i = 0; i < 10; i++) {
    osprey_gpu_set(b, i, 100 + i);
  }
  osprey_gpu_take(b, 20); // larger than length: no growth
  CHECK(osprey_gpu_len(b) == 10);
  osprey_gpu_take(b, 7);
  CHECK(osprey_gpu_len(b) == 7);
  for (int64_t i = 0; i < 7; i++) {
    CHECK(osprey_gpu_get(b, i) == 100 + i);
  }
  CHECK(osprey_gpu_in_bounds(b, 7) == 0);
  CHECK(osprey_gpu_get(b, 7) == 0);
  osprey_gpu_set(b, 7, 55); // past the published length: ignored
  osprey_gpu_take(b, 7);    // idempotent at the same count
  CHECK(osprey_gpu_len(b) == 7);
  osprey_gpu_take(b, -3);   // negative clamps to zero: fully compacted away
  CHECK(osprey_gpu_len(b) == 0);
  CHECK(osprey_gpu_in_bounds(b, 0) == 0);
}

int main(void) {
  t_alloc_zero_filled();
  t_alloc_clamps_to_empty();
  t_alloc_failure_is_empty();
  t_null_safety();
  t_set_get_roundtrip();
  t_boundary_words();
  t_take_shrinks_only();
  printf("[ok] gpu_runtime: %ld assertions\n", g_checks);
  return 0;
}
