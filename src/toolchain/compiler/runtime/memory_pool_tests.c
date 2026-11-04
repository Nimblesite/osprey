// Unit tests for the size-classed recycling pool (memory_pool.h), in isolation
// from any backend. The pool only threads intrusive free-lists through
// caller-owned bodies, so the test mallocs raw blocks and drives cap rounding,
// LIFO reuse at the same address, the intrusive next-free link, size-class
// isolation, and the oversize / retention-ceiling rejections. [MEM-BACKENDS]
//
// Header-only module: nothing to link. Built under the WARN unit-test core.

#include "memory_pool.h"

#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static long g_checks = 0;
#define CHECK(c)                                                               \
  do {                                                                         \
    g_checks++;                                                                \
    assert(c);                                                                 \
  } while (0)

// cap rounds up to a positive multiple of GRAIN, is monotone, and never
// under-allocates the request.
static void t_cap_rounding(void) {
  CHECK(osp_pool_cap(0) == OSP_POOL_GRAIN);
  CHECK(osp_pool_cap(1) == OSP_POOL_GRAIN);
  CHECK(osp_pool_cap(OSP_POOL_GRAIN) == OSP_POOL_GRAIN);
  CHECK(osp_pool_cap(OSP_POOL_GRAIN + 1) == 2 * OSP_POOL_GRAIN);
  CHECK(osp_pool_cap(24) == 32);
  CHECK(osp_pool_cap(OSP_POOL_MAX_CAP) == OSP_POOL_MAX_CAP);
  for (size_t s = 1; s <= 4096; s++) {
    size_t cap = osp_pool_cap(s);
    CHECK(cap >= s && cap % OSP_POOL_GRAIN == 0);
    CHECK(cap >= osp_pool_cap(s - 1)); // monotone
  }
}

// An empty class pops NULL; a pushed body pops back at the SAME address and the
// class drains to empty. Bytes accounting tracks both directions.
static void t_push_pop_same_address(void) {
  OspPool pool = {0};
  CHECK(osp_pool_pop(&pool, 32) == NULL);
  void *blk = malloc(32);
  CHECK(blk);
  CHECK(osp_pool_push(&pool, blk, 32) == 1);
  CHECK(pool.bytes == 32);
  CHECK(osp_pool_pop(&pool, 32) == blk); // reuse returns the identical block
  CHECK(pool.bytes == 0);
  CHECK(osp_pool_pop(&pool, 32) == NULL); // drained
  free(blk);
}

// The lists are LIFO and the next-free link lives in the freed body's word 0.
static void t_lifo_and_intrusive_link(void) {
  OspPool pool = {0};
  void *a = malloc(32);
  void *b = malloc(32);
  CHECK(a && b);
  CHECK(osp_pool_push(&pool, a, 32));
  CHECK(osp_pool_push(&pool, b, 32));
  CHECK(*(void **)b == a);              // b links to a (intrusive next-free)
  CHECK(osp_pool_pop(&pool, 32) == b);  // LIFO: last in, first out
  CHECK(osp_pool_pop(&pool, 32) == a);
  free(a);
  free(b);
}

// Different size classes never cross-satisfy a request.
static void t_size_class_isolation(void) {
  OspPool pool = {0};
  void *blk = malloc(64);
  CHECK(blk);
  CHECK(osp_pool_push(&pool, blk, 32));
  CHECK(osp_pool_pop(&pool, 48) == NULL); // wrong class -> miss
  CHECK(osp_pool_pop(&pool, 32) == blk);  // right class -> hit
  free(blk);
}

// Oversized bodies and pushes past the retention ceiling are rejected (0) so the
// caller hands them to libc instead; a rejected push never touches the body.
static void t_reject_oversize_and_ceiling(void) {
  OspPool pool = {0};
  static char big[OSP_POOL_MAX_CAP + OSP_POOL_GRAIN];
  CHECK(osp_pool_push(&pool, big, OSP_POOL_MAX_CAP + OSP_POOL_GRAIN) == 0);
  CHECK(pool.bytes == 0);

  OspPool tiny = {0};
  tiny.cap_bytes = 32; // room for exactly one 32-byte body
  void *a = malloc(32);
  void *b = malloc(32);
  CHECK(a && b);
  CHECK(osp_pool_push(&tiny, a, 32) == 1); // fits
  CHECK(osp_pool_push(&tiny, b, 32) == 0); // over the ceiling -> rejected
  CHECK(tiny.bytes == 32);
  CHECK(osp_pool_pop(&tiny, 32) == a);     // only `a` was parked
  free(a);
  free(b);
}

int main(void) {
  t_cap_rounding();
  t_push_pop_same_address();
  t_lifo_and_intrusive_link();
  t_size_class_isolation();
  t_reject_oversize_and_ceiling();
  printf("[ok] memory_pool: %ld assertions\n", g_checks);
  return 0;
}
