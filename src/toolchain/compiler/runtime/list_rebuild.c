#include "collection_runtime.h"
#include "memory_hooks.h"

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

/*
 * List operations that rebuild or walk a whole list through the trie's public
 * API (collection_runtime.h), never its nodes: prepend, concat, reverse, the
 * stable sort and the iterator. The trie itself, its builder and its O(1)
 * views live in list_runtime.c. Compiled with the same memory shim as
 * list_runtime.c in every backend [GC-ARC-PERCEUS].
 *
 * Backs [BUILTIN-LIST-PREPEND], [BUILTIN-LIST-CONCAT], [BUILTIN-LIST-REVERSE]
 * and [BUILTIN-LIST-SORT]; the iterator carries the linear scan emitted for
 * [BUILTIN-LIST-CONTAINS] and [BUILTIN-LIST-FOREACH].
 */

/* Copy `src`'s elements into `b`; a C-internal rebuild BORROWS what it reads. */
static void builder_push_all(OspreyListBuilder *b, OspreyList *src) {
  int64_t n = osprey_list_length(src);
  for (int64_t i = 0; i < n; i++) {
    osprey_list_builder_push_borrowed(b, osprey_list_get(src, i));
  }
}

/* O(n): rebuild via builder. Spec documents the trie-concat upgrade path. */
OspreyList *osprey_list_prepend_of(OspreyList *l, int64_t v, int em) {
  OspreyListBuilder *b = osprey_list_builder_new_of(em);
  osprey_list_builder_push_of(b, v, em);
  builder_push_all(b, l);
  return osprey_list_builder_seal(b);
}

OspreyList *osprey_list_prepend(OspreyList *l, int64_t v) {
  return osprey_list_prepend_of(l, v, osprey_list_elem_managed(l));
}

OspreyList *osprey_list_concat(OspreyList *a, OspreyList *b) {
  /* Alias returns carry a fresh +1 (retain-on-return): the caller owns every
   * returned handle, aliased or not [GC-ARC-PERCEUS]. */
  if (osprey_list_length(a) == 0) {
    if (b == NULL) {
      return osprey_list_empty();
    }
    osp_retain(b);
    return (OspreyList *)b;
  }
  if (osprey_list_length(b) == 0) {
    osp_retain(a);
    return (OspreyList *)a;
  }
  OspreyListBuilder *bld =
      osprey_list_builder_new_of(osprey_list_elem_managed(a));
  builder_push_all(bld, a);
  builder_push_all(bld, b);
  return osprey_list_builder_seal(bld);
}

OspreyList *osprey_list_reverse(OspreyList *l) {
  OspreyListBuilder *b =
      osprey_list_builder_new_of(osprey_list_elem_managed(l));
  for (int64_t i = osprey_list_length(l) - 1; i >= 0; i--) {
    osprey_list_builder_push_borrowed(b, osprey_list_get(l, i));
  }
  return osprey_list_builder_seal(b);
}

/* Natural ascending order for [BUILTIN-LIST-SORT]: strings compare by byte
   content, integers by signed value. Codegen guarantees the element type is
   one of those two, so the elem_managed flag alone selects the comparison and
   no other element kind can reach this branch. */
static int64_t list_sort_less(OspreyList *l, int64_t a, int64_t b) {
  if (osprey_list_elem_managed(l)) {
    return strcmp((const char *)(uintptr_t)a, (const char *)(uintptr_t)b) < 0;
  }
  return a < b;
}

/* Merge the two sorted halves [lo, mid) and [mid, hi) into `scratch`, then
   copy the merged run back over `src`. Taking the LEFT element on a tie keeps
   equal elements in their original relative order, making the whole sort
   stable [BUILTIN-LIST-SORT]. */
static void list_sort_merge(OspreyList *l, int64_t *src, int64_t *scratch,
                            int64_t lo, int64_t mid, int64_t hi) {
  int64_t i = lo;
  int64_t j = mid;
  int64_t k = lo;
  while (i < mid && j < hi) {
    scratch[k++] = list_sort_less(l, src[j], src[i]) ? src[j++] : src[i++];
  }
  while (i < mid) {
    scratch[k++] = src[i++];
  }
  while (j < hi) {
    scratch[k++] = src[j++];
  }
  memcpy(src + lo, scratch + lo, (size_t)(hi - lo) * sizeof(int64_t));
}

static void list_sort_range(OspreyList *l, int64_t *src, int64_t *scratch,
                            int64_t lo, int64_t hi) {
  if (hi - lo < 2) {
    return;
  }
  int64_t mid = lo + (hi - lo) / 2;
  list_sort_range(l, src, scratch, lo, mid);
  list_sort_range(l, src, scratch, mid, hi);
  list_sort_merge(l, src, scratch, lo, mid, hi);
}

/* Stable ascending sort of List<int> / List<string>. The borrowed elements are
   copied into a scratch pair, merge-sorted in place, then pushed through the
   builder (which dups each borrowed element) so the result owns its elements
   exactly as `listReverse`'s does. */
OspreyList *osprey_list_sort(OspreyList *l) {
  int64_t n = osprey_list_length(l);
  if (n < 2) {
    osp_retain(l); /* alias return: +1 to the caller (see concat) */
    return (OspreyList *)l;
  }
  int64_t *src = (int64_t *)malloc((size_t)n * sizeof(int64_t));
  int64_t *scratch = (int64_t *)malloc((size_t)n * sizeof(int64_t));
  for (int64_t i = 0; i < n; i++) {
    src[i] = osprey_list_get(l, i);
  }
  list_sort_range(l, src, scratch, 0, n);
  OspreyListBuilder *b =
      osprey_list_builder_new_of(osprey_list_elem_managed(l));
  for (int64_t i = 0; i < n; i++) {
    osprey_list_builder_push_borrowed(b, src[i]);
  }
  free(src);
  free(scratch);
  return osprey_list_builder_seal(b);
}

struct OspreyListIter {
  OspreyList *list;
  int64_t index;
};

/* ===== Iterator ===== */

OspreyListIter *osprey_list_iter_new(OspreyList *l) {
  OspreyListIter *it = (OspreyListIter *)calloc(1, sizeof(OspreyListIter));
  it->list = l;
  it->index = 0;
  return it;
}

int osprey_list_iter_next(OspreyListIter *it, int64_t *out) {
  if (it->index >= osprey_list_length(it->list)) {
    return 0;
  }
  *out = osprey_list_get(it->list, it->index);
  it->index++;
  return 1;
}
