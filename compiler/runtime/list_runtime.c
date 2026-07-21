#include "collection_runtime.h"
#include "memory_hooks.h"

#include <stddef.h>
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
 * - Memory: reference-counted per node under the ARC backend (a node's count is
 *   the number of parents pointing at it), so a dead list reclaims its whole
 *   skeleton while every version that still shares a node keeps it alive; a
 *   no-op under the default/tracing backends. Path-copying still never mutates
 *   a published node. [GC-ARC-PERCEUS], docs/plans/0011 M4b.
 *
 * Implements [TYPE-LIST], [TYPE-LIST-OPS] from
 * docs/specs/0004-TypeSystem.md.
 */

typedef struct OspreyListNode {
  int64_t slots[OSPREY_LIST_BRANCH];
} OspreyListNode;

/* ARC layout words: which 8-byte words of an object hold managed pointers.
 * Derived from offsetof so they cannot drift from the structs.
 *
 * Element slots are deliberately absent: a stored element is a type-blind
 * int64 the container never typed, so releasing one would be a guess. A list
 * BORROWS its elements — codegen dups them at insertion and nothing here ever
 * drops them (docs/plans/0011 M4a). Only the skeleton is reclaimed. */
#define LIST_HDR_LAYOUT                                                        \
  OSP_MEM_LAYOUT(OSP_MEM_WORD(offsetof(OspreyList, root)) |                    \
                 OSP_MEM_WORD(offsetof(OspreyList, tail)))
/* An internal node: all 32 slots are child-node pointers. A leaf's slots are
 * elements, so leaves stay OSP_MEM_RAW. Internal-vs-leaf is intrinsic and
 * stable: growth only stacks new internal nodes above an existing subtree, and
 * push_tail installs the tail chunk (always a leaf) at level OSPREY_LIST_BITS,
 * so no node ever changes kind. */
#define NODE_INTERNAL_LAYOUT OSP_MEM_LAYOUT((uint64_t)0xFFFFFFFFu)
#define NODE_LEAF_LAYOUT ((int64_t)OSP_MEM_RAW)

/* A node reached at `shift` holds child pointers while shift > 0, elements at 0. */
static int64_t node_layout(int32_t shift) {
  return shift > 0 ? NODE_INTERNAL_LAYOUT : NODE_LEAF_LAYOUT;
}

static void *as_node_ptr(int64_t slot) { return (void *)(uintptr_t)slot; }

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

static OspreyListNode *alloc_node(int64_t layout) {
  OspreyListNode *n = (OspreyListNode *)calloc(1, sizeof(OspreyListNode));
  osp_mem_set_layout(n, layout);
  return n;
}

/* Copy a node. The copy shares every child `src` pointed at, so an internal
 * node's slots each gain a reference; a leaf's slots are elements and gain
 * none. NULL slots probe-miss, so the blanket loop needs no bounds. */
static OspreyListNode *clone_node(OspreyListNode *src, int64_t layout) {
  OspreyListNode *n = alloc_node(layout);
  if (src == NULL) {
    return n;
  }
  memcpy(n->slots, src->slots, sizeof(src->slots));
  if (layout == NODE_INTERNAL_LAYOUT) {
    for (int32_t i = 0; i < OSPREY_LIST_BRANCH; i++) {
      osp_retain(as_node_ptr(n->slots[i]));
    }
  }
  return n;
}

/* Wrap `node` in fresh internal nodes from `level` down to the leaf level,
 * moving the caller's reference into the innermost wrapper. */
static OspreyListNode *wrap_path(OspreyListNode *node, int32_t level) {
  for (int32_t lvl = level; lvl > 0; lvl -= OSPREY_LIST_BITS) {
    OspreyListNode *wrap = alloc_node(NODE_INTERNAL_LAYOUT);
    wrap->slots[0] = (int64_t)(uintptr_t)node;
    node = wrap;
  }
  return node;
}

