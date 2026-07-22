#ifndef OSPREY_MAP_RUNTIME_INTERNAL_H
#define OSPREY_MAP_RUNTIME_INTERNAL_H

#include "collection_runtime.h"
#include "memory_hooks.h"
#include <stddef.h>
#include <stdint.h>

/*
 * Internal types shared between map_runtime.c (public API + iter + builder)
 * and map_runtime_hamt.c (HAMT node algebra: hash, assoc, lookup, remove).
 *
 * Not part of the public ABI — included only by the two map_runtime
 * translation units to keep each file under 500 LoC per CLAUDE.md.
 */

#define MAP_HASH_BITS 32

typedef enum {
  NODE_INTERNAL = 0,
  NODE_LEAF = 1,
  NODE_COLLISION = 2
} OspreyMapNodeKind;

typedef struct OspreyMapNode {
  OspreyMapNodeKind kind;
  uint32_t bitmap;
  uint32_t count;
  uint32_t hash;
  struct OspreyMapNode **children;
  int64_t leaf_key;
  int64_t leaf_value;
  int64_t *coll_keys;
  int64_t *coll_values;
} OspreyMapNode;

struct OspreyMap {
  OspreyKeyType key_type;
  int64_t length;
  OspreyMapNode *root;
  int32_t value_managed;
};

struct OspreyMapBuilder {
  OspreyKeyType key_type;
  int64_t length;
  OspreyMapNode *root;
  int32_t value_managed;
};

struct OspreyMapIter {
  OspreyMap *map;
  OspreyMapNode *stack_nodes[8];
  uint32_t stack_slots[8];
  int32_t stack_depth;
  uint32_t coll_index;
};

/* The element policy threaded through the node algebra [GC-ARC-PERCEUS] (plan
 * 0011 M4b): an OspreyKeyType in the low nibble plus MAP_VALUE_MANAGED when
 * the VALUE slot holds a managed pointer. The key side needs no extra bit —
 * OSPREY_KEY_STRING already says the key is a managed string. Packing it into
 * the existing parameter keeps every node function's arity (and the vanilla-C
 * node tests) unchanged; a bare OspreyKeyType is a valid policy meaning
 * "scalar values". */
typedef int32_t OspreyMapPolicy;
#define MAP_KEY_MASK 0x0F
#define MAP_VALUE_MANAGED 0x10

static inline OspreyKeyType map_key_type(OspreyMapPolicy pol) {
  return (OspreyKeyType)(pol & MAP_KEY_MASK);
}
static inline int map_key_is_managed(OspreyMapPolicy pol) {
  return map_key_type(pol) == OSPREY_KEY_STRING;
}
static inline int map_value_is_managed(OspreyMapPolicy pol) {
  return (pol & MAP_VALUE_MANAGED) != 0;
}
static inline OspreyMapPolicy map_policy(OspreyKeyType kt, int value_managed) {
  return (OspreyMapPolicy)kt | (value_managed ? MAP_VALUE_MANAGED : 0);
}

/* Layout words. A HAMT node owns its out-of-line `children` / `coll_keys` /
 * `coll_values` arrays and the nodes it shares are refcounted, so a dead
 * persistent version reclaims exactly the spine it stopped sharing. The
 * `leaf_key` / `leaf_value` inline element slots are walked only when that
 * side of the entry is a managed pointer — releasing a type-blind scalar whose
 * bits collided with a live address would be a use-after-free. The `coll_*`
 * arrays carry the same decision as their own OSP_MEM_PTR_ARRAY / RAW stamp.
 * No-ops off ARC. */
static inline int64_t map_node_layout(OspreyMapPolicy pol) {
  uint64_t bits = OSP_MEM_WORD(offsetof(OspreyMapNode, children)) |
                  OSP_MEM_WORD(offsetof(OspreyMapNode, coll_keys)) |
                  OSP_MEM_WORD(offsetof(OspreyMapNode, coll_values));
  if (map_key_is_managed(pol)) {
    bits |= OSP_MEM_WORD(offsetof(OspreyMapNode, leaf_key));
  }
  if (map_value_is_managed(pol)) {
    bits |= OSP_MEM_WORD(offsetof(OspreyMapNode, leaf_value));
  }
  return OSP_MEM_LAYOUT(bits);
}
#define MAP_HDR_LAYOUT OSP_MEM_LAYOUT(OSP_MEM_WORD(offsetof(OspreyMap, root)))

/* Hashing / equality. Take a bare key type, never a policy. */
uint32_t osprey_map_hash_key(OspreyKeyType kt, int64_t key);
int osprey_map_keys_equal(OspreyKeyType kt, int64_t a, int64_t b);

/* Node algebra. The grew / shrunk out-params report whether the operation
   changed cardinality. assoc TRANSFERS `key` and `value`. */
OspreyMapNode *osprey_map_node_assoc(OspreyMapNode *node, int32_t shift,
                                     uint32_t hash, int64_t key, int64_t value,
                                     OspreyMapPolicy pol, int *grew);
int osprey_map_node_lookup(OspreyMapNode *node, int32_t shift, uint32_t hash,
                           int64_t key, OspreyKeyType kt, int64_t *out);
OspreyMapNode *osprey_map_node_remove(OspreyMapNode *node, int32_t shift,
                                      uint32_t hash, int64_t key,
                                      OspreyMapPolicy pol, int *shrunk);

#endif /* OSPREY_MAP_RUNTIME_INTERNAL_H */
