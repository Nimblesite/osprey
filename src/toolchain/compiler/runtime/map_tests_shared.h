// Shared by the translation units of the map_tests suite: the assertion
// counter, the sizes and keys every case walks, and the node, entry and
// iteration checks.
#ifndef OSPREY_MAP_TESTS_SHARED_H
#define OSPREY_MAP_TESTS_SHARED_H

#include "map_runtime_internal.h"

#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

extern uint64_t g_asserts;
#define CHECK(c)                                                               \
  do {                                                                         \
    g_asserts++;                                                               \
    assert(c);                                                                 \
  } while (0)
#define KI OSPREY_KEY_INT
#define KS OSPREY_KEY_STRING
#define KB OSPREY_KEY_BOOL
#define KBAD ((OspreyKeyType)99) /* exercises the hash/equality default arm */
#define SMALL_N 200
#define TINY_N 50
#define SCALE_N 20000
#define MERGE_LO 15000
#define MERGE_HI 25000
#define OVER_N 10
#define HALF_OVER 5
#define PREFIX_N 4
#define VAL_MUL 11
#define STR_KEYS 8
#define STR_BUF 16
#define CHAIN_LEVELS 6
#define COLL_N 6
#define BM_BOTH 3u   /* root bit of hash 0 plus the bit of hash 1 */
#define BM_ZERO 1u   /* root bit of hash 0 alone */
#define BM_CHAIN 1u  /* chunk 0 of hash 1, at every shift >= 5 */
#define COLL_HASH 1u /* hash_bool of any non-zero key */
#define FNV_BASIS 0x811c9dc5u
#define ABSENT_KEY 999999
#define ABSENT_COLL 13 /* non-zero => collides, but never inserted */
static const int64_t COLL_KEYS[COLL_N] = {1, 3, 5, 7, 9, 11};
static const char *const STRS[STR_KEYS] = {"alice", "bob",   "charlie", "dave",
                                           "eve",   "frank", "grace",   "heidi"};
static const char *const PREFIXES[PREFIX_N] = {"a", "ab", "abc", "abcd"};
/* Helpers ---------------------------------------------------------- */
static inline int64_t skey(const char *s) { return (int64_t)(uintptr_t)s; }
static inline void chk_node(OspreyMapNode *n, OspreyMapNodeKind kind, uint32_t bitmap,
                     uint32_t count) {
  CHECK(n != NULL); CHECK(n->kind == kind);
  CHECK(n->bitmap == bitmap); CHECK(n->count == count);
}
static inline void chk_leaf(OspreyMapNode *n, int64_t key, int64_t value) {
  chk_node(n, NODE_LEAF, 0u, 0u);
  CHECK(n->leaf_key == key); CHECK(n->leaf_value == value);
  CHECK(n->children == NULL); CHECK(n->coll_keys == NULL);
  CHECK(n->coll_values == NULL);
}
static inline void chk_entry(OspreyMap *m, int64_t k, int64_t v) {
  CHECK(osprey_map_contains(m, k) == 1); CHECK(osprey_map_get(m, k) == v);
}
static inline void chk_absent(OspreyMap *m, int64_t k) {
  CHECK(osprey_map_contains(m, k) == 0); CHECK(osprey_map_get(m, k) == 0);
}
static inline OspreyMap *make_int_map(int64_t start, int64_t end) {
  OspreyMap *m = osprey_map_empty(KI);
  for (int64_t i = start; i < end; i++) m = osprey_map_set(m, i, i * VAL_MUL);
  return m;
}
/* Every key in [start, end) resolves to key*VAL_MUL; the fringes must miss. */
static inline void chk_int_range(OspreyMap *m, int64_t start, int64_t end) {
  CHECK(osprey_map_length(m) == end - start);
  for (int64_t i = start; i < end; i++) chk_entry(m, i, i * VAL_MUL);
  chk_absent(m, start - 1); chk_absent(m, end);
}
/* Assert the shape any BOOL-keyed map holding key 0 plus colliding non-zero
   keys must have, and hand back the node its single-child chain ends in. */
static inline OspreyMapNode *coll_tail(OspreyMap *m) {
  chk_node(m->root, NODE_INTERNAL, BM_BOTH, 2u);
  chk_leaf(m->root->children[0], 0, 0);
  OspreyMapNode *n = m->root->children[1];
  int32_t levels = 0;
  while (n->kind == NODE_INTERNAL) {
    chk_node(n, NODE_INTERNAL, BM_CHAIN, 1u); CHECK(n->children != NULL);
    n = n->children[0];
    levels++;
  }
  CHECK(levels == CHAIN_LEVELS); CHECK(n != NULL);
  return n;
}
static inline OspreyMapNode *chk_coll(OspreyMap *m, uint32_t count) {
  OspreyMapNode *t = coll_tail(m);
  chk_node(t, NODE_COLLISION, 0u, count);
  CHECK(t->hash == COLL_HASH); CHECK(t->children == NULL);
  CHECK(t->coll_keys != NULL); CHECK(t->coll_values != NULL);
  for (uint32_t i = 0; i < count; i++)
    CHECK(t->coll_values[i] == t->coll_keys[i] * VAL_MUL);
  return t;
}
/* Drain an iterator: assert v == k*VAL_MUL, every key inside [base, base+n),
   and that no key is visited twice. Returns the number of pairs seen. */
static inline int64_t drain(OspreyMap *m, int64_t base, int64_t n) {
  unsigned char *seen = (unsigned char *)calloc((size_t)(n / 8 + 1), 1);
  OspreyMapIter *it = osprey_map_iter_new(m);
  int64_t k = 0, v = 0, count = 0;
  CHECK(seen != NULL); CHECK(it != NULL);
  while (osprey_map_iter_next(it, &k, &v)) {
    int64_t idx = k - base;
    CHECK(idx >= 0); CHECK(idx < n); CHECK(v == k * VAL_MUL);
    CHECK((seen[idx >> 3] & (unsigned char)(1u << (idx & 7))) == 0);
    seen[idx >> 3] |= (unsigned char)(1u << (idx & 7));
    count++;
  }
  CHECK(osprey_map_iter_next(it, &k, &v) == 0); /* stays exhausted */
  free(it); free(seen);
  return count;
}
static inline OspreyMap *make_coll_map(void) {
  OspreyMap *m = osprey_map_set(osprey_map_empty(KB), 0, 0);
  for (int32_t i = 0; i < COLL_N; i++)
    m = osprey_map_set(m, COLL_KEYS[i], COLL_KEYS[i] * VAL_MUL);
  return m;
}
static inline void chk_coll_map(OspreyMap *m) {
  CHECK(osprey_map_length(m) == COLL_N + 1);
  chk_entry(m, 0, 0);
  for (int32_t i = 0; i < COLL_N; i++)
    chk_entry(m, COLL_KEYS[i], COLL_KEYS[i] * VAL_MUL);
  chk_absent(m, ABSENT_COLL);
}
static inline OspreyMap *make_str_map(void) {
  OspreyMap *m = osprey_map_empty(KS);
  for (int64_t i = 0; i < STR_KEYS; i++)
    m = osprey_map_set(m, skey(STRS[i]), i * VAL_MUL);
  return m;
}

// map_bulk_tests.c
void test_merge(void);
void test_merge_collision(void);
void test_builder(void);
void test_builder_key_types(void);
void test_scale(void);
void test_entry_kind_flags(void);
void test_iter_lifecycle(void);

#endif
