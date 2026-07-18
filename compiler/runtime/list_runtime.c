#include "collection_runtime.h"
#include "memory_hooks.h"

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

/*
 * Immutable bitmapped vector trie for List<T>.
 *
 * Reference design: Clojure PersistentVector (Phil Bagwell, "Ideal Hash
 * Trees", 2000; Rich Hickey's Clojure implementation).
 *
 * - 32-way branching (5 bits per level).
 * - "Tail" optimisation: the last partial 32-element chunk lives outside the
 *   tree, giving amortised O(1) append and O(1) length.
 * - Memory: malloc, never free. Old versions remain valid because
 *   path-copying never mutates a published node.
 *
 * Implements [TYPE-LIST], [TYPE-LIST-OPS] from
 * docs/specs/0004-TypeSystem.md.
 */

typedef struct OspreyListNode {
  int64_t slots[OSPREY_LIST_BRANCH];
} OspreyListNode;

struct OspreyList {
  int64_t length;
  int32_t shift;     /* 0 == root is a leaf; 5, 10, … one level deeper each */
  int32_t tail_count;
  OspreyListNode *root; /* NULL iff in-tree count == 0 */
  OspreyListNode *tail; /* NULL iff length == 0 */
};

struct OspreyListBuilder {
  int64_t length;
  int32_t shift;
  int32_t tail_count;
  OspreyListNode *root;
  OspreyListNode *tail;
};

struct OspreyListIter {
  OspreyList *list;
  int64_t index;
};

static OspreyListNode *alloc_node(void) {
  OspreyListNode *n = (OspreyListNode *)calloc(1, sizeof(OspreyListNode));
  return n;
}

static OspreyListNode *clone_node(OspreyListNode *src) {
  OspreyListNode *n = alloc_node();
  if (src != NULL) {
    memcpy(n->slots, src->slots, sizeof(src->slots));
  }
  return n;
}

static OspreyList *alloc_list(int64_t length, int32_t shift, int32_t tail_count,
                              OspreyListNode *root, OspreyListNode *tail) {
  OspreyList *l = (OspreyList *)calloc(1, sizeof(OspreyList));
  l->length = length;
  l->shift = shift;
  l->tail_count = tail_count;
  l->root = root;
  l->tail = tail;
  return l;
}

static OspreyList *singleton_empty = NULL;

OspreyList *osprey_list_empty(void) {
  if (singleton_empty == NULL) {
    singleton_empty = alloc_list(0, 0, 0, NULL, NULL);
    /* Returned from many sites and owned by many callers at once: under ARC
     * it must never be freed by any of them [GC-ARC-PERCEUS], plan 0011 M4. */
    osp_mem_immortal(singleton_empty);
  }
  return singleton_empty;
}

int64_t osprey_list_length(OspreyList *l) {
  if (l == NULL) {
    return 0;
  }
  return l->length;
}

int osprey_list_in_bounds(OspreyList *l, int64_t i) {
  if (l == NULL) {
    return 0;
  }
  return (i >= 0 && i < l->length) ? 1 : 0;
}

static int64_t tail_offset(OspreyList *l) {
  return l->length - (int64_t)l->tail_count;
}

int64_t osprey_list_get(OspreyList *l, int64_t i) {
  /* Out-of-range indices (negative or past end) used to walk the trie
   * with garbage shifts and dereference NULL/uninit nodes — SIGSEGV.
   * Codegen calls osprey_list_in_bounds first to set the Result
   * discriminant, but the value-side call still ran with the OOB
   * index. Return 0 here so the caller's in-bounds check picks the
   * Error variant cleanly. */
  if (l == NULL || i < 0 || i >= l->length) {
    return 0;
  }
  int64_t off = tail_offset(l);
  if (i >= off) {
    return l->tail->slots[i - off];
  }
  OspreyListNode *node = l->root;
  int32_t s = l->shift;
  while (s > 0) {
    int32_t slot = (int32_t)((i >> s) & OSPREY_LIST_MASK);
    node = (OspreyListNode *)(uintptr_t)node->slots[slot];
    s -= OSPREY_LIST_BITS;
  }
  return node->slots[i & OSPREY_LIST_MASK];
}

/* Set element at index, returning a path-copied list. Index must be in
   bounds. */
