#include "map_runtime_internal.h"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
/*
 * Vanilla-C tests for map_runtime.c + map_runtime_hamt.c. assert()-driven,
 * non-zero exit on failure. Build (all warnings are errors):
 *   cc -O2 -D_FORTIFY_SOURCE=2 -fstack-protector-strong -Werror -Wall -Wextra \
 *      -ftrapv -std=c11 -D_GNU_SOURCE -o map_tests \
 *      map_tests.c map_runtime.c map_runtime_hamt.c memory_runtime.c
 * Covers the runtime half of the map builtins [BUILTIN-MAP], [BUILTIN-MAP-GET],
 * [BUILTIN-MAP-SET], [BUILTIN-MAP-REMOVE], [BUILTIN-MAP-MERGE],
 * [BUILTIN-MAP-CONTAINS], [BUILTIN-COLLECTION-LENGTH] and the iteration behind
 * [BUILTIN-MAP-KEYS] / [BUILTIN-MAP-VALUES]
 * (docs/specs/0012-Built-InFunctions.md), plus
 * [TYPE-MAP] / [TYPE-MAP-LOOKUP] / [TYPE-MAP-OPS] and the node refcount
 * skeleton of [GC-ARC-PERCEUS] (plan 0011 M4b) for INT / STRING / BOOL keys:
 * persistence of every source map, pointer-identical alias returns, all three
 * node kinds asserted structurally, every collision / internal-node branch,
 * iteration, the builder, and a 20k-key scale pass.
 * BOOL keys hash to two buckets (0, 1) while equality stays `==`, so distinct
 * non-zero BOOL keys are a full 32-bit hash collision: that is how
 * NODE_COLLISION is forced, under CHAIN_LEVELS single-child internals — the
 * deepest spine a 32-bit / 5-bit HAMT builds, and exactly the iterator stack.
 */
static uint64_t g_asserts;
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
static int64_t skey(const char *s) { return (int64_t)(uintptr_t)s; }
static void chk_node(OspreyMapNode *n, OspreyMapNodeKind kind, uint32_t bitmap,
                     uint32_t count) {
  CHECK(n != NULL); CHECK(n->kind == kind);
  CHECK(n->bitmap == bitmap); CHECK(n->count == count);
}
static void chk_leaf(OspreyMapNode *n, int64_t key, int64_t value) {
  chk_node(n, NODE_LEAF, 0u, 0u);
  CHECK(n->leaf_key == key); CHECK(n->leaf_value == value);
  CHECK(n->children == NULL); CHECK(n->coll_keys == NULL);
  CHECK(n->coll_values == NULL);
}
static void chk_entry(OspreyMap *m, int64_t k, int64_t v) {
  CHECK(osprey_map_contains(m, k) == 1); CHECK(osprey_map_get(m, k) == v);
}
static void chk_absent(OspreyMap *m, int64_t k) {
  CHECK(osprey_map_contains(m, k) == 0); CHECK(osprey_map_get(m, k) == 0);
}
static OspreyMap *make_int_map(int64_t start, int64_t end) {
  OspreyMap *m = osprey_map_empty(KI);
  for (int64_t i = start; i < end; i++) m = osprey_map_set(m, i, i * VAL_MUL);
  return m;
}
/* Every key in [start, end) resolves to key*VAL_MUL; the fringes must miss. */
static void chk_int_range(OspreyMap *m, int64_t start, int64_t end) {
  CHECK(osprey_map_length(m) == end - start);
  for (int64_t i = start; i < end; i++) chk_entry(m, i, i * VAL_MUL);
  chk_absent(m, start - 1); chk_absent(m, end);
}
/* Assert the shape any BOOL-keyed map holding key 0 plus colliding non-zero
   keys must have, and hand back the node its single-child chain ends in. */
