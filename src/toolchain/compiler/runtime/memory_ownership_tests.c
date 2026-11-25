// Assertion-driven tests for memory_ownership.c, the `--memory=ownership`
// backend of the [MEM-BACKENDS] ABI ([MEM-OWNERSHIP-STATIC],
// docs/specs/compiler/0018-MemoryManagement.md). Its contract: no reference
// counts. An owned block's one release frees it and every block its layout
// word owns; a block a new version took passes, unfreed, to that version at its
// source's release; a block taken twice is never freed, so no holder can read
// freed memory; immortals and foreign pointers are never touched.
#include "memory_hooks.h"
#include "collection_runtime.h"

#include <assert.h>
#include <errno.h>
#ifndef _WIN32
#include <sys/mman.h>
#include <unistd.h>
#endif
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef OSP_TEST_POISONED_FREES
#include <sanitizer/asan_interface.h>
#endif

void *osp_alloc(int64_t size);
void osp_collect(void);
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
  osp_ownership_free(root);
  CHECK(osp_arc_live_objects() == base);
#ifdef OSP_TEST_POISONED_FREES
  CHECK(__asan_address_is_poisoned(root));
  CHECK(__asan_address_is_poisoned(inner));
  CHECK(__asan_address_is_poisoned(leaf));
#endif
  unsigned char *unique = osp_alloc(BODY);
  CHECK(unique != NULL);
  unique[0] = 0x5A;
  osp_release_unique(unique);
  osp_collect();
  CHECK(osp_arc_live_objects() == base + 1);
  CHECK(unique[0] == 0x5A);
  osp_ownership_free(unique);
  CHECK(osp_arc_live_objects() == base);
}

// A block taken twice before a handover has two holders: neither release frees
// it, and its owner's drop walk leaves it allocated, so both keep reading it.
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

