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
 *
 * Backs the map builtins of docs/specs/0012-Built-InFunctions.md — [BUILTIN-MAP],
 * [BUILTIN-MAP-GET], [BUILTIN-MAP-SET], [BUILTIN-MAP-REMOVE],
 * [BUILTIN-MAP-MERGE], [BUILTIN-MAP-CONTAINS] and [BUILTIN-COLLECTION-LENGTH]
 * — under their `mapXxx` spellings, not the bare spec names. The iterator
 * below is what codegen walks for the accessors [BUILTIN-MAP-KEYS] and
 * [BUILTIN-MAP-VALUES]; note those two accessors are what the SHIPPED
 * `mapKeys` / `mapValues` builtins do, while the spec gives the same two
 * spellings to the unimplemented transformers [BUILTIN-MAP-MAPKEYS] /
 * [BUILTIN-MAP-MAPVALUES].
 */

/* ============ Public API ============ */

/* Every returned header owns its root spine: MAP_HDR_LAYOUT makes releasing a
 * dead persistent version release the nodes it stopped sharing, and node RC in
 * map_runtime_hamt.c keeps the versions that still share them alive.
 * [GC-ARC-PERCEUS] plan 0011 M4b. */
static OspreyMap *alloc_map(OspreyKeyType key_type, int64_t length,
                            OspreyMapNode *root, int value_managed) {
  OspreyMap *m = (OspreyMap *)calloc(1, sizeof(OspreyMap));
  osp_mem_set_layout(m, MAP_HDR_LAYOUT);
  m->key_type = key_type;
  m->length = length;
  m->root = root;
  m->value_managed = value_managed;
  return m;
}

OspreyMap *osprey_map_empty(OspreyKeyType key_type) {
  return alloc_map(key_type, 0, NULL, 0);
}

/* The key side is discriminated by the key type alone; the value side carries
 * the flag codegen supplied at the insertion that created this version. */
int osprey_map_key_managed(OspreyMap *m) {
  return (m != NULL && m->key_type == OSPREY_KEY_STRING) ? 1 : 0;
}

int osprey_map_value_managed(OspreyMap *m) {
  return (m == NULL) ? 0 : m->value_managed;
}

static OspreyMapPolicy policy_of(OspreyMap *m, int value_managed) {
  return map_policy(m->key_type, value_managed);
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

/* Insertion TRANSFERS both key and value (codegen dups each before erasing it
 * into the i64 element ABI). [GC-ARC-PERCEUS] */
OspreyMap *osprey_map_set_of(OspreyMap *m, int64_t key, int64_t value,
                             int value_managed) {
  uint32_t h = osprey_map_hash_key(m->key_type, key);
  int grew = 0;
  OspreyMapNode *new_root = osprey_map_node_assoc(
      m->root, 0, h, key, value, policy_of(m, value_managed), &grew);
  return alloc_map(m->key_type, m->length + (grew ? 1 : 0), new_root,
                   value_managed);
}

OspreyMap *osprey_map_set(OspreyMap *m, int64_t key, int64_t value) {
  return osprey_map_set_of(m, key, value, osprey_map_value_managed(m));
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
  OspreyMapNode *new_root = osprey_map_node_remove(
      m->root, 0, h, key, policy_of(m, m->value_managed), &shrunk);
  if (!shrunk) {
    /* node_remove hands back the unchanged root with its own alias +1; the
     * whole spine survives `m` if that reference is dropped on the floor. */
    osp_release(new_root);
    osp_retain(m);
    return m;
  }
  return alloc_map(m->key_type, m->length - 1, new_root, m->value_managed);
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

void osprey_map_iter_free(OspreyMapIter *it) { free(it); }

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
  int km = osprey_map_key_managed(a);
  int vm = osprey_map_value_managed(a);
  int64_t k;
  int64_t v;
  while (osprey_map_iter_next(it, &k, &v)) {
    /* The entry is BORROWED from `b` and set TRANSFERS, so dup both first. */
    if (km) {
      osp_retain((void *)(uintptr_t)k);
    }
    if (vm) {
      osp_retain((void *)(uintptr_t)v);
    }
    OspreyMap *next = osprey_map_set_of(out, k, v, vm);
    /* Intermediate headers are unaliased transients: drop each precisely
     * (no-op under default/gc) [GC-ARC-PERCEUS], plan 0011 M4. */
    if (out != a) {
      osp_release(out);
    }
    out = next;
  }
  osprey_map_iter_free(it);
  return out;
}

/* ============ Builder ============ */

OspreyMapBuilder *osprey_map_builder_new(OspreyKeyType key_type) {
  OspreyMapBuilder *b =
      (OspreyMapBuilder *)calloc(1, sizeof(OspreyMapBuilder));
  b->key_type = key_type;
  return b;
}

/* TRANSFERS key and value, and latches the builder's value kind — every node
 * it mints from here on is stamped alike. */
void osprey_map_builder_put_of(OspreyMapBuilder *b, int64_t key, int64_t value,
                               int value_managed) {
  b->value_managed = value_managed;
  uint32_t h = osprey_map_hash_key(b->key_type, key);
  int grew = 0;
  OspreyMapNode *old = b->root;
  b->root = osprey_map_node_assoc(old, 0, h, key, value,
                                  map_policy(b->key_type, value_managed),
                                  &grew);
  /* The new spine dup'd whatever it kept sharing, so the superseded version
   * dies here instead of accumulating one path per put. */
  osp_release(old);
  if (grew) {
    b->length++;
  }
}

void osprey_map_builder_put(OspreyMapBuilder *b, int64_t key, int64_t value) {
  osprey_map_builder_put_of(b, key, value, b->value_managed);
}

OspreyMap *osprey_map_builder_seal(OspreyMapBuilder *b) {
  /* The builder itself stays RAW (untagged): seal TRANSFERS the root to the
   * sealed header, so freeing the builder must not walk it. */
  OspreyMap *m = alloc_map(b->key_type, b->length, b->root, b->value_managed);
  free(b);
  return m;
}
