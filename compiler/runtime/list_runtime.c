#include "collection_runtime.h"
#include "memory_hooks.h"

#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

/*
 * Immutable bitmapped vector trie for List<T>. Reference design: Clojure
 * PersistentVector (Phil Bagwell, "Ideal Hash Trees", 2000).
 *
 * - 32-way branching (5 bits per level).
 * - "Tail" optimisation: the last partial 32-element chunk lives outside the
 *   tree, giving amortised O(1) append and O(1) length.
 * - Memory: reference-counted per node AND per element under the ARC backend,
 *   so a dead list reclaims its skeleton and drops the elements it owned while
 *   every version still sharing a node keeps it alive; a no-op under the
 *   default/tracing backends. Path-copying still never mutates a published
 *   node. Conventions: collection_runtime.h. [GC-ARC-PERCEUS] plan 0011 M4b.
 *
 * Implements [TYPE-LIST], [TYPE-LIST-OPS] from docs/specs/0004-TypeSystem.md.
 *
 * Backs the list builtins of docs/specs/0012-Built-InFunctions.md — [BUILTIN-LIST],
 * [BUILTIN-LIST-GET], [BUILTIN-LIST-APPEND], [BUILTIN-LIST-PREPEND],
 * [BUILTIN-LIST-CONCAT], [BUILTIN-LIST-REVERSE] and [BUILTIN-COLLECTION-LENGTH]
 * — which ship under their `listXxx` spellings, not the bare spec names. The
 * iterator at the bottom of this file also carries the linear scan codegen
 * emits for [BUILTIN-LIST-CONTAINS]. [BUILTIN-LIST-HEAD], [BUILTIN-LIST-TAIL]
 * and [BUILTIN-LIST-INDEXOF] have no implementation here or anywhere.
 */

typedef struct OspreyListNode {
  int64_t slots[OSPREY_LIST_BRANCH];
} OspreyListNode;

/* ARC layout words: which 8-byte words hold managed pointers (via offsetof). */
#define LIST_HDR_LAYOUT                                                        \
  OSP_MEM_LAYOUT(OSP_MEM_WORD(offsetof(OspreyList, root)) |                    \
                 OSP_MEM_WORD(offsetof(OspreyList, tail)))
/* All 32 slots are managed pointers: true of every internal node (children)
 * AND of a leaf whose ELEMENTS are managed — one layout word, so clone_node's
 * dup loop and the drop walk serve both. Internal-vs-leaf is intrinsic: growth
 * stacks internals above a subtree, push_tail installs the tail chunk (a leaf)
 * at OSPREY_LIST_BITS — kinds never change. */
#define NODE_PTRS_LAYOUT OSP_MEM_LAYOUT((uint64_t)0xFFFFFFFFu)
/* A scalar-element leaf: type-blind int64s whose bits may collide with a live
 * heap address, so the drop walk must never touch them. */
#define NODE_SCALAR_LAYOUT ((int64_t)OSP_MEM_RAW)

/* A node at `shift` holds children while shift > 0, elements at 0 — and
 * elements are walked only when the list's element type is managed. */
static int64_t node_layout(int32_t shift, int em) {
  return (shift > 0 || em) ? NODE_PTRS_LAYOUT : NODE_SCALAR_LAYOUT;
}

static void *as_node_ptr(int64_t slot) { return (void *)(uintptr_t)slot; }

/* Element dup / drop, gated on the element kind. */
static void retain_elem(int64_t v, int em) {
  if (em) osp_retain(as_node_ptr(v));
}
static void release_elem(int64_t v, int em) {
  if (em) osp_release(as_node_ptr(v));
}

struct OspreyList {
  int64_t length;    /* LOGICAL length (elements visible through this header) */
  int32_t shift;     /* 0 == root is a leaf; 5, 10, … one level deeper each */
  int32_t tail_count;
  OspreyListNode *root; /* NULL iff in-tree count == 0 */
  OspreyListNode *tail; /* NULL iff physical count == 0 */
  /* Physical index of logical element 0. osprey_list_drop returns an O(1)
   * VIEW — a fresh header sharing root/tail with offset bumped — so
   * `[head, ...tail]` recursion allocates n headers, not n²/2 nodes. Invariant:
   * offset + length == the physical count (views only trim the FRONT), so the
   * tail always ends at the logical end and append stays valid on a view. */
  int64_t offset;
  /* 1 when the element slots hold managed pointers, 0 for scalars. */
  int32_t elem_managed;
};