OspreyList *osprey_list_set(OspreyList *l, int64_t i, int64_t v) {
  int64_t off = tail_offset(l);
  if (i >= off) {
    OspreyListNode *new_tail = clone_node(l->tail);
    new_tail->slots[i - off] = v;
    return alloc_list(l->length, l->shift, l->tail_count, l->root, new_tail);
  }
  OspreyListNode *new_root = clone_node(l->root);
  OspreyListNode *cur = new_root;
  int32_t s = l->shift;
  while (s > 0) {
    int32_t slot = (int32_t)((i >> s) & OSPREY_LIST_MASK);
    OspreyListNode *child = (OspreyListNode *)(uintptr_t)cur->slots[slot];
    OspreyListNode *new_child = clone_node(child);
    cur->slots[slot] = (int64_t)(uintptr_t)new_child;
    cur = new_child;
    s -= OSPREY_LIST_BITS;
  }
  cur->slots[i & OSPREY_LIST_MASK] = v;
  return alloc_list(l->length, l->shift, l->tail_count, new_root, l->tail);
}

/* Insert the given tail chunk at position level. Path-copies as it descends.
   Returns the new node at this level. */
static OspreyListNode *push_tail(int32_t level, OspreyListNode *parent,
                                 OspreyListNode *tail_node,
                                 int64_t in_tree_count) {
  int32_t sub_index =
      (int32_t)(((in_tree_count - 1) >> level) & OSPREY_LIST_MASK);
  OspreyListNode *ret = clone_node(parent);
  OspreyListNode *node_to_insert;
  if (level == OSPREY_LIST_BITS) {
    node_to_insert = tail_node;
  } else {
    OspreyListNode *child = (OspreyListNode *)(uintptr_t)ret->slots[sub_index];
    if (child != NULL) {
      node_to_insert = push_tail(level - OSPREY_LIST_BITS, child, tail_node,
                                 in_tree_count);
    } else {
      OspreyListNode *new_path = tail_node;
      for (int32_t lvl = level - OSPREY_LIST_BITS; lvl > 0;
           lvl -= OSPREY_LIST_BITS) {
        OspreyListNode *wrap = alloc_node();
        wrap->slots[0] = (int64_t)(uintptr_t)new_path;
        new_path = wrap;
      }
      node_to_insert = new_path;
    }
  }
  ret->slots[sub_index] = (int64_t)(uintptr_t)node_to_insert;
  return ret;
}

OspreyList *osprey_list_append(OspreyList *l, int64_t v) {
  if (l == NULL || l->length == 0) {
    OspreyListNode *new_tail = alloc_node();
    new_tail->slots[0] = v;
    return alloc_list(1, 0, 1, NULL, new_tail);
  }
  /* Room in the tail? */
  if (l->tail_count < OSPREY_LIST_BRANCH) {
    OspreyListNode *new_tail = clone_node(l->tail);
    new_tail->slots[l->tail_count] = v;
    return alloc_list(l->length + 1, l->shift, l->tail_count + 1, l->root,
                      new_tail);
  }
  /* Tail is full. Push it into the tree; new tail = [v]. */
  OspreyListNode *new_root;
  int32_t new_shift = l->shift;
  int64_t in_tree = tail_offset(l);
  int64_t capacity_at_shift = (int64_t)1 << (l->shift + OSPREY_LIST_BITS);
  if (l->root == NULL) {
    new_root = l->tail;
    new_shift = 0;
  } else if (in_tree + OSPREY_LIST_BRANCH > capacity_at_shift) {
    /* Tree must grow one level. */
    OspreyListNode *new_path = l->tail;
    for (int32_t lvl = l->shift; lvl > 0; lvl -= OSPREY_LIST_BITS) {
      OspreyListNode *wrap = alloc_node();
      wrap->slots[0] = (int64_t)(uintptr_t)new_path;
      new_path = wrap;
    }
    new_root = alloc_node();
    new_root->slots[0] = (int64_t)(uintptr_t)l->root;
    new_root->slots[1] = (int64_t)(uintptr_t)new_path;
    new_shift = l->shift + OSPREY_LIST_BITS;
  } else {
    new_root =
        push_tail(l->shift, l->root, l->tail, in_tree + OSPREY_LIST_BRANCH);
  }
  OspreyListNode *new_tail = alloc_node();
  new_tail->slots[0] = v;
  return alloc_list(l->length + 1, new_shift, 1, new_root, new_tail);
}

OspreyList *osprey_list_prepend(OspreyList *l, int64_t v) {
  /* O(n): rebuild via builder. Acceptable for v1; spec documents trie
     concat upgrade path. */
  OspreyListBuilder *b = osprey_list_builder_new();
  osprey_list_builder_push(b, v);
  if (l != NULL && l->length > 0) {
    OspreyListIter *it = osprey_list_iter_new(l);
    int64_t val;
    while (osprey_list_iter_next(it, &val)) {
      osprey_list_builder_push(b, val);
    }
    free(it);
  }
  return osprey_list_builder_seal(b);
}