static OspreyMapNode *coll_tail(OspreyMap *m) {
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
static OspreyMapNode *chk_coll(OspreyMap *m, uint32_t count) {
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
static int64_t drain(OspreyMap *m, int64_t base, int64_t n) {
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
static OspreyMap *make_coll_map(void) {
  OspreyMap *m = osprey_map_set(osprey_map_empty(KB), 0, 0);
  for (int32_t i = 0; i < COLL_N; i++)
    m = osprey_map_set(m, COLL_KEYS[i], COLL_KEYS[i] * VAL_MUL);
  return m;
}
static void chk_coll_map(OspreyMap *m) {
  CHECK(osprey_map_length(m) == COLL_N + 1);
  chk_entry(m, 0, 0);
  for (int32_t i = 0; i < COLL_N; i++)
    chk_entry(m, COLL_KEYS[i], COLL_KEYS[i] * VAL_MUL);
  chk_absent(m, ABSENT_COLL);
}
static OspreyMap *make_str_map(void) {
  OspreyMap *m = osprey_map_empty(KS);
  for (int64_t i = 0; i < STR_KEYS; i++)
    m = osprey_map_set(m, skey(STRS[i]), i * VAL_MUL);
  return m;
}
/* Tests ------------------------------------------------------------ */
static void test_hash_and_equality(void) {
  char copy[STR_BUF];
  (void)snprintf(copy, sizeof(copy), "%s", STRS[0]);
  CHECK(skey(STRS[0]) != skey(copy));
  /* the splitmix64 finalizer maps 0 to 0; elsewhere only determinism holds */
  CHECK(osprey_map_hash_key(KI, 0) == 0u);
  CHECK(osprey_map_hash_key(KI, 7) == osprey_map_hash_key(KI, 7));
  CHECK(osprey_map_hash_key(KI, 7) != osprey_map_hash_key(KI, 8));
  CHECK(osprey_map_hash_key(KB, 0) == 0u);
  CHECK(osprey_map_hash_key(KB, 1) == COLL_HASH);
  CHECK(osprey_map_hash_key(KB, -5) == COLL_HASH);
  CHECK(osprey_map_hash_key(KB, ABSENT_COLL) == COLL_HASH);
  CHECK(osprey_map_hash_key(KS, 0) == 0u); /* NULL string */
  CHECK(osprey_map_hash_key(KS, skey("")) == FNV_BASIS);
  CHECK(osprey_map_hash_key(KS, skey(STRS[0])) ==
        osprey_map_hash_key(KS, skey(copy)));
  CHECK(osprey_map_hash_key(KS, skey(STRS[0])) !=
        osprey_map_hash_key(KS, skey(STRS[1])));
  CHECK(osprey_map_hash_key(KBAD, 1) == 0u);
  CHECK(osprey_map_keys_equal(KI, 5, 5) == 1);
  CHECK(osprey_map_keys_equal(KI, 5, 6) == 0);
  CHECK(osprey_map_keys_equal(KI, -1, -1) == 1);
  CHECK(osprey_map_keys_equal(KB, 0, 0) == 1);
  CHECK(osprey_map_keys_equal(KB, 1, 3) == 0);
  CHECK(osprey_map_keys_equal(KBAD, 4, 4) == 1);
  /* strings: pointer fast path, strcmp path, mismatch, every NULL spelling */
  CHECK(osprey_map_keys_equal(KS, skey(STRS[0]), skey(STRS[0])) == 1);
  CHECK(osprey_map_keys_equal(KS, skey(STRS[0]), skey(copy)) == 1);
  CHECK(osprey_map_keys_equal(KS, skey(STRS[0]), skey(STRS[1])) == 0);
  CHECK(osprey_map_keys_equal(KS, 0, skey(STRS[0])) == 0);
  CHECK(osprey_map_keys_equal(KS, skey(STRS[0]), 0) == 0);
  CHECK(osprey_map_keys_equal(KS, 0, 0) == 1);
}
static void test_node_algebra(void) {
  int grew = -1, shrunk = -1;
  int64_t out = 123;
  CHECK(osprey_map_node_lookup(NULL, 0, 0u, 0, KI, &out) == 0); CHECK(out == 123);
  CHECK(osprey_map_node_remove(NULL, 0, 0u, 0, KI, &shrunk) == NULL);
  CHECK(shrunk == 0);
  OspreyMapNode *leaf = osprey_map_node_assoc(NULL, 0, 7u, 5, 50, KI, &grew);
  CHECK(grew == 1); chk_leaf(leaf, 5, 50); CHECK(leaf->hash == 7u);
  CHECK(osprey_map_node_lookup(leaf, 0, 7u, 5, KI, &out) == 1); CHECK(out == 50);
  /* right hash + wrong key, and right key under the wrong hash, both miss */
  CHECK(osprey_map_node_lookup(leaf, 0, 7u, 6, KI, &out) == 0);
  CHECK(osprey_map_node_lookup(leaf, 0, 8u, 5, KI, &out) == 0); CHECK(out == 50);
  /* value replacement: fresh node, unchanged cardinality, source intact */
  OspreyMapNode *same = osprey_map_node_assoc(leaf, 0, 7u, 5, 51, KI, &grew);
  CHECK(grew == 0); CHECK(same != leaf); CHECK(same->leaf_value == 51);
  CHECK(leaf->leaf_value == 50);
  /* removing a non-matching key alias-returns the very same node */
  CHECK(osprey_map_node_remove(leaf, 0, 7u, 6, KI, &shrunk) == leaf);
  CHECK(shrunk == 0);
  CHECK(osprey_map_node_remove(leaf, 0, 7u, 5, KI, &shrunk) == NULL);
  CHECK(shrunk == 1);
}
static void test_empty_and_single(void) {
  OspreyMap *m = osprey_map_empty(KI);
  CHECK(m != NULL); CHECK(m->root == NULL); CHECK(m->key_type == KI);
  CHECK(osprey_map_length(m) == 0);
  chk_absent(m, 0); chk_absent(m, ABSENT_KEY);
  CHECK(osprey_map_length(NULL) == 0); CHECK(osprey_map_contains(NULL, 1) == 0);
  CHECK(osprey_map_remove(NULL, 1) == NULL);
  CHECK(osprey_map_remove(m, 5) == m); /* NULL root alias-returns the map */
  CHECK(drain(m, 0, 1) == 0); CHECK(osprey_map_iter_new(NULL) != NULL);
  OspreyMap *one = osprey_map_set(m, 1, VAL_MUL);
  CHECK(one != m); chk_leaf(one->root, 1, VAL_MUL);
  CHECK(osprey_map_length(one) == 1);
  chk_entry(one, 1, VAL_MUL); chk_absent(one, 2);
  CHECK(drain(one, 1, 1) == 1);
  CHECK(osprey_map_length(m) == 0); chk_absent(m, 1); /* source untouched */
  OspreyMap *over = osprey_map_set(one, 1, 2 * VAL_MUL);
  CHECK(osprey_map_length(over) == 1); chk_entry(over, 1, 2 * VAL_MUL);
  chk_entry(one, 1, VAL_MUL);              /* source untouched */
  CHECK(osprey_map_remove(one, 2) == one); /* absent: same map back */
  OspreyMap *gone = osprey_map_remove(one, 1);
  CHECK(gone != one); CHECK(gone->root == NULL);
  CHECK(osprey_map_length(gone) == 0); chk_absent(gone, 1);
  chk_entry(one, 1, VAL_MUL); CHECK(osprey_map_length(one) == 1);
}
static void test_int_keys_persistent(void) {
  OspreyMap *m = make_int_map(0, SMALL_N);
  chk_int_range(m, 0, SMALL_N);
  CHECK(m->root->kind == NODE_INTERNAL); CHECK(m->root->count > 1u);
  CHECK(__builtin_popcount(m->root->bitmap) == (int)m->root->count);
  CHECK(drain(m, 0, SMALL_N) == SMALL_N);
  /* Overwrite deep inside the trie: the source keeps its value everywhere. */
  OspreyMap *m2 = osprey_map_set(m, TINY_N, -99);
  CHECK(osprey_map_length(m2) == SMALL_N); chk_entry(m2, TINY_N, -99);
  chk_int_range(m, 0, SMALL_N);
  for (int64_t i = 0; i < SMALL_N; i++)
    CHECK(osprey_map_get(m2, i) == ((i == TINY_N) ? -99 : i * VAL_MUL));
  /* Re-setting the identical value still yields a distinct header. */
  OspreyMap *m3 = osprey_map_set(m, TINY_N, TINY_N * VAL_MUL);
  CHECK(m3 != m); chk_int_range(m3, 0, SMALL_N);
}
static void test_remove_paths(void) {
  OspreyMap *m = make_int_map(0, TINY_N);
  OspreyMap *m2 = osprey_map_remove(m, TINY_N / 2);
  CHECK(m2 != m); CHECK(osprey_map_length(m2) == TINY_N - 1);
  chk_absent(m2, TINY_N / 2);
  for (int64_t i = 0; i < TINY_N; i++)
    CHECK(osprey_map_contains(m2, i) == (i == TINY_N / 2 ? 0 : 1));
  chk_int_range(m, 0, TINY_N); /* source untouched */
  CHECK(osprey_map_remove(m, ABSENT_KEY) == m);
  CHECK(osprey_map_remove(m2, TINY_N / 2) == m2);
  /* Remove every entry: the root must shrink all the way back to NULL. */
  OspreyMap *cleared = m;
  for (int64_t i = 0; i < TINY_N; i++) {
    cleared = osprey_map_remove(cleared, i);
    CHECK(osprey_map_length(cleared) == TINY_N - 1 - i);
  }
  CHECK(cleared->root == NULL); CHECK(drain(cleared, 0, TINY_N) == 0);
  chk_int_range(m, 0, TINY_N);
  /* remove -> set round trip restores the cardinality with the new value. */
  OspreyMap *re = osprey_map_set(m2, TINY_N / 2, 7);
  CHECK(osprey_map_length(re) == TINY_N);
  for (int64_t i = 0; i < TINY_N; i++)
    CHECK(osprey_map_get(re, i) == (i == TINY_N / 2 ? 7 : i * VAL_MUL));
}
static void test_string_keys(void) {
  OspreyMap *m = make_str_map();
  CHECK(osprey_map_length(m) == STR_KEYS);
  /* Independent buffers: value equality, never pointer equality. */
  for (int64_t i = 0; i < STR_KEYS; i++) {
    char buf[STR_BUF];
    (void)snprintf(buf, sizeof(buf), "%s", STRS[i]);
    CHECK(skey(buf) != skey(STRS[i])); chk_entry(m, skey(buf), i * VAL_MUL);
  }
  chk_absent(m, skey("missing")); chk_absent(m, skey(""));
  /* Shared prefixes must not alias each other. */
  OspreyMap *p = osprey_map_empty(KS);
  for (int64_t i = 0; i < PREFIX_N; i++)
    p = osprey_map_set(p, skey(PREFIXES[i]), i + 1);
  CHECK(osprey_map_length(p) == PREFIX_N);
  for (int64_t i = 0; i < PREFIX_N; i++) chk_entry(p, skey(PREFIXES[i]), i + 1);
}
static void test_string_aliases(void) {
  OspreyMap *m = make_str_map();
  char alias[STR_BUF];
  (void)snprintf(alias, sizeof(alias), "%s", STRS[3]);
  /* Overwrite through a different pointer holding the same bytes. */
  OspreyMap *ov = osprey_map_set(m, skey(alias), -1);
  CHECK(osprey_map_length(ov) == STR_KEYS); chk_entry(ov, skey(STRS[3]), -1);
  chk_entry(m, skey(STRS[3]), 3 * VAL_MUL);
  /* Remove through that same alias pointer. */
  OspreyMap *rm = osprey_map_remove(m, skey(alias));
  CHECK(osprey_map_length(rm) == STR_KEYS - 1); chk_absent(rm, skey(STRS[3]));
  chk_entry(m, skey(STRS[3]), 3 * VAL_MUL);
  CHECK(osprey_map_remove(m, skey("nope")) == m);
  OspreyMap *withnull = osprey_map_set(m, 0, 42); /* NULL is a legal key */
  CHECK(osprey_map_length(withnull) == STR_KEYS + 1);
  chk_entry(withnull, 0, 42); chk_absent(m, 0);
  CHECK(osprey_map_length(m) == STR_KEYS);
}
static void test_bool_keys(void) {
  OspreyMap *m = osprey_map_set(osprey_map_empty(KB), 0, 100);
  chk_leaf(m->root, 0, 100);
  m = osprey_map_set(m, 1, 200);
  CHECK(osprey_map_length(m) == 2);
  /* false/true differ in bit 0 only: one internal node holding two leaves. */
  chk_node(m->root, NODE_INTERNAL, BM_BOTH, 2u);
  chk_leaf(m->root->children[0], 0, 100);
  chk_leaf(m->root->children[1], 1, 200);
  chk_entry(m, 0, 100); chk_entry(m, 1, 200);
  OspreyMap *m2 = osprey_map_set(m, 1, 999);
  CHECK(osprey_map_length(m2) == 2);
  chk_entry(m2, 1, 999); chk_entry(m2, 0, 100);
  chk_entry(m, 1, 200); /* source untouched */
}
static void test_collision_structure(void) {
  OspreyMap *m = make_coll_map();
  chk_coll_map(m); (void)chk_coll(m, (uint32_t)COLL_N);
  /* Iterating this spine uses exactly the 8-slot iterator stack. */
  CHECK(drain(m, 0, COLL_KEYS[COLL_N - 1] + 1) == COLL_N + 1);
  CHECK(osprey_map_remove(m, ABSENT_COLL) == m); /* same hash, absent key */
  chk_coll_map(m);
  /* Two colliding keys are the minimum collision node. */
  OspreyMap *pair = osprey_map_set(osprey_map_empty(KB), 0, 0);
  pair = osprey_map_set(pair, COLL_KEYS[0], COLL_KEYS[0] * VAL_MUL);
  chk_node(pair->root, NODE_INTERNAL, BM_BOTH, 2u);
  pair = osprey_map_set(pair, COLL_KEYS[1], COLL_KEYS[1] * VAL_MUL);
  (void)chk_coll(pair, 2u); CHECK(osprey_map_length(pair) == 3);
  CHECK(drain(pair, 0, COLL_KEYS[1] + 1) == 3);
}
static void test_collision_update_and_insert(void) {
  OspreyMap *m = make_coll_map();
  /* Update an existing entry: same arity, new value, source intact. */
  OspreyMap *up = osprey_map_set(m, COLL_KEYS[2], -7);
  CHECK(osprey_map_length(up) == COLL_N + 1);
  chk_node(coll_tail(up), NODE_COLLISION, 0u, (uint32_t)COLL_N);
  for (int32_t i = 0; i < COLL_N; i++)
    CHECK(osprey_map_get(up, COLL_KEYS[i]) ==
          (i == 2 ? -7 : COLL_KEYS[i] * VAL_MUL));
  chk_coll_map(m);
  /* Insert a NEW colliding key: arity grows by one, source intact. */
  OspreyMap *ins = osprey_map_set(m, ABSENT_COLL, ABSENT_COLL * VAL_MUL);
  CHECK(osprey_map_length(ins) == COLL_N + 2);
  (void)chk_coll(ins, (uint32_t)COLL_N + 1u);
  chk_entry(ins, ABSENT_COLL, ABSENT_COLL * VAL_MUL); chk_entry(ins, 0, 0);
  CHECK(drain(ins, 0, ABSENT_COLL + 1) == COLL_N + 2);
  chk_coll_map(m);
}
static void test_collision_removal(void) {
  OspreyMap *m = make_coll_map();
  /* Remove from a >2 collision: arity drops, everything else survives. */
  OspreyMap *c = osprey_map_remove(m, COLL_KEYS[0]);
  CHECK(osprey_map_length(c) == COLL_N); chk_absent(c, COLL_KEYS[0]);
  (void)chk_coll(c, (uint32_t)COLL_N - 1u);
  for (int32_t i = 1; i < COLL_N; i++)
    chk_entry(c, COLL_KEYS[i], COLL_KEYS[i] * VAL_MUL);
  /* Removing the LAST slot exercises the zero-length tail memcpy. */
  OspreyMap *cl = osprey_map_remove(c, COLL_KEYS[COLL_N - 1]);
  (void)chk_coll(cl, (uint32_t)COLL_N - 2u);
  chk_absent(cl, COLL_KEYS[COLL_N - 1]);
  CHECK(osprey_map_length(cl) == COLL_N - 1);
  /* Drain down to two, then one more: the collision degenerates to a leaf. */
  OspreyMap *d = c;
  for (int32_t i = 1; i < COLL_N - 2; i++) d = osprey_map_remove(d, COLL_KEYS[i]);
  (void)chk_coll(d, 2u); CHECK(osprey_map_length(d) == 3);
  d = osprey_map_remove(d, COLL_KEYS[COLL_N - 2]);
  chk_leaf(coll_tail(d), COLL_KEYS[COLL_N - 1], COLL_KEYS[COLL_N - 1] * VAL_MUL);
  CHECK(osprey_map_length(d) == 2);
  CHECK(drain(d, 0, COLL_KEYS[COLL_N - 1] + 1) == 2);
  /* Removing that leaf collapses the whole chain and clears a root bit. */
  d = osprey_map_remove(d, COLL_KEYS[COLL_N - 1]);
  chk_node(d->root, NODE_INTERNAL, BM_ZERO, 1u);
  chk_leaf(d->root->children[0], 0, 0); CHECK(osprey_map_length(d) == 1);
  /* Removing the survivor shrinks the last internal node to NULL. */
  d = osprey_map_remove(d, 0);
  CHECK(d->root == NULL); CHECK(osprey_map_length(d) == 0); chk_absent(d, 0);
  chk_coll_map(m); /* the original survived every one of those versions */
}
static void test_merge(void) {
  OspreyMap *e = osprey_map_empty(KI);
  OspreyMap *a = osprey_map_empty(KI);
  OspreyMap *b = osprey_map_empty(KI);
  for (int64_t i = 0; i < OVER_N; i++) {
    a = osprey_map_set(a, i, 100 + i);
    b = osprey_map_set(b, i + HALF_OVER, 200 + i + HALF_OVER);
  }
  OspreyMap *m = osprey_map_merge(a, b);
  CHECK(osprey_map_length(m) == OVER_N + HALF_OVER);
  for (int64_t i = 0; i < HALF_OVER; i++) chk_entry(m, i, 100 + i);
  for (int64_t i = HALF_OVER; i < OVER_N + HALF_OVER; i++)
    chk_entry(m, i, 200 + i); /* b wins the overlap and owns the tail */
  CHECK(osprey_map_length(a) == OVER_N); CHECK(osprey_map_length(b) == OVER_N);
  for (int64_t i = 0; i < OVER_N; i++) {
    chk_entry(a, i, 100 + i); /* both sources unchanged */
    CHECK(osprey_map_contains(b, i) == (i < HALF_OVER ? 0 : 1));
  }
  /* empty / NULL operands alias-return the other side verbatim */
  CHECK(osprey_map_merge(e, a) == a); CHECK(osprey_map_merge(a, e) == a);
  CHECK(osprey_map_merge(e, e) == e); CHECK(osprey_map_merge(NULL, a) == a);
  CHECK(osprey_map_merge(a, NULL) == a);
  CHECK(osprey_map_merge(NULL, NULL) == NULL);
  OspreyMap *self = osprey_map_merge(a, a); /* self-merge preserves content */
  CHECK(osprey_map_length(self) == OVER_N);
  for (int64_t i = 0; i < OVER_N; i++) chk_entry(self, i, 100 + i);
}
static void test_merge_collision(void) {
  OspreyMap *cm = make_coll_map();
  OspreyMap *cb =
      osprey_map_set(osprey_map_empty(KB), ABSENT_COLL, ABSENT_COLL * VAL_MUL);
  OspreyMap *merged = osprey_map_merge(cm, cb);
  CHECK(osprey_map_length(merged) == COLL_N + 2);
  chk_entry(merged, ABSENT_COLL, ABSENT_COLL * VAL_MUL);
  chk_entry(merged, 0, 0);
  for (int32_t i = 0; i < COLL_N; i++)
    chk_entry(merged, COLL_KEYS[i], COLL_KEYS[i] * VAL_MUL);
  CHECK(drain(merged, 0, ABSENT_COLL + 1) == COLL_N + 2);
  chk_coll_map(cm); CHECK(osprey_map_length(cb) == 1); chk_absent(cb, 0);
}
static void test_builder(void) {
  OspreyMapBuilder *eb = osprey_map_builder_new(KI);
  CHECK(eb != NULL);
  OspreyMap *zero = osprey_map_builder_seal(eb); /* sealing an empty builder */
  CHECK(zero->root == NULL); CHECK(osprey_map_length(zero) == 0);
  CHECK(zero->key_type == KI); chk_absent(zero, 0);
  OspreyMapBuilder *b = osprey_map_builder_new(KI);
  for (int64_t i = 0; i < SMALL_N; i++) osprey_map_builder_put(b, i, i * VAL_MUL);
  for (int64_t i = 0; i < SMALL_N; i += 10)
    osprey_map_builder_put(b, i, i * VAL_MUL); /* duplicates must not grow */
  osprey_map_builder_put(b, 0, 0);
  OspreyMap *built = osprey_map_builder_seal(b);
  chk_int_range(built, 0, SMALL_N); CHECK(drain(built, 0, SMALL_N) == SMALL_N);
}
static void test_builder_key_types(void) {
  OspreyMapBuilder *sb = osprey_map_builder_new(KS);
  for (int64_t i = 0; i < STR_KEYS; i++)
    osprey_map_builder_put(sb, skey(STRS[i]), i * VAL_MUL);
  char alias[STR_BUF];
  (void)snprintf(alias, sizeof(alias), "%s", STRS[2]);
  osprey_map_builder_put(sb, skey(alias), -3); /* duplicate via an alias */
  OspreyMap *sm = osprey_map_builder_seal(sb);
  CHECK(sm->key_type == KS); CHECK(osprey_map_length(sm) == STR_KEYS);
  chk_entry(sm, skey(STRS[2]), -3);
  for (int64_t i = 0; i < STR_KEYS; i++)
    CHECK(osprey_map_contains(sm, skey(STRS[i])) == 1);
  /* A BOOL-keyed builder drives assoc through the collision path. */
  OspreyMapBuilder *bb = osprey_map_builder_new(KB);
  osprey_map_builder_put(bb, 0, 0);
  for (int32_t i = 0; i < COLL_N; i++)
    osprey_map_builder_put(bb, COLL_KEYS[i], COLL_KEYS[i] * VAL_MUL);
  osprey_map_builder_put(bb, COLL_KEYS[0], COLL_KEYS[0] * VAL_MUL);
  OspreyMap *bm = osprey_map_builder_seal(bb);
  chk_coll_map(bm); (void)chk_coll(bm, (uint32_t)COLL_N);
  CHECK(drain(bm, 0, COLL_KEYS[COLL_N - 1] + 1) == COLL_N + 1);
}
static void test_scale(void) {
  OspreyMap *m = make_int_map(0, SCALE_N);
  chk_int_range(m, 0, SCALE_N); CHECK(drain(m, 0, SCALE_N) == SCALE_N);
  /* Remove every odd key; the evens stay intact and the source survives. */
  OspreyMap *half = m;
  for (int64_t i = 1; i < SCALE_N; i += 2) half = osprey_map_remove(half, i);
  CHECK(osprey_map_length(half) == SCALE_N / 2);
  for (int64_t i = 0; i < SCALE_N; i++)
    CHECK(osprey_map_contains(half, i) == ((i % 2 == 0) ? 1 : 0));
  for (int64_t i = 0; i < SCALE_N; i += 2)
    CHECK(osprey_map_get(half, i) == i * VAL_MUL);
  chk_int_range(m, 0, SCALE_N);
  OspreyMap *hi = make_int_map(MERGE_LO, MERGE_HI); /* two large maps merged */
  OspreyMap *big = osprey_map_merge(m, hi);
  chk_int_range(big, 0, MERGE_HI); chk_int_range(m, 0, SCALE_N);
  chk_int_range(hi, MERGE_LO, MERGE_HI);
  CHECK(drain(big, 0, MERGE_HI) == MERGE_HI);
}
void run_map_tests(void) {
  test_hash_and_equality();
  test_node_algebra();
  test_empty_and_single();
  test_int_keys_persistent();
  test_remove_paths();
  test_string_keys();
  test_string_aliases();
  test_bool_keys();
  test_collision_structure();
  test_collision_update_and_insert();
  test_collision_removal();
  test_merge();
  test_merge_collision();
  test_builder();
  test_builder_key_types();
  test_scale();
  printf("[ok] map: %llu assertions\n", (unsigned long long)g_asserts);
}
#ifndef OSPREY_NO_TEST_MAIN
int main(void) {
  run_map_tests();
  return 0;
}
#endif
