// Assertion-driven tests for memory_runtime.c — the DEFAULT backend of the
// [MEM-BACKENDS] / [MEM-BACKENDS-CUSTOM] ABI (docs/specs/0018). Its contract
// is precise no-op-ness: every hook exists and is safe, general dup/drop and
// the collector hook move NOTHING, and the one reclaiming entry point —
// osp_release_unique on a codegen-proved-unique value — really frees.
#include "memory_hooks.h"

#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

void *osp_alloc(int64_t size);
void osp_collect(void);

static long g_checks = 0;
#define CHECK(c)                                                               \
  do {                                                                         \
    g_checks++;                                                                \
    assert(c);                                                                 \
  } while (0)

#define BODY ((int64_t)64)
#define REUSE_TRIES 256

// Fresh blocks are non-NULL, distinct, and writable end to end; the layout
// word is ignored by this backend (any meta yields plain storage).
static void t_alloc_forms(void) {
  unsigned char *a = osp_alloc(BODY);
  unsigned char *b = osp_alloc_tagged(BODY, OSP_MEM_LIST_HDR_PTR);
  unsigned char *c = osp_alloc_tagged_noinit(BODY, OSP_MEM_PTR_ARRAY);
  CHECK(a && b && c);
  CHECK(a != b && b != c && a != c);
  memset(a, 0x11, BODY);
  memset(b, 0x22, BODY);
  memset(c, 0x33, BODY);
  CHECK(a[0] == 0x11 && a[BODY - 1] == 0x11);
  CHECK(b[0] == 0x22 && b[BODY - 1] == 0x22);
  CHECK(c[0] == 0x33 && c[BODY - 1] == 0x33);
}

// General retain/release, the layout stamp, immortality, notify, boot, and
// the collect hook are ALL no-ops: the block survives byte-for-byte and NULL
// is tolerated everywhere. [MEM-OPAQUE]
static void t_hooks_are_noops(void) {
  unsigned char *p = osp_alloc(BODY);
  CHECK(p != NULL);
  memset(p, 0x5C, BODY);
  osp_retain(p);
  osp_release(p);
  osp_mem_set_layout(p, OSP_MEM_LAYOUT(OSP_MEM_WORD(0)));
  osp_mem_immortal(p);
  osp_mem_notify_multithreaded();
  osp_mem_notify_multithreaded(); // idempotent
  osp_mem_boot();
  osp_mem_boot();
  osp_collect();
  for (int64_t i = 0; i < BODY; i++) {
    CHECK(p[i] == 0x5C);
  }
  osp_retain(NULL);
  osp_release(NULL);
  osp_release_unique(NULL);
  osp_mem_immortal(NULL);
  osp_mem_set_layout(NULL, OSP_MEM_RAW);
}

// osp_release_unique REALLY frees under this backend: released blocks come
// back from malloc. A silent no-op implementation would never reuse a single
// address across 256 same-size alloc/release pairs.
static void t_release_unique_reclaims(void) {
  int reused = 0;
  void *prev = NULL;
  for (int i = 0; i < REUSE_TRIES; i++) {
    void *p = osp_alloc(BODY);
    CHECK(p != NULL);
    reused += (p == prev);
    prev = p;
    osp_release_unique(p);
  }
  CHECK(reused > 0);
}

int main(void) {
  t_alloc_forms();
  t_hooks_are_noops();
  t_release_unique_reclaims();
  printf("[ok] memory_runtime: %ld assertions\n", g_checks);
  return 0;
}
