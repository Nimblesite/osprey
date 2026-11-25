#ifndef OSPREY_LIST_TRIE_H
#define OSPREY_LIST_TRIE_H

/*
 * The List<T> trie's private layout and node primitives, shared by
 * list_runtime.c (the persistent operations) and list_rebuild.c (the
 * transient builder). Nothing outside those two files may include this:
 * everyone else speaks collection_runtime.h. [TYPE-LIST], [GC-ARC-PERCEUS].
 */

#include "collection_runtime.h"
#include "memory_hooks.h"
#include "memory_refused.h"

#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef struct OspreyListNode {
  int64_t slots[OSPREY_LIST_BRANCH];
} OspreyListNode;

/* ARC layout words: which 8-byte words hold managed pointers (via offsetof). */
#define LIST_HDR_LAYOUT                                                        \
  OSP_MEM_LAYOUT(OSP_MEM_WORD(offsetof(OspreyList, root)) |                    \
                 OSP_MEM_WORD(offsetof(OspreyList, tail)) |                    \
                 OSP_MEM_WORD(offsetof(OspreyList, front)))
/* All 32 slots are managed pointers: true of every internal node (children)
 * AND of a leaf whose ELEMENTS are managed — one layout word, so the clone's
 * dup loop and the drop walk serve both. Internal-vs-leaf is intrinsic: growth
 * stacks internals above a subtree, push_tail installs the tail chunk (a leaf)
 * at OSPREY_LIST_BITS — kinds never change. */
#define NODE_PTRS_LAYOUT ((int64_t)OSP_MEM_PTR_ARRAY)
/* A scalar-element leaf: type-blind int64s whose bits may collide with a live
 * heap address, so the drop walk must never touch them. */
#define NODE_SCALAR_LAYOUT ((int64_t)OSP_MEM_RAW)

/* A node at `shift` holds children while shift > 0, elements at 0 — and
 * elements are walked only when the list's element type is managed. */
static inline int64_t node_layout(int32_t shift, int em) {
  return (shift > 0 || em) ? NODE_PTRS_LAYOUT : NODE_SCALAR_LAYOUT;
}

static inline void *as_node_ptr(int64_t slot) { return (void *)(uintptr_t)slot; }

/* Element dup / drop, gated on the element kind. */
static inline void retain_elem(int64_t v, int em) {
  if (em) osp_retain(as_node_ptr(v));
}
static inline void release_elem(int64_t v, int em) {
  if (em) osp_release(as_node_ptr(v));
}

struct OspreyList {
  int64_t length;    /* LOGICAL length (elements visible through this header) */
  int32_t shift;     /* 0 == root is a leaf; 5, 10, … one level deeper each */
  int32_t tail_count;
  OspreyListNode *root; /* NULL iff in-tree count == 0 */
  OspreyListNode *tail; /* NULL iff physical count == 0 */
  /* The prepend chunk, or NULL: a leaf standing in for the tree's leaf over
   * the 32-slot block that holds logical element 0, so a prepend copies one
   * leaf, not a tree path. That block always lies before the tail chunk, and
   * the tree's own leaf there is never read while `front` is set. */
  OspreyListNode *front;
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

/* ARC can publish the layout at allocation. Fresh nodes need empty slots;
 * a copied node and a list header are initialized in full by their caller.
 * Every caller writes the block at once, so a refusal stops by name (#409). */
static inline void *alloc_trie_storage(size_t size, int64_t layout, int clear) {
#ifdef OSP_ARC_SHIM_H
  void *body = osp_alloc_checked(osp_alloc_tagged_noinit((int64_t)size, layout), "list", size);
  if (clear) memset(body, 0, size);
#else
  (void)clear;
  void *body = osp_alloc_checked(calloc(1, size), "list", size);
  osp_mem_set_layout(body, layout);
#endif
  return body;
}

static inline OspreyListNode *alloc_node(int64_t layout) {
  return alloc_trie_storage(sizeof(OspreyListNode), layout, 1);
}

/* The source owns the old slot. Copy only the other ownership edges and
 * move value into the replacement slot [MEM-OWNERSHIP-STATIC-SHARING]. */
static inline OspreyListNode *replace_slot(OspreyListNode *src, int64_t layout,
                                          int32_t used, int32_t slot, int64_t value) {
  OspreyListNode *n = alloc_trie_storage(sizeof(*n), layout, src == NULL);
  if (src != NULL) {
    memcpy(n->slots, src->slots, sizeof(src->slots));
    if (layout == NODE_PTRS_LAYOUT) {
      for (int32_t i = 0; i < used; i++) {
        if (i != slot) osp_retain(as_node_ptr(n->slots[i]));
      }
    }
  }
  n->slots[slot] = value;
  return n;
}

/* Wrap `node` in fresh internal nodes down to the leaf level, moving the
 * caller's reference into the innermost wrapper. */
static inline OspreyListNode *wrap_path(OspreyListNode *node, int32_t level) {
  for (int32_t lvl = level; lvl > 0; lvl -= OSPREY_LIST_BITS) {
    OspreyListNode *wrap = alloc_node(NODE_PTRS_LAYOUT);
    wrap->slots[0] = (int64_t)(uintptr_t)node;
    node = wrap;
  }
  return node;
}

/* Build a header owning `root`, `tail` and `front` — each reference is MOVED
   in. */
static inline OspreyList *alloc_list(int64_t length, int32_t shift, int32_t tail_count,
                              OspreyListNode *root, OspreyListNode *tail,
                              int64_t offset, int elem_managed,
                              OspreyListNode *front) {
  OspreyList *l = alloc_trie_storage(sizeof(*l), LIST_HDR_LAYOUT, 0);
  l->length = length;
  l->shift = shift;
  l->tail_count = tail_count;
  l->root = root;
  l->tail = tail;
  l->front = front;
  l->offset = offset;
  l->elem_managed = elem_managed;
  return l;
}

/* Insert the tail chunk at `level`, path-copying as it descends; returns the
   new node (+1). `tail_node` is BORROWED and gains a reference where it lands.
   `level` is never 0: appending to a shift-0 list grows the tree first. */
static inline OspreyListNode *push_tail(int32_t level, OspreyListNode *parent,
                                 OspreyListNode *tail_node,
                                 int64_t in_tree_count) {
  int32_t sub_index =
      (int32_t)(((in_tree_count - 1) >> level) & OSPREY_LIST_MASK);
  OspreyListNode *child = parent ? as_node_ptr(parent->slots[sub_index]) : NULL;
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
  return replace_slot(parent, NODE_PTRS_LAYOUT, OSPREY_LIST_BRANCH, sub_index,
                      (int64_t)(uintptr_t)node_to_insert);
}

#endif /* OSPREY_LIST_TRIE_H */
