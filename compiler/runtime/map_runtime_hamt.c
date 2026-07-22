#include "map_runtime_internal.h"

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

/*
 * HAMT node algebra: hashing, key equality, assoc, lookup, remove.
 * Public API and iterator live in map_runtime.c. Split per CLAUDE.md
 * 500-LoC limit.
 *
 * Implements [TYPE-MAP] node-level invariants from
 * docs/specs/0004-TypeSystem.md.
 */

static uint32_t hash_int(int64_t key) {
  uint64_t k = (uint64_t)key;
  k = (k ^ (k >> 30)) * (uint64_t)0xbf58476d1ce4e5b5ULL;
  k = (k ^ (k >> 27)) * (uint64_t)0x94d049bb133111ebULL;
  k = k ^ (k >> 31);
  return (uint32_t)(k & 0xffffffffu);
}

static uint32_t hash_bool(int64_t key) { return (key != 0) ? 1u : 0u; }

static uint32_t hash_string(int64_t key) {
  const char *s = (const char *)(uintptr_t)key;
  if (s == NULL) {
    return 0u;
  }
  uint32_t h = 0x811c9dc5u;
  while (*s != '\0') {
    h ^= (uint32_t)(unsigned char)*s;
    h *= 0x01000193u;
    s++;
  }
  return h;
}

uint32_t osprey_map_hash_key(OspreyKeyType kt, int64_t key) {
  switch (kt) {
  case OSPREY_KEY_INT:
    return hash_int(key);
  case OSPREY_KEY_BOOL:
    return hash_bool(key);
  case OSPREY_KEY_STRING:
    return hash_string(key);
  default:
    return 0u;
  }
}

int osprey_map_keys_equal(OspreyKeyType kt, int64_t a, int64_t b) {
  if (kt == OSPREY_KEY_STRING) {
    const char *sa = (const char *)(uintptr_t)a;
    const char *sb = (const char *)(uintptr_t)b;
    if (sa == sb) {
      return 1;
    }
    if (sa == NULL || sb == NULL) {
      return 0;
    }
    return (strcmp(sa, sb) == 0) ? 1 : 0;
  }
  return (a == b) ? 1 : 0;
}

static uint32_t bit_for(uint32_t hash, int32_t shift) {
  return 1u << ((hash >> shift) & (uint32_t)OSPREY_LIST_MASK);
}

static uint32_t index_for(uint32_t bitmap, uint32_t bit) {
  return (uint32_t)__builtin_popcount(bitmap & (bit - 1u));
}

/* Every node and every out-of-line array below is stamped with its layout so a
 * release walks exactly what it owns — skeleton plus the element slots the
 * policy says are managed (map_node_layout in map_runtime_internal.h).
 * [GC-ARC-PERCEUS] */
static OspreyMapNode *alloc_node(OspreyMapNodeKind kind, uint32_t h,
                                 OspreyMapPolicy pol) {
  OspreyMapNode *n = (OspreyMapNode *)calloc(1, sizeof(OspreyMapNode));
  osp_mem_set_layout(n, map_node_layout(pol));
  n->kind = kind;
  n->hash = h;
  return n;
}

static OspreyMapNode **alloc_kids(size_t slots) {
  OspreyMapNode **k =
      (OspreyMapNode **)calloc(slots, sizeof(OspreyMapNode *));
  osp_mem_set_layout(k, OSP_MEM_PTR_ARRAY);
  return k;
}

/* A managed element array is walked when its owning node dies; a scalar one is
 * freed with the node and never read as pointers. */
static int64_t *alloc_slots(size_t n, int managed) {
  int64_t *a = (int64_t *)calloc(n, sizeof(int64_t));
  if (managed) {
    osp_mem_set_layout(a, OSP_MEM_PTR_ARRAY);
  }
  return a;
}

/* Dup every slot a copied element array now shares with its source. */
static void retain_slots(const int64_t *a, uint32_t count, int managed) {
  if (!managed) {
    return;
  }
  for (uint32_t i = 0; i < count; i++) {
    osp_retain((void *)(uintptr_t)a[i]);
  }
}

static void release_slot(int64_t v, int managed) {
  if (managed) {
    osp_release((void *)(uintptr_t)v);
  }
}

/* TRANSFERS `key` and `value`. */
static OspreyMapNode *make_leaf(uint32_t h, int64_t key, int64_t value,
                                OspreyMapPolicy pol) {
  OspreyMapNode *n = alloc_node(NODE_LEAF, h, pol);
  n->leaf_key = key;
  n->leaf_value = value;
  return n;
}