struct OspreyListBuilder {
  int64_t length;
  int32_t shift;
  int32_t tail_count;
  OspreyListNode *root;
  OspreyListNode *tail;
  int32_t elem_managed;
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

/* Copy a node. The copy shares every slot `src` pointed at, so each managed
 * slot gains a reference — children for an internal node, elements for a
 * managed leaf; a scalar leaf none. NULL slots probe-miss. */
static OspreyListNode *clone_node(OspreyListNode *src, int64_t layout) {
  OspreyListNode *n = alloc_node(layout);
  if (src == NULL) {
    return n;
  }
  memcpy(n->slots, src->slots, sizeof(src->slots));
  if (layout == NODE_PTRS_LAYOUT) {
    for (int32_t i = 0; i < OSPREY_LIST_BRANCH; i++) {
      osp_retain(as_node_ptr(n->slots[i]));
    }
  }
  return n;
}

/* Wrap `node` in fresh internal nodes down to the leaf level, moving the
 * caller's reference into the innermost wrapper. */
static OspreyListNode *wrap_path(OspreyListNode *node, int32_t level) {
  for (int32_t lvl = level; lvl > 0; lvl -= OSPREY_LIST_BITS) {
    OspreyListNode *wrap = alloc_node(NODE_PTRS_LAYOUT);
    wrap->slots[0] = (int64_t)(uintptr_t)node;
    node = wrap;
  }
  return node;
}

/* Build a header owning `root` and `tail` — both references are MOVED in. */
static OspreyList *alloc_list(int64_t length, int32_t shift, int32_t tail_count,
                              OspreyListNode *root, OspreyListNode *tail,
                              int64_t offset, int elem_managed) {
  OspreyList *l = (OspreyList *)calloc(1, sizeof(OspreyList));
  osp_mem_set_layout(l, LIST_HDR_LAYOUT);
  l->length = length;
  l->shift = shift;
  l->tail_count = tail_count;
  l->root = root;
  l->tail = tail;
  l->offset = offset;
  l->elem_managed = elem_managed;
  return l;
}

static OspreyList *singleton_empty = NULL;

OspreyList *osprey_list_empty(void) {
  if (singleton_empty == NULL) {
    /* Returned from many sites and owned by many callers at once: under ARC
     * it must never be freed by any of them [GC-ARC-PERCEUS] plan 0011 M4. */
    singleton_empty = alloc_list(0, 0, 0, NULL, NULL, 0, 0);
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

int osprey_list_elem_managed(OspreyList *l) {
  return (l == NULL) ? 0 : l->elem_managed;
}

int osprey_list_in_bounds(OspreyList *l, int64_t i) {
  if (l == NULL) {
    return 0;
  }
  return (i >= 0 && i < l->length) ? 1 : 0;
}

/* PHYSICAL index where the tail chunk starts (all tree arithmetic below is
   physical; only the public API speaks logical indices). */
static int64_t tail_offset(OspreyList *l) {
  return l->offset + l->length - (int64_t)l->tail_count;
}

/* Extraction hands back a BORROWED element: the list keeps its reference, so
 * every reader must keep the source alive and dup before storing it elsewhere.
 * Out-of-range indices return 0 rather than walking the trie with garbage
 * shifts (codegen's paired in-bounds call picks Error). */
int64_t osprey_list_get(OspreyList *l, int64_t i) {
  if (l == NULL || i < 0 || i >= l->length) {
    return 0;
  }
  int64_t j = l->offset + i;
  int64_t off = tail_offset(l);
  if (j >= off) {
    return l->tail->slots[j - off];
  }
  OspreyListNode *node = l->root;
  int32_t s = l->shift;
  while (s > 0) {
    int32_t slot = (int32_t)((j >> s) & OSPREY_LIST_MASK);
    node = (OspreyListNode *)(uintptr_t)node->slots[slot];
    s -= OSPREY_LIST_BITS;
  }
  return node->slots[j & OSPREY_LIST_MASK];
}

/* Path-copy down to the leaf holding PHYSICAL index `i` and write a
   TRANSFERRED `v`. Every clone dups what it shares, so each overwritten slot
   — child or element — drops one. */
static OspreyListNode *set_in_tree(OspreyList *l, int64_t i, int64_t v) {
  int em = l->elem_managed;
  OspreyListNode *new_root = clone_node(l->root, node_layout(l->shift, em));
  OspreyListNode *cur = new_root;
  int32_t s = l->shift;
  while (s > 0) {
    int32_t slot = (int32_t)((i >> s) & OSPREY_LIST_MASK);
    OspreyListNode *child = (OspreyListNode *)(uintptr_t)cur->slots[slot];
    s -= OSPREY_LIST_BITS;
    OspreyListNode *new_child = clone_node(child, node_layout(s, em));
    osp_release(child); /* the clone above dup'd it into this slot */
    cur->slots[slot] = (int64_t)(uintptr_t)new_child;
    cur = new_child;
  }
  release_elem(cur->slots[i & OSPREY_LIST_MASK], em); /* the clone's dup */
  cur->slots[i & OSPREY_LIST_MASK] = v;
  return new_root;
}

/* Set element at an in-bounds index, path-copied; `v` is TRANSFERRED. An
   out-of-contract index degrades like the hardened osprey_list_get instead of
   subscripting past a 32-slot node: the list comes back unchanged (+1, the
   alias-return convention) and the transferred element is dropped. */
OspreyList *osprey_list_set(OspreyList *l, int64_t i, int64_t v) {
  if (l == NULL || i < 0 || i >= l->length) {
    release_elem(v, osprey_list_elem_managed(l));
    osp_retain(l);
    return l;
  }
  int em = l->elem_managed;
  int64_t j = l->offset + i;
  int64_t off = tail_offset(l);
  if (j >= off) {
    OspreyListNode *new_tail = clone_node(l->tail, node_layout(0, em));
    release_elem(new_tail->slots[j - off], em);
    new_tail->slots[j - off] = v;
    osp_retain(l->root); /* aliased into a second header */
    return alloc_list(l->length, l->shift, l->tail_count, l->root, new_tail,
                      l->offset, em);
  }
  OspreyListNode *new_root = set_in_tree(l, j, v);
  osp_retain(l->tail);
  return alloc_list(l->length, l->shift, l->tail_count, new_root, l->tail,
                    l->offset, em);
}

/* Insert the tail chunk at `level`, path-copying as it descends; returns the
   new node (+1). `tail_node` is BORROWED and gains a reference where it lands.
   `level` is never 0: appending to a shift-0 list grows the tree first. */
static OspreyListNode *push_tail(int32_t level, OspreyListNode *parent,
                                 OspreyListNode *tail_node,
                                 int64_t in_tree_count) {
  int32_t sub_index =
      (int32_t)(((in_tree_count - 1) >> level) & OSPREY_LIST_MASK);
  OspreyListNode *ret = clone_node(parent, NODE_PTRS_LAYOUT);
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

/* `l`'s root with its full tail chunk folded in, as a fresh +1; *shift grows
   when the tree gains a level. `l` keeps its own references. */
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
  OspreyListNode *new_root = alloc_node(NODE_PTRS_LAYOUT);
  osp_retain(l->root);
  new_root->slots[0] = (int64_t)(uintptr_t)l->root;
  new_root->slots[1] = (int64_t)(uintptr_t)new_path;
  *shift = l->shift + OSPREY_LIST_BITS;
  return new_root;
}

/* A fresh single-element leaf chunk holding a TRANSFERRED `v`. */
static OspreyListNode *leaf_of(int64_t v, int em) {
  OspreyListNode *n = alloc_node(node_layout(0, em));
  n->slots[0] = v;
  return n;
}

/* Insertion TRANSFERS the caller's reference on `v` (codegen dups before
 * erasing it into the i64 element ABI), so nothing here retains it again.
 * [GC-ARC-PERCEUS] */
OspreyList *osprey_list_append_of(OspreyList *l, int64_t v, int em) {
  if (l == NULL || l->length == 0) {
    return alloc_list(1, 0, 1, NULL, leaf_of(v, em), 0, em);
  }
  /* Room in the tail? Slots from tail_count on are always zero (a chunk fills
     left to right), so no element is overwritten here. */
  if (l->tail_count < OSPREY_LIST_BRANCH) {
    OspreyListNode *new_tail = clone_node(l->tail, node_layout(0, em));
    new_tail->slots[l->tail_count] = v;
    osp_retain(l->root);
    return alloc_list(l->length + 1, l->shift, l->tail_count + 1, l->root,
                      new_tail, l->offset, em);
  }
  /* Tail is full. Push it into the tree; new tail = [v]. */
  int32_t new_shift = l->shift;
  OspreyListNode *new_root = grow_root(l, &new_shift);
  return alloc_list(l->length + 1, new_shift, 1, new_root, leaf_of(v, em),
                    l->offset, em);
}

OspreyList *osprey_list_append(OspreyList *l, int64_t v) {
  return osprey_list_append_of(l, v, osprey_list_elem_managed(l));
}

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
  OspreyListBuilder *bld =
      osprey_list_builder_new_of(osprey_list_elem_managed(a));
  builder_push_all(bld, a);
  builder_push_all(bld, b);
  return osprey_list_builder_seal(bld);
}

/* Drop the first `n` elements as an O(1) VIEW: a fresh header sharing `l`'s
   whole skeleton with `offset` bumped, which makes `[head, ...tail]` recursion
   linear. Node RC (the +1s below) makes it safe: last one out frees. */
OspreyList *osprey_list_drop(OspreyList *l, int64_t n) {
  if (l == NULL || n <= 0) {
    osp_retain(l); /* alias return: +1 to the caller (see concat) */
    return (OspreyList *)l;
  }
  if (n >= l->length) {
    return osprey_list_empty();
  }
  osp_retain(l->root);
  osp_retain(l->tail);
  return alloc_list(l->length - n, l->shift, l->tail_count, l->root, l->tail,
                    l->offset + n, l->elem_managed);
}

OspreyList *osprey_list_reverse(OspreyList *l) {
  OspreyListBuilder *b =
      osprey_list_builder_new_of(osprey_list_elem_managed(l));
  for (int64_t i = osprey_list_length(l) - 1; i >= 0; i--) {
    osprey_list_builder_push_borrowed(b, osprey_list_get(l, i));
  }
  return osprey_list_builder_seal(b);
}

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
                             b->tail, 0, b->elem_managed);
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