// A derived version takes a node from its moved source
// ([MEM-OWNERSHIP-STATIC-SHARING]): the copy marks the node moving, the
// source's release hands it over without freeing it, and the new version's
// release frees it. Repeated derivations each move the node on once.
static void t_moved_block_passes_to_its_new_owner(void) {
  size_t base = osp_arc_live_objects();
  unsigned char *node = osp_alloc(BODY);
  memset(node, 0x6B, BODY);
  void **version = parent_of(node, NULL);
  for (int step = 0; step < 3; step++) {
    osp_retain(node);
    void **next = parent_of(node, NULL);
    osp_release(version);
    CHECK(osp_arc_live_objects() == base + 2);
    CHECK(node[0] == 0x6B && node[BODY - 1] == 0x6B);
    version = next;
  }
  osp_ownership_free(version);
  CHECK(osp_arc_live_objects() == base);
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

// A generic ABI registration must neither run nor suppress the typed GPU drop.
static int generic_calls;
static void count_generic(void *body) { (void)body; generic_calls++; }

#ifndef _WIN32
void *osprey_gpu_map_file(const char *, int64_t, int64_t, int64_t);
void *osprey_gpu_data(void *);

static void t_external_mapping_drop(void) {
  char path[] = "/tmp/osprey-own-map-XXXXXX";
  int fd = mkstemp(path);
  CHECK(fd >= 0 && write(fd, "m", 1) == 1 && close(fd) == 0);
  size_t base = osp_arc_live_objects();
  void *buffer = osprey_gpu_map_file(path, 0, 1, 1);
  CHECK(buffer != NULL && unlink(path) == 0);
  char *page = osprey_gpu_data(buffer);
  CHECK(page != NULL && page[0] == 'm' && msync(page, 1, MS_SYNC) == 0);
  osp_mem_set_finalizer(OSP_MEM_LIST_HDR_EXTERN, count_generic);
  osp_ownership_free(buffer);
  CHECK(generic_calls == 0);
  CHECK(msync(page, 1, MS_SYNC) == -1 && errno == ENOMEM);
  CHECK(osp_arc_live_objects() == base);
}
#endif

// One past the second trie level exercises the front, inner paths and tail.
enum { LAST_INDEX = 1056 };
static int64_t managed_int(int number) {
  int *value = osp_alloc(sizeof(*value));
  CHECK(value != NULL);
  *value = number;
  return (int64_t)(uintptr_t)value;
}

static OspreyList *managed_sequence(int prepend) {
  OspreyList *list = osprey_list_empty();
  for (int i = 0; i <= LAST_INDEX; i++) {
    int64_t word = managed_int(i);
    OspreyList *next = prepend ? osprey_list_prepend_of(list, word, 1)
                               : osprey_list_append_of(list, word, 1);
    osp_ownership_free(list);
    list = next;
  }
  return list;
}

// Transfer a fresh managed value to a derived list and drop the source.
static void t_prepend_transferred_value(int prepend) {
  const int indices[] = {0, 31, 32, 1023, 1024, LAST_INDEX};
  (void)osprey_list_empty(); // Initialize the singleton before measuring.
  size_t base = osp_arc_live_objects();
  OspreyList *list = managed_sequence(prepend);
  for (size_t i = 0; i < sizeof(indices) / sizeof(indices[0]); i++) {
    int64_t index = indices[i] + (int64_t)i;
    int64_t existing = osprey_list_get(list, index);
    int64_t value = managed_int(-1 - (int)i);
    OspreyList *next = osprey_list_prepend_of(list, value, 1);
    osp_ownership_free(list);
    CHECK(osprey_list_get(next, 0) == value && *(int *)(uintptr_t)value == -1 - (int)i);
    CHECK(osprey_list_get(next, index + 1) == existing);
    CHECK(*(int *)(uintptr_t)existing == (prepend ? LAST_INDEX - indices[i] : indices[i]));
    list = next;
  }
  osp_ownership_free(list);
  CHECK(osp_arc_live_objects() == base);
}

// Distinct FNV-1a keys with hash 1246396058 force the production collision path.
static int64_t owned_key(const char *text) {
  size_t bytes = strlen(text) + 1;
  char *key = osp_alloc_tagged((int64_t)bytes, OSP_MEM_STRING);
  CHECK(key != NULL);
  memcpy(key, text, bytes);
  return (int64_t)(uintptr_t)key;
}

static void t_collision_transfer(void) {
  size_t base = osp_arc_live_objects();
  const int64_t keys[] = {owned_key("keyktfa"), owned_key("key51wu")};
  OspreyMap *map = osprey_map_empty(OSPREY_KEY_STRING);
  for (int i = 0; i < 2; i++) {
    OspreyMap *next = osprey_map_set_of(map, keys[i], owned_key("value"), 1);
    osp_ownership_free(map);
    map = next;
  }
  for (int i = 0; i < 2; i++) {
    int64_t value = osprey_map_get(map, keys[i]);
    osp_retain((void *)(uintptr_t)keys[i]);
    osp_retain((void *)(uintptr_t)value);
    OspreyMap *next = osprey_map_set_of(map, keys[i], value, 1);
    osp_ownership_free(map);
    CHECK(osprey_map_length(next) == 2 && osprey_map_get(next, keys[i]) == value);
    CHECK(strcmp((char *)(uintptr_t)value, "value") == 0);
    map = next;
  }
  osp_ownership_free(map);
  CHECK(osp_arc_live_objects() == base);
}

int main(void) {
  // Arm the live counters before the first allocation (memory_tracked_heap.h).
  CHECK(setenv("OSPREY_OWNERSHIP_DEBUG", "1", 1) == 0);
  CHECK(setenv("OSPREY_SCRIBBLE_FREES", "1", 1) == 0);
  osp_mem_boot();
  osp_mem_set_finalizer(OSP_MEM_LIST_HDR_EXTERN, count_generic);
  size_t base = osp_arc_live_objects();
  void *external = osp_alloc_tagged(16, OSP_MEM_LIST_HDR_EXTERN);
  CHECK(external != NULL && osp_arc_live_objects() == base + 1);
  osp_ownership_free(external);
  CHECK(generic_calls == 0 && osp_arc_live_objects() == base);
#ifndef _WIN32
  t_external_mapping_drop();
#endif
  t_collision_transfer();
  t_prepend_transferred_value(0);
  t_prepend_transferred_value(1);
  t_release_frees_the_owned_tree();
  t_retained_blocks_are_never_freed();
  t_moved_block_passes_to_its_new_owner();
  t_immortal_and_foreign_are_untouched();
  printf("[ok] memory_ownership: %ld assertions\n", g_checks);
  return 0;
}