static OspreyMapNode *make_internal(uint32_t bitmap, uint32_t count,
                                    OspreyMapNode **children,
                                    OspreyMapPolicy pol) {
  OspreyMapNode *n = alloc_node(NODE_INTERNAL, 0u, pol);
  n->bitmap = bitmap;
  n->count = count;
  n->children = children;
  return n;
}

static OspreyMapNode *make_collision(uint32_t h, uint32_t count, int64_t *keys,
                                     int64_t *values, OspreyMapPolicy pol) {
  OspreyMapNode *n = alloc_node(NODE_COLLISION, h, pol);
  n->count = count;
  n->coll_keys = keys;
  n->coll_values = values;
  return n;
}

/* Dup the `count` children a path-copied array shares with `src`: both arrays
 * now reference them, so both must count. */
static void retain_kids(OspreyMapNode **src, uint32_t count) {
  for (uint32_t i = 0; i < count; i++) {
    osp_retain(src[i]);
  }
}

static OspreyMapNode **clone_children(OspreyMapNode **src, uint32_t count) {
  OspreyMapNode **out = alloc_kids((size_t)count + 1);
  if (src != NULL) {
    memcpy(out, src, (size_t)count * sizeof(OspreyMapNode *));
    retain_kids(out, count);
  }
  return out;
}

/* Fold `a` (leaf or collision) and leaf `b` into one collision node at the
 * bottom of the hash. Both are BORROWED, so every element slot copied out of
 * them gains a reference. */
static OspreyMapNode *collide(OspreyMapNode *a, OspreyMapNode *b,
                              OspreyMapPolicy pol) {
  int km = map_key_is_managed(pol);
  int vm = map_value_is_managed(pol);
  uint32_t base = (a->kind == NODE_COLLISION) ? a->count : 1u;
  int64_t *ks = alloc_slots((size_t)base + 1u, km);
  int64_t *vs = alloc_slots((size_t)base + 1u, vm);
  if (a->kind == NODE_COLLISION) {
    memcpy(ks, a->coll_keys, (size_t)base * sizeof(int64_t));
    memcpy(vs, a->coll_values, (size_t)base * sizeof(int64_t));
  } else {
    ks[0] = a->leaf_key;
    vs[0] = a->leaf_value;
  }
  ks[base] = b->leaf_key;
  vs[base] = b->leaf_value;
  retain_slots(ks, base + 1u, km);
  retain_slots(vs, base + 1u, vm);
  return make_collision(a->hash, base + 1u, ks, vs, pol);
}

/* Borrows BOTH `a` and `b` — it dups whichever it stores, so each caller keeps
 * its own reference and releases the transient leaf itself. */
static OspreyMapNode *merge_leaves(OspreyMapNode *a, OspreyMapNode *b,
                                   int32_t shift, OspreyMapPolicy pol) {
  if (shift >= MAP_HASH_BITS) {
    return collide(a, b, pol);
  }
  uint32_t bit_a = bit_for(a->hash, shift);
  uint32_t bit_b = bit_for(b->hash, shift);
  if (bit_a == bit_b) {
    OspreyMapNode **kids = alloc_kids(1);
    kids[0] = merge_leaves(a, b, shift + OSPREY_MAP_BITS, pol);
    return make_internal(bit_a, 1u, kids, pol);
  }
  OspreyMapNode **kids = alloc_kids(2);
  uint32_t idx_a = index_for(bit_a | bit_b, bit_a);
  uint32_t idx_b = index_for(bit_a | bit_b, bit_b);
  kids[idx_a] = a;
  kids[idx_b] = b;
  osp_retain(a);
  osp_retain(b);
  return make_internal(bit_a | bit_b, 2u, kids, pol);
}

/* Split borrowed `node` and a fresh leaf into a shared subtree. merge_leaves
 * borrows the leaf, so it dies here unless the new tree dup'd it — and with it
 * the key/value references it did not hand on. */
static OspreyMapNode *assoc_split(OspreyMapNode *node, int32_t shift,
                                  uint32_t hash, int64_t key, int64_t value,
                                  OspreyMapPolicy pol) {
  OspreyMapNode *leaf = make_leaf(hash, key, value, pol);
  OspreyMapNode *merged = merge_leaves(node, leaf, shift, pol);
  osp_release(leaf);
  return merged;
}

