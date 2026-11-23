// Assertion-driven tests for memory_ownership.c, the `--memory=ownership`
// backend of the [MEM-BACKENDS] ABI ([MEM-OWNERSHIP-STATIC],
// docs/specs/compiler/0018-MemoryManagement.md). Its contract: no reference
// counts. An owned block's one release frees it and every block its layout
// word owns; a block a runtime unit retained is never freed by any release, so
// no holder can read freed memory; immortals and foreign pointers are never
// touched.
#include "memory_hooks.h"

#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

void *osp_alloc(int64_t size);
size_t osp_arc_live_objects(void);

static long g_checks = 0;
#define CHECK(c)                                                               \
  do {                                                                         \
    g_checks++;                                                                \
    assert(c);                                                                 \
  } while (0)

#define BODY ((int64_t)32)
#define CHILD_WORD 0
#define SIBLING_WORD 8

// A two-word parent owning `child` and `sibling` through its layout word.
static void **parent_of(void *child, void *sibling) {
  int64_t meta = OSP_MEM_LAYOUT(OSP_MEM_WORD(CHILD_WORD) | OSP_MEM_WORD(SIBLING_WORD));
  void **parent = osp_alloc_tagged(2 * (int64_t)sizeof(void *), meta);
  CHECK(parent != NULL);
  parent[0] = child;
  parent[1] = sibling;
  return parent;
}

// The owner's one release frees the block and every child it owns; a second
// owner's release would be a double free, so nothing else is ever released.
static void t_release_frees_the_owned_tree(void) {
  size_t base = osp_arc_live_objects();
  void *leaf = osp_alloc_tagged(BODY, OSP_MEM_RAW);
  void **inner = parent_of(leaf, NULL);
  void **root = parent_of(inner, osp_alloc(BODY));
  CHECK(osp_arc_live_objects() == base + 4);
  osp_release(root);
  CHECK(osp_arc_live_objects() == base);
  void *unique = osp_alloc(BODY);
  osp_release_unique(unique);
  CHECK(osp_arc_live_objects() == base);
}

// A retained block has two holders: neither release frees it, and its owner's
// drop walk leaves it allocated, so both holders keep reading live memory.
static void t_retained_blocks_are_never_freed(void) {
  size_t base = osp_arc_live_objects();
  unsigned char *shared = osp_alloc(BODY);
  memset(shared, 0x5A, BODY);
  osp_retain(shared);
  osp_retain(shared); // idempotent: still one shared block
  void **parent = parent_of(shared, NULL);
  osp_release(parent);
  CHECK(osp_arc_live_objects() == base + 1);
  osp_release(shared);
  osp_release_unique(shared);
  CHECK(osp_arc_live_objects() == base + 1);
  for (int64_t i = 0; i < BODY; i++) {
    CHECK(shared[i] == 0x5A);
  }
}

// Immortals, foreign pointers and NULL are outside the heap's ownership.
static void t_immortal_and_foreign_are_untouched(void) {
  size_t base = osp_arc_live_objects();
  unsigned char *forever = osp_alloc(BODY);
  memset(forever, 0x3C, BODY);
  osp_mem_immortal(forever);
  osp_release(forever);
  osp_retain(forever);
  osp_release(forever);
  CHECK(forever[0] == 0x3C && forever[BODY - 1] == 0x3C);
  CHECK(osp_arc_live_objects() == base + 1);
  int64_t local = 7;
  osp_retain(&local);
  osp_release(&local);
  osp_release_unique(&local);
  CHECK(local == 7);
  osp_retain(NULL);
  osp_release(NULL);
  osp_release_unique(NULL);
}

int main(void) {
  // Arm the live counters before the first allocation (memory_tracked_heap.h).
  CHECK(setenv("OSPREY_OWNERSHIP_DEBUG", "1", 1) == 0);
  osp_mem_boot();
  t_release_frees_the_owned_tree();
  t_retained_blocks_are_never_freed();
  t_immortal_and_foreign_are_untouched();
  printf("[ok] memory_ownership: %ld assertions\n", g_checks);
  return 0;
}
