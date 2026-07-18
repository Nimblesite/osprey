#include "map_runtime_internal.h"
#include "memory_hooks.h"

#include <stdint.h>
#include <stdlib.h>

/*
 * Persistent Map<K, V> public API + iterator + builder. HAMT node
 * algebra (hashing, assoc, lookup, remove) lives in map_runtime_hamt.c.
 *
 * Implements [TYPE-MAP], [TYPE-MAP-LOOKUP], [TYPE-MAP-OPS] from
 * docs/specs/0004-TypeSystem.md.
 */

/* ============ Public API ============ */

OspreyMap *osprey_map_empty(OspreyKeyType key_type) {
  OspreyMap *m = (OspreyMap *)calloc(1, sizeof(OspreyMap));
  m->key_type = key_type;
  return m;
}

int64_t osprey_map_length(OspreyMap *m) {
  if (m == NULL) {
    return 0;
  }
  return m->length;
}

int osprey_map_contains(OspreyMap *m, int64_t key) {
  if (m == NULL || m->root == NULL) {
    return 0;
  }
  uint32_t h = osprey_map_hash_key(m->key_type, key);
  int64_t dummy;
  return osprey_map_node_lookup(m->root, 0, h, key, m->key_type, &dummy);
}

int64_t osprey_map_get(OspreyMap *m, int64_t key) {
  uint32_t h = osprey_map_hash_key(m->key_type, key);
  int64_t out = 0;
  (void)osprey_map_node_lookup(m->root, 0, h, key, m->key_type, &out);
  return out;
}

OspreyMap *osprey_map_set(OspreyMap *m, int64_t key, int64_t value) {
  uint32_t h = osprey_map_hash_key(m->key_type, key);
  int grew = 0;
  OspreyMapNode *new_root =
      osprey_map_node_assoc(m->root, 0, h, key, value, m->key_type, &grew);
  OspreyMap *out = (OspreyMap *)calloc(1, sizeof(OspreyMap));
  out->key_type = m->key_type;
  out->root = new_root;
  out->length = m->length + (grew ? 1 : 0);
  return out;
}

OspreyMap *osprey_map_remove(OspreyMap *m, int64_t key) {
  /* Alias returns carry a fresh +1 (retain-on-return) so every returned
   * handle is caller-owned [GC-ARC-PERCEUS], plan 0011 M4. */
  if (m == NULL || m->root == NULL) {
    osp_retain(m);
    return m;
  }
  uint32_t h = osprey_map_hash_key(m->key_type, key);
  int shrunk = 0;
  OspreyMapNode *new_root =
      osprey_map_node_remove(m->root, 0, h, key, m->key_type, &shrunk);
  if (!shrunk) {
    osp_retain(m);
    return m;
  }
  OspreyMap *out = (OspreyMap *)calloc(1, sizeof(OspreyMap));
  out->key_type = m->key_type;
  out->root = new_root;
  out->length = m->length - 1;
  return out;
}

/* ============ Iteration ============ */

OspreyMapIter *osprey_map_iter_new(OspreyMap *m) {
  OspreyMapIter *it = (OspreyMapIter *)calloc(1, sizeof(OspreyMapIter));
  it->map = m;
  if (m != NULL && m->root != NULL) {
    it->stack_nodes[0] = m->root;
    it->stack_slots[0] = 0;
    it->stack_depth = 1;
  }
  return it;
}

int osprey_map_iter_next(OspreyMapIter *it, int64_t *out_key,
                         int64_t *out_value) {
  while (it->stack_depth > 0) {
    int32_t top = it->stack_depth - 1;
    OspreyMapNode *node = it->stack_nodes[top];
    uint32_t slot = it->stack_slots[top];
    if (node->kind == NODE_LEAF) {
      if (slot == 0u) {
        *out_key = node->leaf_key;
        *out_value = node->leaf_value;
        it->stack_slots[top] = 1u;
        it->stack_depth = top;
        return 1;
      }
      it->stack_depth = top;
      continue;
    }
    if (node->kind == NODE_COLLISION) {
      if (slot < node->count) {
        *out_key = node->coll_keys[slot];
        *out_value = node->coll_values[slot];
        it->stack_slots[top] = slot + 1u;
        return 1;
      }
      it->stack_depth = top;
      continue;
    }
    if (slot < node->count) {
      it->stack_slots[top] = slot + 1u;
      if ((size_t)it->stack_depth >=
          sizeof(it->stack_nodes) / sizeof(it->stack_nodes[0])) {
        return 0;
      }
      it->stack_nodes[it->stack_depth] = node->children[slot];
      it->stack_slots[it->stack_depth] = 0;
      it->stack_depth++;
      continue;
    }
    it->stack_depth = top;
  }
  return 0;
}

OspreyMap *osprey_map_merge(OspreyMap *a, OspreyMap *b) {
  if (a == NULL || a->length == 0) {
    osp_retain(b); /* alias return: +1 to the caller (see remove) */
    return b;
  }
  if (b == NULL || b->length == 0) {
    osp_retain(a);
    return a;
  }
  OspreyMap *out = a;
  OspreyMapIter *it = osprey_map_iter_new(b);
  int64_t k;
  int64_t v;
  while (osprey_map_iter_next(it, &k, &v)) {
    OspreyMap *next = osprey_map_set(out, k, v);
    /* Intermediate headers are unaliased transients: drop each precisely
     * (no-op under default/gc) [GC-ARC-PERCEUS], plan 0011 M4. */
    if (out != a) {
      osp_release(out);
    }
    out = next;
  }
  free(it);
  return out;
}

/* ============ Builder ============ */

OspreyMapBuilder *osprey_map_builder_new(OspreyKeyType key_type) {
  OspreyMapBuilder *b =
      (OspreyMapBuilder *)calloc(1, sizeof(OspreyMapBuilder));
  b->key_type = key_type;
  return b;
}

void osprey_map_builder_put(OspreyMapBuilder *b, int64_t key, int64_t value) {
  uint32_t h = osprey_map_hash_key(b->key_type, key);
  int grew = 0;
  b->root =
      osprey_map_node_assoc(b->root, 0, h, key, value, b->key_type, &grew);
  if (grew) {
    b->length++;
  }
}

OspreyMap *osprey_map_builder_seal(OspreyMapBuilder *b) {
  OspreyMap *m = (OspreyMap *)calloc(1, sizeof(OspreyMap));
  m->key_type = b->key_type;
  m->length = b->length;
  m->root = b->root;
  free(b);
  return m;
}
