#include "collection_runtime.h"
#include "list_trie.h"
#include "memory_hooks.h"

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

/*
 * The transient list builder, and the list operations that rebuild or walk a
 * whole list through the trie's public API (collection_runtime.h): concat,
 * reverse, the stable sort and the iterator. The persistent trie and its O(1)
 * views live in list_runtime.c; the node layout both share is list_trie.h.
 * Compiled with the same memory shim as list_runtime.c in every backend
 * [GC-ARC-PERCEUS].
 *
 * Backs [BUILTIN-LIST-CONCAT], [BUILTIN-LIST-REVERSE] and [BUILTIN-LIST-SORT];
 * the iterator carries the linear scan emitted for [BUILTIN-LIST-CONTAINS] and
 * [BUILTIN-LIST-FOREACH].
 */

/* ===== Builder (transient-style, no sharing during construction) ===== */

OspreyListBuilder *osprey_list_builder_new_of(int em) {
  OspreyListBuilder *b =
      (OspreyListBuilder *)calloc(1, sizeof(OspreyListBuilder));
  b->elem_managed = em;
  return b;
}

OspreyListBuilder *osprey_list_builder_new(void) {
  return osprey_list_builder_new_of(0);
}

/* Stack a fresh root over the builder's root and its full tail chunk. Unlike
   grow_root this MOVES both builder references into the new node. */
static void builder_grow_root(OspreyListBuilder *b) {
  OspreyListNode *new_path = wrap_path(b->tail, b->shift);
  OspreyListNode *new_root = alloc_node(NODE_PTRS_LAYOUT);
  new_root->slots[0] = (int64_t)(uintptr_t)b->root;
  new_root->slots[1] = (int64_t)(uintptr_t)new_path;
  b->root = new_root;
  b->shift += OSPREY_LIST_BITS;
}

/* The builder is a transient it alone owns, so each branch MOVES its root and
   tail references — except the path-copying one, which leaves the old root
   unreachable and takes its own tail reference. */
static void builder_push_tail_to_tree(OspreyListBuilder *b) {
  int64_t in_tree = b->length - (int64_t)b->tail_count;
  int64_t capacity_at_shift = (int64_t)1 << (b->shift + OSPREY_LIST_BITS);
  if (b->root == NULL) {
    b->root = b->tail;
    b->shift = 0;
  } else if (in_tree + OSPREY_LIST_BRANCH > capacity_at_shift) {
    builder_grow_root(b);
  } else {
    OspreyListNode *old_root = b->root;
    b->root =
        push_tail(b->shift, old_root, b->tail, in_tree + OSPREY_LIST_BRANCH);
    osp_release(old_root);
    osp_release(b->tail);
  }
  b->tail = NULL;
  b->tail_count = 0;
}

/* TRANSFERS `v`. Every leaf the builder allocates is minted here, so latching
 * the element kind on each push keeps them all stamped alike — which lets a
 * caller that only learns the element type inside its loop (mapList) still
 * build a correctly walked list. */
void osprey_list_builder_push_of(OspreyListBuilder *b, int64_t v, int em) {
  b->elem_managed = em;
  if (b->tail == NULL || b->tail_count == OSPREY_LIST_BRANCH) {
    if (b->tail != NULL) {
      builder_push_tail_to_tree(b);
    }
    b->tail = alloc_node(node_layout(0, em));
    b->tail_count = 0;
  }
  b->tail->slots[b->tail_count] = v;
  b->tail_count++;
  b->length++;
}

void osprey_list_builder_push(OspreyListBuilder *b, int64_t v) {
  osprey_list_builder_push_of(b, v, b->elem_managed);
}

/* Push an element BORROWED from another container: the source keeps its
 * reference, so the builder takes its own. */
void osprey_list_builder_push_borrowed(OspreyListBuilder *b, int64_t v) {
  retain_elem(v, b->elem_managed);
  osprey_list_builder_push_of(b, v, b->elem_managed);
}

OspreyList *osprey_list_builder_seal(OspreyListBuilder *b) {
  if (b->length == 0) {
    free(b);
    return osprey_list_empty();
  }
  OspreyList *l = alloc_list(b->length, b->shift, b->tail_count, b->root,
                             b->tail, 0, b->elem_managed, NULL);
  free(b);
  return l;
}

/* Copy `src`'s elements into `b`; a C-internal rebuild BORROWS what it reads. */
static void builder_push_all(OspreyListBuilder *b, OspreyList *src) {
  int64_t n = osprey_list_length(src);
  for (int64_t i = 0; i < n; i++) {
    osprey_list_builder_push_borrowed(b, osprey_list_get(src, i));
  }
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