/* Path-copy a collision node with `key`/`value` (TRANSFERRED) written in:
 * replacing the matching entry, or appended when there is none. Every slot
 * copied out of `node` is dup'd; a replaced pair drops the copy's dup. */
static OspreyMapNode *assoc_collision(OspreyMapNode *node, uint32_t hash,
                                      int64_t key, int64_t value,
                                      OspreyMapPolicy pol, int *grew) {
  int km = map_key_is_managed(pol);
  int vm = map_value_is_managed(pol);
  uint32_t at = node->count;
  for (uint32_t i = 0; i < node->count; i++) {
    if (osprey_map_keys_equal(map_key_type(pol), node->coll_keys[i], key)) {
      at = i;
      break;
    }
  }
  *grew = (at == node->count) ? 1 : 0;
  uint32_t count = node->count + (uint32_t)*grew;
  int64_t *ks = alloc_slots((size_t)count, km);
  int64_t *vs = alloc_slots((size_t)count, vm);
  memcpy(ks, node->coll_keys, (size_t)node->count * sizeof(int64_t));
  memcpy(vs, node->coll_values, (size_t)node->count * sizeof(int64_t));
  retain_slots(ks, node->count, km);
  retain_slots(vs, node->count, vm);
  if (!*grew) {
    release_slot(ks[at], km);
    release_slot(vs[at], vm);
  }
  ks[at] = key;
  vs[at] = value;
  return make_collision(hash, count, ks, vs, pol);
}

OspreyMapNode *osprey_map_node_assoc(OspreyMapNode *node, int32_t shift,
                                     uint32_t hash, int64_t key, int64_t value,
                                     OspreyMapPolicy pol, int *grew) {
  if (node == NULL) {
    *grew = 1;
    return make_leaf(hash, key, value, pol);
  }
  if (node->kind == NODE_LEAF) {
    if (node->hash == hash &&
        osprey_map_keys_equal(map_key_type(pol), node->leaf_key, key)) {
      *grew = 0;
      return make_leaf(hash, key, value, pol);
    }
    *grew = 1;
    return assoc_split(node, shift, hash, key, value, pol);
  }
  if (node->kind == NODE_COLLISION) {
    if (node->hash == hash) {
      return assoc_collision(node, hash, key, value, pol, grew);
    }
    *grew = 1;
    return assoc_split(node, shift, hash, key, value, pol);
  }
  uint32_t bit = bit_for(hash, shift);
  uint32_t idx = index_for(node->bitmap, bit);
  if ((node->bitmap & bit) != 0u) {
    OspreyMapNode *child = node->children[idx];
    OspreyMapNode *new_child = osprey_map_node_assoc(
        child, shift + OSPREY_MAP_BITS, hash, key, value, pol, grew);
    OspreyMapNode **new_kids = clone_children(node->children, node->count);
    osp_release(new_kids[idx]); /* the clone's dup on the replaced child */
    new_kids[idx] = new_child;  /* transfer the +1 from the recursive call */
    return make_internal(node->bitmap, node->count, new_kids, pol);
  }
  *grew = 1;
  OspreyMapNode **new_kids = alloc_kids((size_t)node->count + 1u);
  memcpy(new_kids, node->children, (size_t)idx * sizeof(OspreyMapNode *));
  new_kids[idx] = make_leaf(hash, key, value, pol);
  memcpy(new_kids + idx + 1, node->children + idx,
         (size_t)(node->count - idx) * sizeof(OspreyMapNode *));
  retain_kids(node->children, node->count);
  return make_internal(node->bitmap | bit, node->count + 1u, new_kids, pol);
}

int osprey_map_node_lookup(OspreyMapNode *node, int32_t shift, uint32_t hash,
                           int64_t key, OspreyKeyType kt, int64_t *out) {
  while (node != NULL) {
    if (node->kind == NODE_LEAF) {
      if (node->hash == hash &&
          osprey_map_keys_equal(kt, node->leaf_key, key)) {
        *out = node->leaf_value;
        return 1;
      }
      return 0;
    }
    if (node->kind == NODE_COLLISION) {
      if (node->hash != hash) {
        return 0;
      }
      for (uint32_t i = 0; i < node->count; i++) {
        if (osprey_map_keys_equal(kt, node->coll_keys[i], key)) {
          *out = node->coll_values[i];
          return 1;
        }
      }
      return 0;
    }
    uint32_t bit = bit_for(hash, shift);
    if ((node->bitmap & bit) == 0u) {
      return 0;
    }
    node = node->children[index_for(node->bitmap, bit)];
    shift += OSPREY_MAP_BITS;
  }
  return 0;
}

