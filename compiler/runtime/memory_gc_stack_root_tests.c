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

#if defined(OSP_SYMBOLIC) && defined(OSP_HUNT_ARC)
void osp_retain(void *object);
void osp_release(void *object);
size_t osp_arc_live_objects(void);

int main(void) {
  CHECK(osp_arc_live_objects() == 0);
  unsigned char *root = osp_alloc(8);
  CHECK(root != NULL);
  memset(root, 'A', 8);
  CHECK(osp_arc_live_objects() == 1);
  osp_retain(root);
  osp_release(root);
  CHECK(osp_arc_live_objects() == 1);
  CHECK(root[0] == 'A' && root[7] == 'A');
  osp_release(root);
  CHECK(osp_arc_live_objects() == 0);
  return 0;
}
#elif !defined(OSP_SYMBOLIC)
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
#else
size_t osp_gc_live_objects(void);
size_t osp_gc_collections(void);
static unsigned char *global_root;
typedef struct { unsigned char *child; } Parent;

__attribute__((noinline)) static void make_global_root(void) {
  global_root = osp_alloc(8);
  CHECK(global_root != NULL);
  memset(global_root, 'G', 8);
}

__attribute__((noinline)) static Parent *make_parent(void) {
  Parent *parent = osp_alloc(sizeof(Parent));
  CHECK(parent != NULL);
  parent->child = osp_alloc(8);
  CHECK(parent->child != NULL);
  memset(parent->child, 'K', 8);
  return parent;
}

__attribute__((noinline)) static void make_garbage(void) {
  void *garbage = osp_alloc(8);
  CHECK(garbage != NULL);
  memset(garbage, 'D', 8);
}

__attribute__((noinline)) static void collect_nested(void) { osp_collect(); }

__attribute__((noinline)) static void roots_survive_real_collection(void) {
  unsigned char *root = osp_alloc(8);
  CHECK(root != NULL);
  memset(root, 'S', 8);
  make_global_root();
  Parent *parent = make_parent();
  make_garbage();
  collect_nested();
  CHECK(osp_gc_collections() == 1);
  CHECK(osp_gc_live_objects() == 4);
  for (int i = 0; i < 8; i++) {
    CHECK(root[i] == 'S');
    CHECK(global_root[i] == 'G');
    CHECK(parent->child[i] == 'K');
  }
}

int main(void) {
  roots_survive_real_collection();
  global_root = NULL;
  collect_nested();
  CHECK(osp_gc_collections() == 2);
  CHECK(osp_gc_live_objects() == 0);
  printf("[ok] symbolic GC roots: %ld assertions\n", g_checks);
  return 0;
}
#endif
