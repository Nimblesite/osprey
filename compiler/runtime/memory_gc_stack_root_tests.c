// Regression test for conservative GC roots held in an outer caller frame.
// The pointer itself is volatile so it remains in main's stack slot while a
// nested allocation loop crosses the collection threshold. A collector that
// scans only its own frames can reclaim and overwrite this still-live block.

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

static void churn_and_clobber(unsigned char byte) {
  for (int i = 0; i < 150000; i++) {
    unsigned char *p = osp_alloc(64);
    if (p) {
      memset(p, byte, 64);
    }
  }
}

static void t_outer_stack_root_survives_nested_collection(void) {
  unsigned char *volatile root = osp_alloc(64);
  CHECK(root != NULL);
  memset(root, 0x5A, 64);
  churn_and_clobber(0xA5);
  osp_collect();
  churn_and_clobber(0xC3);
  for (int i = 0; i < 64; i++) {
    CHECK(root[i] == 0x5A);
  }
}

int main(void) {
  t_outer_stack_root_survives_nested_collection();
  printf("[ok] memory_gc_stack_root: %ld assertions\n", g_checks);
  return 0;
}
