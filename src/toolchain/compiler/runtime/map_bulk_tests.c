// Bulk map operations: merge (with collisions), the builder over every key
// type, scale, the entry-ownership flags and the iterator lifecycle. Part of
// the map_tests suite; see map_tests_shared.h.
#include "map_tests_shared.h"

void test_merge(void) {
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
void test_merge_collision(void) {
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
void test_builder(void) {
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
void test_builder_key_types(void) {
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
void test_scale(void) {
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
/* The entry-ownership flag surface [MEM-BACKENDS-ELEMENTS]: the KEY side is
   discriminated by the key type (STRING managed, INT/BOOL scalar), the VALUE
   side latches through the `_of` spellings, the arity-3 forms inherit it from
   the source, and every derived map carries it forward. */
void test_entry_kind_flags(void) {
  CHECK(osprey_map_key_managed(osprey_map_empty(KI)) == 0);
  CHECK(osprey_map_key_managed(osprey_map_empty(KB)) == 0);
  CHECK(osprey_map_key_managed(osprey_map_empty(KS)) == 1);
  OspreyMap *e = osprey_map_empty(KI);
  CHECK(osprey_map_value_managed(e) == 0);
  OspreyMap *mv = osprey_map_set_of(e, 1, 10, 1);
  CHECK(osprey_map_value_managed(mv) == 1);
  CHECK(osprey_map_get(mv, 1) == 10);
  CHECK(osprey_map_value_managed(e) == 0); /* the source is never stamped */
  OspreyMap *inherit = osprey_map_set(mv, 2, 20); /* arity-3 inherits */
  CHECK(osprey_map_value_managed(inherit) == 1);
  CHECK(osprey_map_length(inherit) == 2 && osprey_map_get(inherit, 2) == 20);
  CHECK(osprey_map_value_managed(osprey_map_remove(inherit, 1)) == 1);
  CHECK(osprey_map_value_managed(osprey_map_merge(mv, inherit)) == 1);
  OspreyMapBuilder *b = osprey_map_builder_new(KI);
  CHECK(b != NULL);
  osprey_map_builder_put_of(b, 1, 100, 1); /* latches on first put */
  osprey_map_builder_put(b, 2, 200);
  OspreyMap *sealed = osprey_map_builder_seal(b);
  CHECK(osprey_map_value_managed(sealed) == 1);
  CHECK(osprey_map_get(sealed, 1) == 100 && osprey_map_get(sealed, 2) == 200);
}

/* [BUILTIN-MAP-KEYS]/[BUILTIN-MAP-VALUES] cursors: a full walk yields every
   entry exactly once, exhaustion is stable, and the dedicated free entry
   point releases the cursor (codegen must call it once per finished walk). */
void test_iter_lifecycle(void) {
  OspreyMap *m = osprey_map_empty(KI);
  for (int64_t k = 0; k < PREFIX_N; k++) {
    m = osprey_map_set(m, k, k * VAL_MUL);
  }
  OspreyMapIter *it = osprey_map_iter_new(m);
  CHECK(it != NULL);
  int64_t key = 0;
  int64_t value = 0;
  int64_t seen_mask = 0;
  int64_t seen = 0;
  while (osprey_map_iter_next(it, &key, &value) == 1) {
    CHECK(key >= 0 && key < PREFIX_N);
    CHECK(value == key * VAL_MUL);
    CHECK((seen_mask & ((int64_t)1 << key)) == 0); /* no duplicates */
    seen_mask |= (int64_t)1 << key;
    seen++;
  }
  CHECK(seen == PREFIX_N);
  CHECK(osprey_map_iter_next(it, &key, &value) == 0); /* stays exhausted */
  osprey_map_iter_free(it);
  osprey_map_iter_free(NULL); /* tolerated, like free(NULL) */
}
