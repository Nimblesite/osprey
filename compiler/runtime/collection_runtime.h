#ifndef OSPREY_COLLECTION_RUNTIME_H
#define OSPREY_COLLECTION_RUNTIME_H

#include <stdint.h>

/*
 * Shared types and ABI for List<T> and Map<K,V>.
 *
 * Spec: docs/specs/0004-TypeSystem.md#collection-types
 * Plan: docs/plans/collections.md
 *
 * Implements [TYPE-LIST], [TYPE-MAP], [TYPE-MAP-LOOKUP], [TYPE-MAP-OPS].
 *
 * This ABI is what the builtin collection surface of
 * docs/specs/0012-Built-InFunctions.md lowers to: [BUILTIN-COLLECTIONS],
 * [BUILTIN-COLLECTION-COMMON], [BUILTIN-LIST], [BUILTIN-MAP]. The two
 * `*_length` entry points back both [BUILTIN-COLLECTION-LENGTH] and
 * [BUILTIN-COLLECTION-ISEMPTY] (`isEmpty` is `length == 0`, folded at codegen).
 * The spec's bare spellings (`append`, `get`, `set`, …) are NOT the names that
 * ship today — see the implementation-status note in that spec section.
 *
 * Every element is stored as an int64_t. Pointers (strings, nested
 * collections, records) are cast to int64_t at storage time. Codegen on the
 * Go side is responsible for boxing/unboxing.
 *
 * Memory model: refcounted per node/header under the ARC backend, leak-semantic
 * under the default and tracing backends (the osp_retain / osp_release /
 * osp_mem_set_layout hooks are no-ops there, so one code path serves all
 * three). Structural sharing is achieved via path-copying; a shared node
 * outlives every version that still points at it.
 *
 * Element ownership [GC-ARC-PERCEUS] (docs/plans/0011 M4b). A container OWNS
 * its elements, so its death releases them — but element slots are type-blind
 * int64, and releasing an integer whose bits collide with a live heap address
 * would be a use-after-free. Every container therefore carries an
 * element-kind flag (`elem_managed` for List, the VALUE side for Map; a Map's
 * KEY side is already discriminated by OSPREY_KEY_STRING) that codegen decides
 * from the static Hindley-Milner element type at each insertion site. The
 * conventions, uniform across both containers:
 *   - insertion (`*_append*`, `*_prepend*`, `*_set*`, `*_builder_push*`,
 *     `*_builder_put*`) TRANSFERS the caller's reference — codegen dups before
 *     erasing the value into the i64 element ABI, and nothing retains again;
 *   - a derived container INHERITS its source's flag (set / concat / reverse /
 *     drop / remove / merge / seal);
 *   - extraction (`*_get`, the iterators) hands back a BORROWED reference: the
 *     source must outlive it, and storing it into another owner needs a dup —
 *     `osprey_list_builder_push_borrowed` is that dup.
 */

#define OSPREY_LIST_BITS 5
#define OSPREY_LIST_BRANCH 32
#define OSPREY_LIST_MASK 31

#define OSPREY_MAP_BITS 5
#define OSPREY_MAP_BRANCH 32

/* Key-type tags for Map. Codegen passes one of these at map creation. */
typedef enum {
  OSPREY_KEY_INT = 0,
  OSPREY_KEY_STRING = 1,
  OSPREY_KEY_BOOL = 2
} OspreyKeyType;

/* Opaque handles. */
typedef struct OspreyList OspreyList;
typedef struct OspreyListBuilder OspreyListBuilder;
typedef struct OspreyListIter OspreyListIter;

typedef struct OspreyMap OspreyMap;
typedef struct OspreyMapBuilder OspreyMapBuilder;
typedef struct OspreyMapIter OspreyMapIter;

/* ============ List API ============ */

/*
 * Pointer parameters are not declared `const` even though the runtime never
 * mutates inputs — returning a (possibly aliased) input from `concat` /
 * `drop` etc. would require dropping const, which `-Wcast-qual` rejects.
 * Treat every collection pointer as read-only by contract.
 */

