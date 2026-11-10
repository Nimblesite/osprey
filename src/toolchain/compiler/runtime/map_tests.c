#include "map_runtime_internal.h"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
/*
 * Vanilla-C src/osprey/tests for map_runtime.c + map_runtime_hamt.c. assert()-driven,
 * non-zero exit on failure. Build (all warnings are errors):
 *   cc -O2 -D_FORTIFY_SOURCE=2 -fstack-protector-strong -Werror -Wall -Wextra \
 *      -ftrapv -std=c11 -D_GNU_SOURCE -o map_tests \
 *      map_tests.c map_runtime.c map_runtime_hamt.c memory_runtime.c
 * Covers the runtime half of the map builtins [BUILTIN-MAP], [BUILTIN-MAP-GET],
 * [BUILTIN-MAP-SET], [BUILTIN-MAP-REMOVE], [BUILTIN-MAP-MERGE],
 * [BUILTIN-MAP-CONTAINS], [BUILTIN-COLLECTION-LENGTH] and the iteration behind
 * [BUILTIN-MAP-KEYS] / [BUILTIN-MAP-VALUES]
 * (docs/specs/runtime/0012-Built-InFunctions.md), plus
 * [TYPE-MAP] / [TYPE-MAP-LOOKUP] / [TYPE-MAP-OPS] and container ownership
 * [MEM-BACKENDS-ELEMENTS] / [GC-ARC-PERCEUS] for INT / STRING / BOOL keys:
 * persistence of every source map, pointer-identical alias returns, all three
 * node kinds asserted structurally, every collision / internal-node branch,
 * iteration, the builder, and a 20k-key scale pass.
 * BOOL keys hash to two buckets (0, 1) while equality stays `==`, so distinct
 * non-zero BOOL keys are a full 32-bit hash collision: that is how
 * NODE_COLLISION is forced, under CHAIN_LEVELS single-child internals — the
 * deepest spine a 32-bit / 5-bit HAMT builds, and exactly the iterator stack.
 */
#include "map_tests_shared.h"

uint64_t g_asserts;

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
  test_entry_kind_flags();
  test_iter_lifecycle();
  printf("[ok] map: %llu assertions\n", (unsigned long long)g_asserts);
}
#ifndef OSPREY_NO_TEST_MAIN
int main(void) {
  run_map_tests();
  return 0;
}
#endif
