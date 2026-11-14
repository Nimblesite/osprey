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
 *   node. Conventions: collection_runtime.h. [GC-ARC-PERCEUS].
 *
 * Implements [TYPE-LIST], [TYPE-LIST-OPS] from docs/specs/language/0004-TypeSystem.md.
 *
 * Backs the list builtins of docs/specs/runtime/0012-Built-InFunctions.md — [BUILTIN-LIST],
 * [BUILTIN-LIST-GET], [BUILTIN-LIST-APPEND] and [BUILTIN-COLLECTION-LENGTH].
 * The builder, whole-list rebuilds (concat, reverse, sort) and the iterator are in
 * list_rebuild.c; the node layout both files share is list_trie.h.
 */

#include "list_trie.h"

static OspreyList *singleton_empty = NULL;

OspreyList *osprey_list_empty(void) {
  if (singleton_empty == NULL) {
    /* Returned from many sites and owned by many callers at once: under ARC
     * it must never be freed by any of them [GC-ARC-PERCEUS]. */
    singleton_empty = alloc_list(0, 0, 0, NULL, NULL, 0, 0, NULL);
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

/* Whether PHYSICAL index `j`, already known to lie before the tail chunk, is
   in the block `front` stands in for. */
static int in_front(OspreyList *l, int64_t j) {
  return l->front != NULL &&
         (j >> OSPREY_LIST_BITS) == (l->offset >> OSPREY_LIST_BITS);
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
  if (in_front(l, j)) {
    return l->front->slots[j & OSPREY_LIST_MASK];
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

/* Path-copy from the root down to the node at `level` on PHYSICAL index `i`'s
   path; returns that node and stores the new root (+1) in *root. Every clone
   dups what it shares, so each slot overwritten in one — child or element —
   drops a reference. NULL nodes on the path are created. */
static OspreyListNode *copy_path(OspreyList *l, int64_t i, int32_t level,
                                 OspreyListNode **root) {
  int em = l->elem_managed;
  OspreyListNode *cur = clone_node(l->root, node_layout(l->shift, em));
  *root = cur;
  for (int32_t s = l->shift; s > level;) {
    int32_t slot = (int32_t)((i >> s) & OSPREY_LIST_MASK);
    OspreyListNode *child = (OspreyListNode *)(uintptr_t)cur->slots[slot];
    s -= OSPREY_LIST_BITS;
    OspreyListNode *new_child = clone_node(child, node_layout(s, em));
    osp_release(child); /* the clone above dup'd it into this slot */
    cur->slots[slot] = (int64_t)(uintptr_t)new_child;
    cur = new_child;
  }
  return cur;
}

/* Overwrite `leaf`'s slot for PHYSICAL index `j` with a TRANSFERRED `v`; the
   leaf is a clone, so the element it dup'd there is dropped. */
static void write_slot(OspreyListNode *leaf, int64_t j, int64_t v, int em) {
  release_elem(leaf->slots[j & OSPREY_LIST_MASK], em);
  leaf->slots[j & OSPREY_LIST_MASK] = v;
}

/* A fresh single-element leaf chunk holding a TRANSFERRED `v`. */
static OspreyListNode *leaf_of(int64_t v, int em) {
  OspreyListNode *n = alloc_node(node_layout(0, em));
  n->slots[0] = v;
  return n;
}

/* `h` with a TRANSFERRED `v` written at PHYSICAL index `j`, as a fresh header
   of `length` elements starting at `offset`: the tail chunk, the front chunk
   or the tree path holding `j` is copied and the rest shared. `h` is BORROWED. */
static OspreyList *write_at(OspreyList *h, int64_t j, int64_t v, int64_t length,
                            int64_t offset) {
  int em = h->elem_managed;
  int64_t off = tail_offset(h);
  OspreyListNode *root = h->root, *tail = h->tail, *front = h->front;
  if (j >= off) {
    tail = clone_node(h->tail, node_layout(0, em));
    write_slot(tail, j - off, v, em);
  } else if (in_front(h, j)) {
    front = clone_node(h->front, node_layout(0, em));
    write_slot(front, j, v, em);
  } else {
    write_slot(copy_path(h, j, 0, &root), j, v, em);
  }
  /* Whatever was not copied is aliased into a second header. */
  if (root == h->root) osp_retain(root);
  if (tail == h->tail) osp_retain(tail);
  if (front == h->front) osp_retain(front);
  return alloc_list(length, h->shift, h->tail_count, root, tail, offset, em,
                    front);
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
  return write_at(l, l->offset + i, v, l->length, l->offset);
}

/* Open room before physical index 0 of `h`, a by-value header copy: a tree one
   level taller holds the old root in slot 1, so every element moves up by the
   old capacity and slot 0 is free; a tail-only list gains an empty leaf root.
   Returns the fresh root (+1), which `h` now borrows. */
static OspreyListNode *open_front(OspreyList *h) {
  int32_t shift = (h->root == NULL) ? 0 : h->shift + OSPREY_LIST_BITS;
  OspreyListNode *fresh = alloc_node(node_layout(shift, h->elem_managed));
  if (h->root != NULL) {
    osp_retain(h->root);
    fresh->slots[1] = (int64_t)(uintptr_t)h->root;
  }
  h->offset = (int64_t)1 << (h->shift + OSPREY_LIST_BITS);
  h->shift = shift;
  h->root = fresh;
  return fresh;
}

/* `h`'s root with its front chunk installed as the tree leaf of its block, as
   a fresh +1. */
static OspreyListNode *settle_front(OspreyList *h) {
  osp_retain(h->front);
  if (h->shift == 0) {
    return h->front;
  }
  OspreyListNode *root;
  OspreyListNode *parent = copy_path(h, h->offset, OSPREY_LIST_BITS, &root);
  int32_t slot = (int32_t)((h->offset >> OSPREY_LIST_BITS) & OSPREY_LIST_MASK);
  osp_release(as_node_ptr(parent->slots[slot])); /* the clone's dup */
  parent->slots[slot] = (int64_t)(uintptr_t)h->front;
  return root;
}

/* The tree leaf holding PHYSICAL index `j`, BORROWED; NULL where front room
   has not been filled yet. */
static OspreyListNode *tree_leaf(OspreyList *h, int64_t j) {
  OspreyListNode *node = h->root;
  for (int32_t s = h->shift; s > 0 && node != NULL; s -= OSPREY_LIST_BITS) {
    node = (OspreyListNode *)(uintptr_t)node->slots[(j >> s) & OSPREY_LIST_MASK];
  }
  return node;
}

/* Prepend into a block no chunk covers yet: the current front chunk settles
   into the tree, room opens when the list starts at physical 0, and a fresh
   front chunk over the block before the first element takes `v`. */
static OspreyList *prepend_new_front(OspreyList h, int64_t v) {
  int em = h.elem_managed;
  OspreyListNode *root = h.root; /* held at +1 from here on */
  if (h.front != NULL) root = settle_front(&h);
  else osp_retain(root);
  h.root = root;
  if (h.offset == 0) {
    OspreyListNode *fresh = open_front(&h);
    osp_release(root); /* the fresh root holds it now */
    root = fresh;
  }
  int64_t j = h.offset - 1;
  OspreyListNode *front = clone_node(tree_leaf(&h, j), node_layout(0, em));
  write_slot(front, j, v, em);
  osp_retain(h.tail);
  return alloc_list(h.length + 1, h.shift, h.tail_count, root, h.tail, j, em,
                    front);
}

/* Amortised O(1): `v` goes into the slot before the first element, in the tail
   chunk or the front chunk when one covers it, so most prepends copy a single
   leaf; one in 32 path-copies the full front chunk into the tree. A view's
   trimmed front is reused. `v` is TRANSFERRED. Because drop is an O(1) view
   too, cons-style `[head, ...tail]` rebuilding is linear. */
OspreyList *osprey_list_prepend_of(OspreyList *l, int64_t v, int em) {
  if (l == NULL || l->length == 0) {
    return alloc_list(1, 0, 1, NULL, leaf_of(v, em), 0, em, NULL);
  }
  OspreyList h = *l; /* a borrowed copy stamped with the caller's element kind */
  h.elem_managed = em;
  int64_t j = h.offset - 1;
  if (j >= 0 && (j >= tail_offset(&h) || in_front(&h, j))) {
    return write_at(&h, j, v, h.length + 1, j);
  }
  return prepend_new_front(h, v);
}

OspreyList *osprey_list_prepend(OspreyList *l, int64_t v) {
  return osprey_list_prepend_of(l, v, osprey_list_elem_managed(l));
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

/* Insertion TRANSFERS the caller's reference on `v` (codegen dups before
 * erasing it into the i64 element ABI), so nothing here retains it again.
 * [GC-ARC-PERCEUS] */
OspreyList *osprey_list_append_of(OspreyList *l, int64_t v, int em) {
  if (l == NULL || l->length == 0) {
    return alloc_list(1, 0, 1, NULL, leaf_of(v, em), 0, em, NULL);
  }
  /* Room in the tail? Slots from tail_count on are always zero (a chunk fills
     left to right), so no element is overwritten here. */
  if (l->tail_count < OSPREY_LIST_BRANCH) {
    OspreyListNode *new_tail = clone_node(l->tail, node_layout(0, em));
    new_tail->slots[l->tail_count] = v;
    osp_retain(l->root);
    osp_retain(l->front);
    return alloc_list(l->length + 1, l->shift, l->tail_count + 1, l->root,
                      new_tail, l->offset, em, l->front);
  }
  /* Tail is full. Push it into the tree; new tail = [v]. */
  int32_t new_shift = l->shift;
  OspreyListNode *new_root = grow_root(l, &new_shift);
  osp_retain(l->front);
  return alloc_list(l->length + 1, new_shift, 1, new_root, leaf_of(v, em),
                    l->offset, em, l->front);
}

OspreyList *osprey_list_append(OspreyList *l, int64_t v) {
  return osprey_list_append_of(l, v, osprey_list_elem_managed(l));
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
  /* The front chunk follows the view while its block still holds element 0. */
  OspreyListNode *front = in_front(l, l->offset + n) ? l->front : NULL;
  osp_retain(l->root);
  osp_retain(l->tail);
  osp_retain(front);
  return alloc_list(l->length - n, l->shift, l->tail_count, l->root, l->tail,
                    l->offset + n, l->elem_managed, front);
}