OspreyList *osprey_list_empty(void);
int64_t osprey_list_length(OspreyList *l);
/* 1 when the element slots hold managed pointers, 0 when they hold scalars. */
int osprey_list_elem_managed(OspreyList *l);
/* 1 if 0 <= i < length, else 0. */
int osprey_list_in_bounds(OspreyList *l, int64_t i);
/* Caller must ensure osprey_list_in_bounds(l, i) before calling. BORROWED. */
int64_t osprey_list_get(OspreyList *l, int64_t i);
OspreyList *osprey_list_set(OspreyList *l, int64_t i, int64_t v);
/* The `_of` spellings take the element kind from the call site; the arity-2
   ones inherit it from the source list (an empty source means "scalar"). */
OspreyList *osprey_list_append_of(OspreyList *l, int64_t v, int elem_managed);
OspreyList *osprey_list_append(OspreyList *l, int64_t v);
OspreyList *osprey_list_prepend_of(OspreyList *l, int64_t v, int elem_managed);
OspreyList *osprey_list_prepend(OspreyList *l, int64_t v);
OspreyList *osprey_list_concat(OspreyList *a, OspreyList *b);
OspreyList *osprey_list_drop(OspreyList *l, int64_t n);
OspreyList *osprey_list_reverse(OspreyList *l);

OspreyListBuilder *osprey_list_builder_new_of(int elem_managed);
OspreyListBuilder *osprey_list_builder_new(void);
/* TRANSFERS `v` and latches the builder's element kind. */
void osprey_list_builder_push_of(OspreyListBuilder *b, int64_t v,
                                 int elem_managed);
void osprey_list_builder_push(OspreyListBuilder *b, int64_t v);
/* Dups `v` first: for elements BORROWED out of another container. */
void osprey_list_builder_push_borrowed(OspreyListBuilder *b, int64_t v);
OspreyList *osprey_list_builder_seal(OspreyListBuilder *b);

OspreyListIter *osprey_list_iter_new(OspreyList *l);
/* Returns 1 if a value was produced (in *out), 0 if exhausted. */
int osprey_list_iter_next(OspreyListIter *it, int64_t *out);

/* ============ Map API ============ */

OspreyMap *osprey_map_empty(OspreyKeyType key_type);
int64_t osprey_map_length(OspreyMap *m);
/* 1 when that side of an entry holds a managed pointer, 0 for a scalar. */
int osprey_map_key_managed(OspreyMap *m);
int osprey_map_value_managed(OspreyMap *m);
/* 1 if present, 0 if absent. */
int osprey_map_contains(OspreyMap *m, int64_t key);
/* Caller must ensure osprey_map_contains(m, k) before calling. BORROWED. */
int64_t osprey_map_get(OspreyMap *m, int64_t key);
/* Both key and value are TRANSFERRED. The `_of` spelling takes the value kind
   from the call site; the arity-3 one inherits it from `m`. */
OspreyMap *osprey_map_set_of(OspreyMap *m, int64_t key, int64_t value,
                             int value_managed);
OspreyMap *osprey_map_set(OspreyMap *m, int64_t key, int64_t value);
/* The lookup key is BORROWED (nothing is stored). */
OspreyMap *osprey_map_remove(OspreyMap *m, int64_t key);
/* Right-biased: keys in b override keys in a. */
OspreyMap *osprey_map_merge(OspreyMap *a, OspreyMap *b);

OspreyMapBuilder *osprey_map_builder_new(OspreyKeyType key_type);
void osprey_map_builder_put_of(OspreyMapBuilder *b, int64_t key, int64_t value,
                               int value_managed);
void osprey_map_builder_put(OspreyMapBuilder *b, int64_t key, int64_t value);
OspreyMap *osprey_map_builder_seal(OspreyMapBuilder *b);

OspreyMapIter *osprey_map_iter_new(OspreyMap *m);
/* Returns 1 if a (key, value) was produced, 0 if exhausted. */
int osprey_map_iter_next(OspreyMapIter *it, int64_t *out_key, int64_t *out_value);
/* Iterators are plain cursors that BORROW their map — codegen's mapKeys /
   mapValues loop must free the cursor when the walk ends, or one leaks per
   call on every backend. */
void osprey_map_iter_free(OspreyMapIter *it);

#endif /* OSPREY_COLLECTION_RUNTIME_H */
