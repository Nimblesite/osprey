#include "memory_hooks.h"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
// A scalar that equals a live object's address still owns no reference.
// Run on wasm32; the two fields share one 8-byte layout-mask word.
static int32_t count(void *p) {
  int32_t n;
  memcpy(&n, (char *)p - 8, 4);
  return n;
}
static void *held_raw(void) {
  void *body = osp_alloc_tagged(8, OSP_MEM_RAW);
  if (body)
    osp_retain(body);
  return body;
}
static void release_twice(void *body) {
  osp_release(body);
  osp_release(body);
}
static int scalar_ownership(void) {
  void *child = held_raw(), *other = held_raw();
  void *parent = osp_alloc_tagged(8, OSP_MEM_LAYOUT(OSP_MEM_WORD(0)));
  if (!child || !other || !parent || sizeof(void *) != 4)
    return 2;
  uint32_t scalar = (uint32_t)(uintptr_t)other;
  memcpy(parent, &child, 4);
  memcpy((char *)parent + 4, &scalar, 4);
  osp_release(parent);
  int32_t actual = count(other);
  printf("unrelated scalar target reference count: expected=2 actual=%d\n",
         actual);
  osp_release(child);
  release_twice(other);
  return actual == 2 ? 0 : 1;
}
typedef struct {
  int64_t length;
  void **data;
} PointerListFixture;
static void *pointer_list_owner(void *first, void *second, void *other) {
  void **data = osp_alloc_tagged(4 * sizeof(void *), OSP_MEM_RAW);
  PointerListFixture *list = osp_alloc_tagged(16, OSP_MEM_LIST_HDR_PTR);
  if (!data || !list)
    return NULL;
  data[0] = first;
  data[1] = second;
  data[2] = other;
  data[3] = NULL;
  list->length = 2;
  list->data = data;
  return list;
}
static int pointer_list_ownership(void) {
  void *first = held_raw(), *second = held_raw(), *other = held_raw();
  void *list = pointer_list_owner(first, second, other);
  if (!first || !second || !other || !list)
    return 2;
  osp_release(list);
  int valid = count(first) == 1 && count(second) == 1 && count(other) == 2;
  printf("pointer list children: expected=1,1,2 actual=%d,%d,%d\n",
         count(first), count(second), count(other));
  osp_release(first);
  osp_release(second);
  release_twice(other);
  return valid ? 0 : 1;
}

extern void *osp_arc_realloc(void *old, size_t size);
static int foreign_ownership(void) {
  void *child = osp_alloc_tagged(64, OSP_MEM_RAW);
  void *foreign = malloc(64);
  if (!child || !foreign)
    return 2;
  void *candidates[] = {foreign, (char *)child + 1, (char *)child + 8,
                        (void *)(UINTPTR_MAX & ~(uintptr_t)15)};
  for (size_t i = 0; i < sizeof(candidates) / sizeof(candidates[0]); i++) {
    osp_retain(candidates[i]);
    osp_release(candidates[i]);
  }
  int valid = count(child) == 1;
  free(foreign);
  osp_release(child);
  return valid ? 0 : 1;
}
static int realloc_ownership(void) {
  void *body = osp_alloc_tagged(64, OSP_MEM_RAW);
  if (!body)
    return 2;
  void *changed = osp_arc_realloc(body, 128);
  if (!changed) {
    osp_release(body);
    return 2;
  }
  osp_retain(changed);
  int valid = count(changed) == 2;
  osp_release(changed);
  osp_release(changed);
  return valid ? 0 : 1;
}
static int retired_ownership(void) {
  void *large = osp_alloc_tagged(1048576, OSP_MEM_RAW);
  if (!large)
    return 2;
  osp_release(large);
  osp_retain(large);
  osp_release(large);
  return 0;
}

static int balanced_once(void *body) {
  if (!body)
    return 0;
  osp_retain(body);
  osp_release(body);
  return count(body) == 1;
}
static int growth_ownership(void) {
  enum { ITEMS = 16000 };
  void **bodies = calloc(ITEMS, sizeof(*bodies));
  if (!bodies)
    return 2;
  int valid = 1;
  for (size_t i = 0; i < ITEMS; i++) {
    bodies[i] = osp_alloc_tagged(64, OSP_MEM_RAW);
    valid = balanced_once(bodies[i]) && valid;
  }
  for (size_t i = 0; i < ITEMS; i++)
    osp_release(bodies[i]);
  free(bodies);
  return valid ? 0 : 1;
}
#if defined(OSP_ARC_TEST_PAGE_OOM) || defined(OSP_ARC_TEST_REGISTRY_OOM)
#ifdef OSP_ARC_TEST_PAGE_OOM
static int failed_page;
static size_t page_allocations;
#endif
extern void *__real_calloc(size_t, size_t);
void *__wrap_calloc(size_t element_count, size_t element_size) {
#ifdef OSP_ARC_TEST_REGISTRY_OOM
  if (element_count >= 16384 && element_size == sizeof(uintptr_t))
    return NULL;
#endif
#ifdef OSP_ARC_TEST_PAGE_OOM
  if (element_count == 1024 && element_size == 1 &&
      ++page_allocations == OSP_ARC_TEST_PAGE_OOM) {
    failed_page = 1;
    return NULL;
  }
#endif
  return __real_calloc(element_count, element_size);
}
#endif
#ifdef OSP_ARC_TEST_PAGE_OOM
static int allocation_failure_checked(void) { return !failed_page; }
#else
static int allocation_failure_checked(void) { return 0; }
#endif

#ifdef OSP_ARC_TEST_WORKLIST_OOM
extern void *__real_realloc(void *, size_t);
void *__wrap_realloc(void *old, size_t size) {
  (void)old;
  (void)size;
  return NULL;
}
#endif

static int bookkeeping_fault(void) {
#ifdef OSP_ARC_TEST_REGISTRY_OOM
  (void)osp_alloc_tagged(8, OSP_MEM_RAW);
  return 0;
#endif
#ifdef OSP_ARC_TEST_WORKLIST_OOM
  void *probe = osp_alloc_tagged(8, OSP_MEM_RAW);
  if (!probe)
    return 2;
  osp_release(probe);
  return 0;
#endif
  return -1;
}

int main(void) {
  int fault = bookkeeping_fault();
  if (fault >= 0)
    return fault;
  int scalar = scalar_ownership();
  int pointers = pointer_list_ownership();
  int foreign = foreign_ownership();
  int resized = realloc_ownership();
  int retired = retired_ownership();
  int growth = growth_ownership();
  return scalar != 0 || pointers != 0 || foreign != 0 || resized != 0 ||
         retired != 0 || growth != 0 || allocation_failure_checked() != 0;
}
