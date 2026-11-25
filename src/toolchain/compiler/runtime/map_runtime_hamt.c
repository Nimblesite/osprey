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
 * docs/specs/language/0004-TypeSystem.md.
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

/* Every node is stamped with its owned slots. Internal children live inline,
 * so a path copy needs one allocation and ARC walks those slots directly.
 * [GC-ARC-PERCEUS] */
static OspreyMapNode *alloc_node(OspreyMapNodeKind kind, uint32_t h,
                                 uint32_t count, OspreyMapPolicy pol) {
  size_t bytes = sizeof(OspreyMapNode) +
                 (kind == NODE_INTERNAL ? (size_t)count * sizeof(OspreyMapNode *) : 0);
  OspreyMapNode *n = (OspreyMapNode *)osp_alloc_tagged_noinit(
      (int64_t)bytes, map_node_layout(pol, kind, count));
  memset(n, 0, bytes);
  n->kind = kind;
  n->hash = h;
  if (kind == NODE_INTERNAL) {
    n->children = (OspreyMapNode **)(n + 1);
  }
  return n;
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

/* TRANSFERS `key` and `value`. */
static OspreyMapNode *make_leaf(uint32_t h, int64_t key, int64_t value,
                                OspreyMapPolicy pol) {
  OspreyMapNode *n = alloc_node(NODE_LEAF, h, 0, pol);
  n->leaf_key = key;
  n->leaf_value = value;
  return n;
}

static OspreyMapNode *make_internal(uint32_t bitmap, uint32_t count,
                                    OspreyMapPolicy pol) {
  OspreyMapNode *n = alloc_node(NODE_INTERNAL, 0u, count, pol);
  n->bitmap = bitmap;
  n->count = count;
  return n;
}

static OspreyMapNode *make_collision(uint32_t h, uint32_t count, int64_t *keys,
                                     int64_t *values, OspreyMapPolicy pol) {
  OspreyMapNode *n = alloc_node(NODE_COLLISION, h, 0, pol);
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

static OspreyMapNode *copy_internal_except(OspreyMapNode *src, uint32_t idx,
                                            OspreyMapPolicy pol) {
  OspreyMapNode *out = make_internal(src->bitmap, src->count, pol);
  memcpy(out->children, src->children,
         (size_t)src->count * sizeof(OspreyMapNode *));
  for (uint32_t i = 0; i < src->count; i++) {
    if (i != idx) {
      osp_retain(out->children[i]);
    }
  }
  return out;
}

/* The inserted word is transferred. Only words surviving from the source
 * acquire a reference [MEM-OWNERSHIP-STATIC-SHARING]. at==count appends. */
static int64_t *collision_words(const int64_t *src, uint32_t count, uint32_t at,
                                int64_t value, int managed) {
  int64_t *out = alloc_slots((size_t)count + (at == count), managed);
  memcpy(out, src, (size_t)count * sizeof(*out));
  retain_slots(out, at, managed);
  if (at < count) retain_slots(out + at + 1, count - at - 1, managed);
  out[at] = value;
  return out;
}

static OspreyMapNode *collision_with(OspreyMapNode *node, uint32_t at, int64_t key,
                                     int64_t value, OspreyMapPolicy pol) {
  int leaf = node->kind == NODE_LEAF;
  uint32_t count = leaf ? 1u : node->count;
  int64_t *keys = collision_words(leaf ? &node->leaf_key : node->coll_keys,
                                  count, at, key, map_key_is_managed(pol));
  int64_t *values = collision_words(leaf ? &node->leaf_value : node->coll_values,
                                    count, at, value, map_value_is_managed(pol));
  return make_collision(node->hash, count + (at == count), keys, values, pol);
}

/* Borrow node; move the incoming key and value directly to their destination. */
static OspreyMapNode *assoc_split(OspreyMapNode *node, int32_t shift, uint32_t hash,
                                  int64_t key, int64_t value, OspreyMapPolicy pol) {
  if (shift >= MAP_HASH_BITS)
    return collision_with(node, node->kind == NODE_LEAF ? 1u : node->count, key, value, pol);
  uint32_t old_bit = bit_for(node->hash, shift);
  uint32_t new_bit = bit_for(hash, shift);
  if (old_bit == new_bit) {
    OspreyMapNode *out = make_internal(old_bit, 1u, pol);
    out->children[0] = assoc_split(node, shift + OSPREY_MAP_BITS, hash, key, value, pol);
    return out;
  }
  uint32_t bitmap = old_bit | new_bit;
  OspreyMapNode *out = make_internal(bitmap, 2u, pol);
  osp_retain(node);
  out->children[index_for(bitmap, old_bit)] = node;
  out->children[index_for(bitmap, new_bit)] = make_leaf(hash, key, value, pol);
  return out;
}

static OspreyMapNode *assoc_collision(OspreyMapNode *node,
                                      int64_t key, int64_t value,
                                      OspreyMapPolicy pol, int *grew) {
  uint32_t at = node->count;
  for (uint32_t i = 0; i < node->count; i++) {
    if (osprey_map_keys_equal(map_key_type(pol), node->coll_keys[i], key)) {
      at = i;
      break;
    }
  }
  *grew = (at == node->count) ? 1 : 0;
  return collision_with(node, at, key, value, pol);
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
      return assoc_collision(node, key, value, pol, grew);
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
    OspreyMapNode *out = copy_internal_except(node, idx, pol);
    out->children[idx] = new_child;
    return out;
  }
  *grew = 1;
  OspreyMapNode *out = make_internal(node->bitmap | bit, node->count + 1u, pol);
  memcpy(out->children, node->children, (size_t)idx * sizeof(OspreyMapNode *));
  out->children[idx] = make_leaf(hash, key, value, pol);
  memcpy(out->children + idx + 1, node->children + idx,
         (size_t)(node->count - idx) * sizeof(OspreyMapNode *));
  retain_kids(node->children, node->count);
  return out;
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
  *shrunk = 0;
  if (node == NULL) {
    return NULL;
  }
  if (node->kind == NODE_LEAF) {
    if (node->hash == hash && osprey_map_keys_equal(kt, node->leaf_key, key)) {
      *shrunk = 1;
      return NULL; /* the dying node releases its own key/value */
    }
    return NULL;
  }
  if (node->kind == NODE_COLLISION) {
    if (node->hash != hash) {
      return NULL;
    }
    for (uint32_t i = 0; i < node->count; i++) {
      if (osprey_map_keys_equal(kt, node->coll_keys[i], key)) {
        *shrunk = 1;
        return remove_from_collision(node, i, hash, pol);
      }
    }
    return NULL;
  }
  uint32_t bit = bit_for(hash, shift);
  if ((node->bitmap & bit) == 0u) {
    return NULL;
  }
  uint32_t idx = index_for(node->bitmap, bit);
  OspreyMapNode *child = node->children[idx];
  OspreyMapNode *new_child = osprey_map_node_remove(
      child, shift + OSPREY_MAP_BITS, hash, key, pol, shrunk);
  if (!*shrunk) {
    return NULL;
  }
  if (new_child == NULL) {
    if (node->count == 1u) {
      return NULL;
    }
    uint32_t new_count = node->count - 1u;
    OspreyMapNode *out = make_internal(node->bitmap & ~bit, new_count, pol);
    memcpy(out->children, node->children,
           (size_t)idx * sizeof(OspreyMapNode *));
    memcpy(out->children + idx, node->children + idx + 1,
           (size_t)(node->count - idx - 1u) * sizeof(OspreyMapNode *));
    retain_kids(out->children, new_count);
    return out;
  }
  OspreyMapNode *out = copy_internal_except(node, idx, pol);
  out->children[idx] = new_child;
  return out;
}
