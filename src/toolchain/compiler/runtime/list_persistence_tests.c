// Persistence under sharing, a randomised differential run against the flat
// model, the builder path alone, and the element-kind surface. Part of the
// list_tests suite; see list_tests_shared.h.
#include "list_tests_shared.h"

/* Sharing: many independent versions derived from one ancestor must all stay
   valid at once (path copying + node refcounting). */
#define BRANCH_FAN 24
#define BRANCH_N 200
#define BRANCH_STRIDE 7

void test_branching_persistence(void) {
  const int64_t n = BRANCH_N, fan = BRANCH_FAN;
  OspreyList *base = make_range_list(0, n);
  int64_t *m = model_range(0, n);
  OspreyList *kids[BRANCH_FAN];
  for (int64_t i = 0; i < fan; i++) {
    kids[i] = osprey_list_append(
        osprey_list_set(base, i * BRANCH_STRIDE, SET_SENTINEL - i),
        APPEND_SENTINEL - i);
  }
  for (int64_t i = 0; i < fan; i++) {
    int64_t *e = join_model(m, n, NULL, 0);
    e[i * BRANCH_STRIDE] = SET_SENTINEL - i;
    e[n] = APPEND_SENTINEL - i;
    check_elems(kids[i], e, n + 1);
    free(e);
  }
  check_elems(base, m, n);
  free(m);
}

/* Randomised differential run against a flat-array model. */
#define RAND_OPS 400
#define RAND_OP_KINDS 5
#define RAND_VALUE_SPAN 1000
#define RAND_SEED 0x9E3779B97F4A7C15ULL
#define LCG_MUL 6364136223846793005ULL
#define LCG_ADD 1442695040888963407ULL

static uint64_t rng_next(uint64_t *s) {
  *s = *s * LCG_MUL + LCG_ADD; /* unsigned: wraps, never traps under -ftrapv */
  return *s >> 33;
}

static int64_t rand_step(OspreyList **l, int64_t *m, int64_t n, uint64_t r) {
  int64_t v = (int64_t)(r % RAND_VALUE_SPAN) - RAND_VALUE_SPAN / 2;
  int64_t i = n > 0 ? (int64_t)(r % (uint64_t)n) : 0;
  switch (r % RAND_OP_KINDS) {
  case 0:
    *l = osprey_list_append(*l, v);
    m[n] = v;
    return n + 1;
  case 1:
    *l = osprey_list_prepend(*l, v);
    memmove(m + 1, m, (size_t)n * sizeof(int64_t));
    m[0] = v;
    return n + 1;
  case 2:
    if (n == 0) return n;
    *l = osprey_list_set(*l, i, v);
    m[i] = v;
    return n;
  case 3:
    if (n == 0) return n;
    *l = osprey_list_drop(*l, i + 1);
    memmove(m, m + i + 1, (size_t)(n - i - 1) * sizeof(int64_t));
    return n - i - 1;
  default:
    *l = osprey_list_reverse(*l);
    reverse_model(m, n);
    return n;
  }
}

void test_random_ops(void) {
  uint64_t seed = RAND_SEED;
  int64_t *m = model_range(0, RAND_OPS + MODEL_SLACK);
  int64_t n = 0;
  OspreyList *l = osprey_list_empty();
  for (int op = 0; op < RAND_OPS; op++) {
    n = rand_step(&l, m, n, rng_next(&seed));
    check_elems(l, m, n);
  }
  CHECK(n >= 0);
  free(m);
}

/* The builder path in isolation: transient pushes, then seal. */
void test_builder_paths(void) {
  for (size_t k = 0; k < NELEMS(GROWTH_SIZES); k++) {
    int64_t n = GROWTH_SIZES[k];
    if (n > APPEND_BUILD_LIMIT) continue;
    OspreyListBuilder *b = osprey_list_builder_new();
    CHECK(b != NULL);
    for (int64_t i = 0; i < n; i++) {
      osprey_list_builder_push(b, i * 3 - 1);
    }
    OspreyList *l = osprey_list_builder_seal(b);
    int64_t *m = model_range(0, n);
    for (int64_t i = 0; i < n; i++) {
      m[i] = i * 3 - 1;
    }
    check_elems(l, m, n);
    check_appends(l, m, n); /* seal then keep growing */
    free(m);
  }
}

/* The element-kind surface [MEM-BACKENDS-ELEMENTS]: `_of` spellings latch the
   managed flag at the call site, the arity-2 forms inherit it from the source,
   every derived container carries it forward, builders latch it at
   construction or first push, and the borrowed push stores the same word. */
void test_element_kind_flags(void) {
  OspreyList *e = osprey_list_empty();
  CHECK(osprey_list_elem_managed(e) == 0); /* empty source means "scalar" */
  OspreyList *managed = osprey_list_append_of(e, 41, 1);
  CHECK(osprey_list_elem_managed(managed) == 1);
  CHECK(osprey_list_get(managed, 0) == 41);
  CHECK(osprey_list_elem_managed(e) == 0); /* the singleton is never stamped */
  OspreyList *scalar = osprey_list_append_of(e, 42, 0);
  CHECK(osprey_list_elem_managed(scalar) == 0);
  OspreyList *inherit = osprey_list_append(managed, 43);
  CHECK(osprey_list_elem_managed(inherit) == 1); /* arity-2 inherits */
  CHECK(osprey_list_get(inherit, 1) == 43);
  OspreyList *pre = osprey_list_prepend_of(e, 40, 1);
  CHECK(osprey_list_elem_managed(pre) == 1 && osprey_list_get(pre, 0) == 40);
  OspreyList *pre2 = osprey_list_prepend(pre, 39);
  CHECK(osprey_list_elem_managed(pre2) == 1 && osprey_list_get(pre2, 0) == 39);
  CHECK(osprey_list_elem_managed(osprey_list_set(inherit, 0, 7)) == 1);
  CHECK(osprey_list_elem_managed(osprey_list_reverse(inherit)) == 1);
  CHECK(osprey_list_elem_managed(osprey_list_drop(inherit, 1)) == 1);
  CHECK(osprey_list_elem_managed(osprey_list_concat(managed, inherit)) == 1);
  OspreyListBuilder *b1 = osprey_list_builder_new_of(1);
  CHECK(b1 != NULL);
  osprey_list_builder_push(b1, 1);
  osprey_list_builder_push_borrowed(b1, 2); /* dup-first path, same word */
  OspreyList *s1 = osprey_list_builder_seal(b1);
  CHECK(osprey_list_elem_managed(s1) == 1);
  CHECK(osprey_list_length(s1) == 2 && osprey_list_get(s1, 1) == 2);
  OspreyListBuilder *b2 = osprey_list_builder_new();
  CHECK(b2 != NULL);
  osprey_list_builder_push_of(b2, 5, 1); /* latches on first push */
  OspreyList *s2 = osprey_list_builder_seal(b2);
  CHECK(osprey_list_elem_managed(s2) == 1 && osprey_list_get(s2, 0) == 5);
}