/* Build a header owning `root` and `tail` — both references are MOVED in. */
static OspreyList *alloc_list(int64_t length, int32_t shift, int32_t tail_count,
                              OspreyListNode *root, OspreyListNode *tail) {
  OspreyList *l = (OspreyList *)calloc(1, sizeof(OspreyList));
  osp_mem_set_layout(l, LIST_HDR_LAYOUT);
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

/* Path-copy down to the leaf holding index `i` and write `v`. Returns the fresh
   root; every clone dups its children, so each overwritten slot drops one. */
static OspreyListNode *set_in_tree(OspreyList *l, int64_t i, int64_t v) {
  OspreyListNode *new_root = clone_node(l->root, node_layout(l->shift));
  OspreyListNode *cur = new_root;
  int32_t s = l->shift;
  while (s > 0) {
    int32_t slot = (int32_t)((i >> s) & OSPREY_LIST_MASK);
    OspreyListNode *child = (OspreyListNode *)(uintptr_t)cur->slots[slot];
    s -= OSPREY_LIST_BITS;
    OspreyListNode *new_child = clone_node(child, node_layout(s));
    osp_release(child); /* the clone above dup'd it into this slot */
    cur->slots[slot] = (int64_t)(uintptr_t)new_child;
    cur = new_child;
  }
  cur->slots[i & OSPREY_LIST_MASK] = v;
  return new_root;
}

/* Set element at index, returning a path-copied list. Index must be in
   bounds. */
OspreyList *osprey_list_set(OspreyList *l, int64_t i, int64_t v) {
  int64_t off = tail_offset(l);
  if (i >= off) {
    OspreyListNode *new_tail = clone_node(l->tail, NODE_LEAF_LAYOUT);
    new_tail->slots[i - off] = v;
    osp_retain(l->root); /* aliased into a second header */
    return alloc_list(l->length, l->shift, l->tail_count, l->root, new_tail);
  }
  OspreyListNode *new_root = set_in_tree(l, i, v);
  osp_retain(l->tail);
  return alloc_list(l->length, l->shift, l->tail_count, new_root, l->tail);
}

/* Insert the given tail chunk at position level. Path-copies as it descends.
   Returns the new node at this level (+1); `tail_node` is BORROWED and gains
   its own reference where it lands. `level` is never 0: a shift-0 list has at
   most one full chunk in the tree, so appending to it always grows the tree
   (osprey_list_append's capacity test) instead of reaching here. */
static OspreyListNode *push_tail(int32_t level, OspreyListNode *parent,
                                 OspreyListNode *tail_node,
                                 int64_t in_tree_count) {
  int32_t sub_index =
      (int32_t)(((in_tree_count - 1) >> level) & OSPREY_LIST_MASK);
  OspreyListNode *ret = clone_node(parent, NODE_INTERNAL_LAYOUT);
  OspreyListNode *child = (OspreyListNode *)(uintptr_t)ret->slots[sub_index];
  OspreyListNode *node_to_insert;
  if (level == OSPREY_LIST_BITS) {
    osp_retain(tail_node);
    node_to_insert = tail_node;
  } else if (child != NULL) {
    node_to_insert =
        push_tail(level - OSPREY_LIST_BITS, child, tail_node, in_tree_count);
  } else {
    osp_retain(tail_node);
    node_to_insert = wrap_path(tail_node, level - OSPREY_LIST_BITS);
  }
  osp_release(child); /* clone_node dup'd it; this slot now holds the copy */
  ret->slots[sub_index] = (int64_t)(uintptr_t)node_to_insert;
  return ret;
}

/* `l`'s root with its full tail chunk folded in, as a fresh +1; *shift is
   updated when the tree gains a level. `l` keeps its own references. */
static OspreyListNode *grow_root(OspreyList *l, int32_t *shift) {
  int64_t in_tree = tail_offset(l);
  int64_t capacity_at_shift = (int64_t)1 << (l->shift + OSPREY_LIST_BITS);
  if (l->root == NULL) {
    *shift = 0;
    osp_retain(l->tail);
    return l->tail;
  }
  if (in_tree + OSPREY_LIST_BRANCH <= capacity_at_shift) {
    return push_tail(l->shift, l->root, l->tail, in_tree + OSPREY_LIST_BRANCH);
  }
  osp_retain(l->tail);
  OspreyListNode *new_path = wrap_path(l->tail, l->shift);
  OspreyListNode *new_root = alloc_node(NODE_INTERNAL_LAYOUT);
  osp_retain(l->root);
  new_root->slots[0] = (int64_t)(uintptr_t)l->root;
  new_root->slots[1] = (int64_t)(uintptr_t)new_path;
  *shift = l->shift + OSPREY_LIST_BITS;
  return new_root;
}

/* A fresh single-element leaf chunk. */
static OspreyListNode *leaf_of(int64_t v) {
  OspreyListNode *n = alloc_node(NODE_LEAF_LAYOUT);
  n->slots[0] = v;
  return n;
}

OspreyList *osprey_list_append(OspreyList *l, int64_t v) {
  if (l == NULL || l->length == 0) {
    return alloc_list(1, 0, 1, NULL, leaf_of(v));
  }
  /* Room in the tail? */
  if (l->tail_count < OSPREY_LIST_BRANCH) {
    OspreyListNode *new_tail = clone_node(l->tail, NODE_LEAF_LAYOUT);
    new_tail->slots[l->tail_count] = v;
    osp_retain(l->root);
    return alloc_list(l->length + 1, l->shift, l->tail_count + 1, l->root,
                      new_tail);
  }
  /* Tail is full. Push it into the tree; new tail = [v]. */
  int32_t new_shift = l->shift;
  OspreyListNode *new_root = grow_root(l, &new_shift);
  return alloc_list(l->length + 1, new_shift, 1, new_root, leaf_of(v));
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

/* Stack a fresh root over the builder's root and its full tail chunk. Unlike
   grow_root this MOVES both of the builder's references into the new node. */
static void builder_grow_root(OspreyListBuilder *b) {
  OspreyListNode *new_path = wrap_path(b->tail, b->shift);
  OspreyListNode *new_root = alloc_node(NODE_INTERNAL_LAYOUT);
  new_root->slots[0] = (int64_t)(uintptr_t)b->root;
  new_root->slots[1] = (int64_t)(uintptr_t)new_path;
  b->root = new_root;
  b->shift += OSPREY_LIST_BITS;
}

/* The builder is a transient it alone owns, so each branch MOVES its root and
   tail references rather than duplicating them — except the path-copying one,
   which leaves the old root unreachable and takes its own tail reference. */
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

void osprey_list_builder_push(OspreyListBuilder *b, int64_t v) {
  if (b->tail == NULL) {
    b->tail = alloc_node(NODE_LEAF_LAYOUT);
    b->tail_count = 0;
  }
  if (b->tail_count == OSPREY_LIST_BRANCH) {
    builder_push_tail_to_tree(b);
    b->tail = alloc_node(NODE_LEAF_LAYOUT);
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