OspreyList *osprey_list_concat(OspreyList *a, OspreyList *b) {
  /* Alias returns carry a fresh +1 (retain-on-return): the caller owns every
   * returned handle, aliased or not [GC-ARC-PERCEUS], plan 0011 M4. */
  if (a == NULL || a->length == 0) {
    if (b == NULL) {
      return osprey_list_empty();
    }
    osp_retain(b);
    return (OspreyList *)b;
  }
  if (b == NULL || b->length == 0) {
    osp_retain(a);
    return (OspreyList *)a;
  }
  OspreyListBuilder *bld = osprey_list_builder_new();
  OspreyListIter *ita = osprey_list_iter_new(a);
  int64_t v;
  while (osprey_list_iter_next(ita, &v)) {
    osprey_list_builder_push(bld, v);
  }
  free(ita);
  OspreyListIter *itb = osprey_list_iter_new(b);
  while (osprey_list_iter_next(itb, &v)) {
    osprey_list_builder_push(bld, v);
  }
  free(itb);
  return osprey_list_builder_seal(bld);
}

OspreyList *osprey_list_drop(OspreyList *l, int64_t n) {
  if (l == NULL || n <= 0) {
    osp_retain(l); /* alias return: +1 to the caller (see concat) */
    return (OspreyList *)l;
  }
  if (n >= l->length) {
    return osprey_list_empty();
  }
  OspreyListBuilder *b = osprey_list_builder_new();
  for (int64_t i = n; i < l->length; i++) {
    osprey_list_builder_push(b, osprey_list_get(l, i));
  }
  return osprey_list_builder_seal(b);
}

OspreyList *osprey_list_reverse(OspreyList *l) {
  OspreyListBuilder *b = osprey_list_builder_new();
  if (l != NULL) {
    for (int64_t i = l->length - 1; i >= 0; i--) {
      osprey_list_builder_push(b, osprey_list_get(l, i));
    }
  }
  return osprey_list_builder_seal(b);
}

/* ===== Builder (transient-style, no sharing during construction) ===== */

OspreyListBuilder *osprey_list_builder_new(void) {
  OspreyListBuilder *b = (OspreyListBuilder *)calloc(1, sizeof(OspreyListBuilder));
  return b;
}

static void builder_push_tail_to_tree(OspreyListBuilder *b) {
  int64_t in_tree = b->length - (int64_t)b->tail_count;
  int64_t capacity_at_shift = (int64_t)1 << (b->shift + OSPREY_LIST_BITS);
  if (b->root == NULL) {
    b->root = b->tail;
    b->shift = 0;
  } else if (in_tree + OSPREY_LIST_BRANCH > capacity_at_shift) {
    OspreyListNode *new_path = b->tail;
    for (int32_t lvl = b->shift; lvl > 0; lvl -= OSPREY_LIST_BITS) {
      OspreyListNode *wrap = alloc_node();
      wrap->slots[0] = (int64_t)(uintptr_t)new_path;
      new_path = wrap;
    }
    OspreyListNode *new_root = alloc_node();
    new_root->slots[0] = (int64_t)(uintptr_t)b->root;
    new_root->slots[1] = (int64_t)(uintptr_t)new_path;
    b->root = new_root;
    b->shift += OSPREY_LIST_BITS;
  } else {
    b->root = push_tail(b->shift, b->root, b->tail,
                        in_tree + OSPREY_LIST_BRANCH);
  }
  b->tail = NULL;
  b->tail_count = 0;
}

void osprey_list_builder_push(OspreyListBuilder *b, int64_t v) {
  if (b->tail == NULL) {
    b->tail = alloc_node();
    b->tail_count = 0;
  }
  if (b->tail_count == OSPREY_LIST_BRANCH) {
    builder_push_tail_to_tree(b);
    b->tail = alloc_node();
    b->tail_count = 0;
  }
  b->tail->slots[b->tail_count] = v;
  b->tail_count++;
  b->length++;
}

OspreyList *osprey_list_builder_seal(OspreyListBuilder *b) {
  if (b->length == 0) {
    free(b);
    return osprey_list_empty();
  }
  OspreyList *l = alloc_list(b->length, b->shift, b->tail_count, b->root,
                             b->tail);
  free(b);
  return l;
}

/* ===== Iterator ===== */

OspreyListIter *osprey_list_iter_new(OspreyList *l) {
  OspreyListIter *it = (OspreyListIter *)calloc(1, sizeof(OspreyListIter));
  it->list = l;
  it->index = 0;
  return it;
}

int osprey_list_iter_next(OspreyListIter *it, int64_t *out) {
  if (it->list == NULL || it->index >= it->list->length) {
    return 0;
  }
  *out = osprey_list_get(it->list, it->index);
  it->index++;
  return 1;
}