/* Unchanged subtrees are returned as-is; like every other alias-returning op
 * ([GC-ARC-PERCEUS] M4a) they carry a fresh +1 so the caller owns EVERY return
 * path uniformly and never has to know which it got. */
static OspreyMapNode *keep(OspreyMapNode *node, int *shrunk) {
  *shrunk = 0;
  osp_retain(node);
  return node;
}

/* Drop entry `at` from a collision node. `node` survives, so every element the
 * result keeps is BORROWED out of it and gains a reference. */
static OspreyMapNode *remove_from_collision(OspreyMapNode *node, uint32_t at,
                                            uint32_t hash,
                                            OspreyMapPolicy pol) {
  int km = map_key_is_managed(pol);
  int vm = map_value_is_managed(pol);
  uint32_t other = (at == 0u) ? 1u : 0u;
  if (node->count == 2u) {
    retain_slots(&node->coll_keys[other], 1u, km);
    retain_slots(&node->coll_values[other], 1u, vm);
    return make_leaf(hash, node->coll_keys[other], node->coll_values[other],
                     pol);
  }
  uint32_t new_count = node->count - 1u;
  uint32_t after = node->count - at - 1u;
  int64_t *ks = alloc_slots((size_t)new_count, km);
  int64_t *vs = alloc_slots((size_t)new_count, vm);
  memcpy(ks, node->coll_keys, (size_t)at * sizeof(int64_t));
  memcpy(vs, node->coll_values, (size_t)at * sizeof(int64_t));
  memcpy(ks + at, node->coll_keys + at + 1, (size_t)after * sizeof(int64_t));
  memcpy(vs + at, node->coll_values + at + 1, (size_t)after * sizeof(int64_t));
  retain_slots(ks, new_count, km);
  retain_slots(vs, new_count, vm);
  return make_collision(hash, new_count, ks, vs, pol);
}

OspreyMapNode *osprey_map_node_remove(OspreyMapNode *node, int32_t shift,
                                      uint32_t hash, int64_t key,
                                      OspreyMapPolicy pol, int *shrunk) {
  OspreyKeyType kt = map_key_type(pol);
  if (node == NULL) {
    *shrunk = 0;
    return NULL;
  }
  if (node->kind == NODE_LEAF) {
    if (node->hash == hash && osprey_map_keys_equal(kt, node->leaf_key, key)) {
      *shrunk = 1;
      return NULL; /* the dying node releases its own key/value */
    }
    return keep(node, shrunk);
  }
  if (node->kind == NODE_COLLISION) {
    if (node->hash != hash) {
      return keep(node, shrunk);
    }
    for (uint32_t i = 0; i < node->count; i++) {
      if (osprey_map_keys_equal(kt, node->coll_keys[i], key)) {
        *shrunk = 1;
        return remove_from_collision(node, i, hash, pol);
      }
    }
    return keep(node, shrunk);
  }
  uint32_t bit = bit_for(hash, shift);
  if ((node->bitmap & bit) == 0u) {
    return keep(node, shrunk);
  }
  uint32_t idx = index_for(node->bitmap, bit);
  OspreyMapNode *child = node->children[idx];
  OspreyMapNode *new_child = osprey_map_node_remove(
      child, shift + OSPREY_MAP_BITS, hash, key, pol, shrunk);
  if (!*shrunk) {
    osp_release(new_child); /* the child's own alias +1 — nothing changed */
    return keep(node, shrunk);
  }
  if (new_child == NULL) {
    if (node->count == 1u) {
      return NULL;
    }
    uint32_t new_count = node->count - 1u;
    OspreyMapNode **new_kids = alloc_kids((size_t)new_count);
    memcpy(new_kids, node->children, (size_t)idx * sizeof(OspreyMapNode *));
    memcpy(new_kids + idx, node->children + idx + 1,
           (size_t)(node->count - idx - 1u) * sizeof(OspreyMapNode *));
    retain_kids(new_kids, new_count);
    return make_internal(node->bitmap & ~bit, new_count, new_kids, pol);
  }
  OspreyMapNode **new_kids = clone_children(node->children, node->count);
  osp_release(new_kids[idx]); /* the clone's dup on the replaced child */
  new_kids[idx] = new_child;  /* transfer the +1 from the recursive call */
  return make_internal(node->bitmap, node->count, new_kids, pol);
}
