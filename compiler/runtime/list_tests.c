#include "collection_runtime.h"

#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/*
 * Vanilla-C suite for list_runtime.c — the immutable bitmapped vector trie
 * behind List<T>. assert()-driven: any failure aborts with a non-zero exit.
 *
 * Covers [TYPE-LIST], [TYPE-LIST-OPS] and the container node/element
 * refcounting of [MEM-BACKENDS] / [MEM-BACKENDS-ELEMENTS]
 * (docs/specs/0018-MemoryManagement.md):
 *   - persistence: every source is re-verified in full after every derived op
 *   - the O(1) drop VIEW (offset + length == physical count): get/set/append/
 *     length/drop on a view, a view of a view, one inside the shared tail
 *   - trie growth across every level boundary, append path and builder path
 *   - singleton identity + aliased (retain-on-return) results
 *   - boundary/error arguments: NULL lists, negative and past-end indices
 *   - a randomised differential run against a flat-array model
 *
 * Every check goes through CHECK() so the suite reports its assertion count.
 */

static int64_t g_checks = 0;
#define CHECK(cond)                                                            \
  do {                                                                         \
    g_checks++;                                                                \
    assert(cond);                                                              \
  } while (0)
#define NELEMS(a) (sizeof(a) / sizeof((a)[0]))

/* Counts straddling every structural boundary: tail full (32), first push into
   the tree (33), root internal (64/65), level-1 (1024/1025), first NULL-child
   wrap_path at level 10 (2081), level-2 (32768/32801). */
static const int64_t GROWTH_SIZES[] = {0,    1,    2,    31,   32,    33,
                                       63,   64,   65,   96,   1023,  1024,
                                       1025, 2081, 32767, 32768, 32769, 32801};
/* Above this the builder path covers the sizes (plus one deep list below). */
#define APPEND_BUILD_LIMIT 2100
#define DEEP_APPEND_N 32801

static const int64_t VIEW_SIZES[] = {1,  2,  32,   33,   34,   63,  64,
                                     65, 96, 1023, 1024, 1025, 1057};
static const int64_t VIEW_DROPS[] = {1, 2, 31, 32, 33, 34, 63, 64, 65, 1022};
/* Views no larger than this also get the exhaustive append/set drilling. */
#define DEEP_VIEW_LIMIT 96
/* > 2 * OSPREY_LIST_BRANCH: crosses the tail boundary from any starting fill. */
#define VIEW_APPEND_COUNT 70
#define MODEL_SLACK (VIEW_APPEND_COUNT + 1)

static const int64_t CAT_SIZES[] = {0, 1, 31, 32, 33, 64, 65, 100};
#define CONCAT_B_BASE 1000000
#define SET_SENTINEL (-1000000)
#define OOB_SET_INDEX 100000 /* past every list here, and past one node's 32 */
#define APPEND_SENTINEL (-2000000)
#define EXHAUSTIVE_N 100
#define PREPEND_N 100

/* Test helpers ----------------------------------------------------- */
static OspreyList *make_range_list(int64_t start, int64_t end) {
  OspreyListBuilder *b = osprey_list_builder_new();
  for (int64_t i = start; i < end; i++) osprey_list_builder_push(b, i);
  return osprey_list_builder_seal(b);
}

static OspreyList *append_range_list(int64_t n) {
  OspreyList *l = osprey_list_empty();
  for (int64_t i = 0; i < n; i++) l = osprey_list_append(l, i);
  return l;
}

/* Models are flat arrays with MODEL_SLACK spare cells, so appends never realloc. */
static int64_t *new_model(int64_t n) {
  int64_t *m = (int64_t *)malloc((size_t)(n + MODEL_SLACK) * sizeof(int64_t));
  CHECK(m != NULL);
  return m;
}
static int64_t *join_model(const int64_t *a, int64_t an, const int64_t *b,
                           int64_t bn) {
  int64_t *m = new_model(an + bn);
  if (an > 0) memcpy(m, a, (size_t)an * sizeof(int64_t));
  if (bn > 0) memcpy(m + an, b, (size_t)bn * sizeof(int64_t));
  return m;
}

static int64_t *model_range(int64_t start, int64_t n) {
  int64_t *m = new_model(n);
  for (int64_t i = 0; i < n; i++) m[i] = start + i;
  return m;
}

static void reverse_model(int64_t *m, int64_t n) {
  for (int64_t i = 0, t = 0; i < n / 2; i++) {
    t = m[i], m[i] = m[n - 1 - i], m[n - 1 - i] = t;
  }
}

static void check_iter(OspreyList *l, const int64_t *m, int64_t n) {
  OspreyListIter *it = osprey_list_iter_new(l);
  CHECK(it != NULL);
  int64_t v = SET_SENTINEL;
  for (int64_t i = 0; i < n; i++) {
    CHECK(osprey_list_iter_next(it, &v) == 1);
    CHECK(v == m[i]);
  }
  CHECK(osprey_list_iter_next(it, &v) == 0);
  CHECK(osprey_list_iter_next(it, &v) == 0); /* exhaustion is stable */
  free(it);
}

/* The workhorse: length, in-bounds + value at every index, every OOB answer,
   and a full iteration. */
static void check_elems(OspreyList *l, const int64_t *m, int64_t n) {
  const int64_t oob[] = {-1, -2, n, n + 1, INT64_MIN, INT64_MAX};
  CHECK(osprey_list_length(l) == n);
  for (size_t k = 0; k < NELEMS(oob); k++) {
    CHECK(osprey_list_in_bounds(l, oob[k]) == 0);
    CHECK(osprey_list_get(l, oob[k]) == 0); /* hardened OOB get returns 0 */
  }
  for (int64_t i = 0; i < n; i++) {
    CHECK(osprey_list_in_bounds(l, i) == 1);
    CHECK(osprey_list_get(l, i) == m[i]);
  }
  check_iter(l, m, n);
}

static void check_range(OspreyList *l, int64_t start, int64_t n) {
  int64_t *m = model_range(start, n);
  check_elems(l, m, n);
  free(m);
}

/* Tests ------------------------------------------------------------ */
static void test_empty_singleton(void) {
  OspreyList *e = osprey_list_empty();
  CHECK(e != NULL);
  CHECK(osprey_list_empty() == e); /* one immortal singleton, forever */
  check_range(e, 0, 0);
  CHECK(osprey_list_drop(e, 0) == e);
  CHECK(osprey_list_drop(e, 1) == e);
  CHECK(osprey_list_drop(e, INT64_MAX) == e);
  CHECK(osprey_list_concat(e, e) == e);
  CHECK(osprey_list_concat(NULL, NULL) == e);
  CHECK(osprey_list_reverse(e) == e);
  CHECK(osprey_list_reverse(NULL) == e);
  CHECK(osprey_list_builder_seal(osprey_list_builder_new()) == e);
  CHECK(osprey_list_drop(make_range_list(0, 40), 40) == e);
  check_range(e, 0, 0); /* still pristine after all of the above */
}

static void test_null_arguments(void) {
  CHECK(osprey_list_length(NULL) == 0);
  CHECK(osprey_list_in_bounds(NULL, 0) == 0);
  CHECK(osprey_list_in_bounds(NULL, -1) == 0);
  CHECK(osprey_list_in_bounds(NULL, INT64_MAX) == 0);
  CHECK(osprey_list_get(NULL, 0) == 0);
  CHECK(osprey_list_get(NULL, -5) == 0);
  CHECK(osprey_list_get(NULL, INT64_MAX) == 0);
  CHECK(osprey_list_drop(NULL, 0) == NULL); /* alias return of NULL */
  CHECK(osprey_list_drop(NULL, 7) == NULL);
  int64_t one = 5;
  check_elems(osprey_list_append(NULL, one), &one, 1);
  check_elems(osprey_list_prepend(NULL, one), &one, 1);
  OspreyList *a = make_range_list(0, 5);
  CHECK(osprey_list_concat(NULL, a) == a);
  CHECK(osprey_list_concat(a, NULL) == a);
  check_range(a, 0, 5);
  OspreyListIter *it = osprey_list_iter_new(NULL);
  int64_t v = 0;
  CHECK(osprey_list_iter_next(it, &v) == 0);
  free(it);
  /* Out-of-contract set: an empty list has no in-bounds index, and a past-end
     or negative one would subscript past a 32-slot node. Degrades like the
     hardened osprey_list_get — unchanged list back — instead of crashing. */
  CHECK(osprey_list_length(osprey_list_set(osprey_list_empty(), 0, 9)) == 0);
  CHECK(osprey_list_set(NULL, 0, 9) == NULL);
  CHECK(osprey_list_set(a, OOB_SET_INDEX, 9) == a);
  CHECK(osprey_list_set(a, -1, 9) == a);
  check_range(a, 0, 5);
  check_range(osprey_list_empty(), 0, 0);
}

/* Every level boundary, built both ways; builder and append must agree
   element for element. */
static void test_growth_sizes(void) {
  for (size_t k = 0; k < NELEMS(GROWTH_SIZES); k++) {
    int64_t n = GROWTH_SIZES[k];
    check_range(make_range_list(0, n), 0, n);
    if (n <= APPEND_BUILD_LIMIT) check_range(append_range_list(n), 0, n);
  }
  /* One deep append-built list: grow_root level-2 stacking + wrap_path. */
  check_range(append_range_list(DEEP_APPEND_N), 0, DEEP_APPEND_N);
}

/* Set every index in turn: one slot differs, the source stays identical. */
static void test_set_exhaustive(void) {
  OspreyList *base = make_range_list(0, EXHAUSTIVE_N);
  int64_t *m = model_range(0, EXHAUSTIVE_N);
  for (int64_t i = 0; i < EXHAUSTIVE_N; i++) {
    OspreyList *s = osprey_list_set(base, i, SET_SENTINEL - i);
    m[i] = SET_SENTINEL - i;
    check_elems(s, m, EXHAUSTIVE_N);
    m[i] = i;
    check_elems(base, m, EXHAUSTIVE_N);
  }
  free(m);
}

static void check_sets(OspreyList *l, const int64_t *m, int64_t n) {
  int64_t *e = join_model(m, n, NULL, 0);
  for (int64_t i = 0; i < n; i++) {
    OspreyList *s = osprey_list_set(l, i, SET_SENTINEL - i);
    e[i] = SET_SENTINEL - i;
    check_elems(s, e, n);
    e[i] = m[i];
    check_elems(l, m, n); /* persistence of the source */
  }
  free(e);
}

/* Set at both sides of the tail boundary for every structural size. */
static void test_set_boundaries(void) {
  for (size_t k = 0; k < NELEMS(GROWTH_SIZES); k++) {
    int64_t n = GROWTH_SIZES[k];
    if (n == 0 || n > APPEND_BUILD_LIMIT) continue;
    OspreyList *l = make_range_list(0, n);
    int64_t *m = model_range(0, n);
    int64_t idx[] = {0, n / 2, n - 1, n - (n % OSPREY_LIST_BRANCH) - 1};
    for (size_t j = 0; j < NELEMS(idx); j++) {
      int64_t i = idx[j] < 0 ? 0 : idx[j];
      OspreyList *s = osprey_list_set(l, i, SET_SENTINEL);
      int64_t old = m[i];
      m[i] = SET_SENTINEL;
      check_elems(s, m, n);
      m[i] = old;
      check_elems(l, m, n);
    }
    free(m);
  }
}

/* Appends past the end of a (possibly shared/view) list; source must survive. */
static void check_appends(OspreyList *l, const int64_t *m, int64_t n) {
  int64_t *e = join_model(m, n, NULL, 0);
  OspreyList *cur = l;
  for (int64_t i = 0; i < VIEW_APPEND_COUNT; i++) {
    e[n + i] = APPEND_SENTINEL - i;
    cur = osprey_list_append(cur, e[n + i]);
    check_elems(cur, e, n + i + 1);
  }
  check_elems(l, m, n);
  free(e);
}

/* The O(1) view contract, for one list and one drop position. */
static void check_view_at(OspreyList *l, const int64_t *m, int64_t n, int64_t k,
                          int deep) {
  int64_t vn = n - k;
  OspreyList *v = osprey_list_drop(l, k);
  check_elems(v, m + k, vn);
  check_elems(l, m, n); /* dropping never disturbs the source */
  CHECK(osprey_list_drop(v, 0) == v);
  CHECK(osprey_list_drop(v, -1) == v);
  CHECK(osprey_list_drop(v, vn) == osprey_list_empty());
  CHECK(osprey_list_drop(v, vn + 1) == osprey_list_empty());
  OspreyList *vv = osprey_list_drop(v, vn / 2); /* view of a view */
  check_elems(vv, m + k + vn / 2, vn - vn / 2);
  check_elems(v, m + k, vn);
  if (deep) {
    check_appends(v, m + k, vn);
    check_sets(v, m + k, vn);
    check_elems(l, m, n);
  }
}

static void test_views(void) {
  for (size_t si = 0; si < NELEMS(VIEW_SIZES); si++) {
    int64_t n = VIEW_SIZES[si];
    int64_t *m = model_range(0, n);
    OspreyList *l = make_range_list(0, n);
    CHECK(osprey_list_drop(l, 0) == l);
    for (size_t di = 0; di < NELEMS(VIEW_DROPS); di++) {
      int64_t k = VIEW_DROPS[di];
      if (k >= n) continue;
      check_view_at(l, m, n, k, n <= DEEP_VIEW_LIMIT);
    }
    check_view_at(l, m, n, n - 1, n <= DEEP_VIEW_LIMIT); /* last element only */
    free(m);
  }
}

/* concat / reverse / prepend applied to a view whose offset lands inside the
   tail chunk of a 70-element list (tail spans physical 64..69). */
static void test_view_derived_ops(void) {
  const int64_t n = 70, k = 66;
  int64_t vn = n - k;
  OspreyList *v = osprey_list_drop(make_range_list(0, n), k);
  int64_t *m = model_range(k, vn);
  check_elems(v, m, vn);
  int64_t *cm = join_model(m, vn, m, vn);
  check_elems(osprey_list_concat(v, v), cm, vn * 2);
  free(cm);
  int64_t *rm = join_model(m, vn, NULL, 0);
  reverse_model(rm, vn);
  check_elems(osprey_list_reverse(v), rm, vn);
  free(rm);
  int64_t head = SET_SENTINEL;
  int64_t *pm = join_model(&head, 1, m, vn);
  check_elems(osprey_list_prepend(v, head), pm, vn + 1);
  free(pm);
  check_elems(v, m, vn); /* the view survived every derivation */
  free(m);
}

static void test_concat_matrix(void) {
  for (size_t i = 0; i < NELEMS(CAT_SIZES); i++) {
    for (size_t j = 0; j < NELEMS(CAT_SIZES); j++) {
      int64_t x = CAT_SIZES[i], y = CAT_SIZES[j];
      int64_t *ma = model_range(0, x), *mb = model_range(CONCAT_B_BASE, y);
      OspreyList *a = make_range_list(0, x);
      OspreyList *b = make_range_list(CONCAT_B_BASE, CONCAT_B_BASE + y);
      OspreyList *c = osprey_list_concat(a, b);
      int64_t *mc = join_model(ma, x, mb, y);
      check_elems(c, mc, x + y);
      check_elems(a, ma, x); /* both inputs untouched */
      check_elems(b, mb, y);
      if (x == 0) CHECK(c == b);      /* aliased empty-left return */
      else if (y == 0) CHECK(c == a); /* aliased empty-right return */
      free(ma);
      free(mb);
      free(mc);
    }
  }
}

static void test_reverse(void) {
  for (size_t k = 0; k < NELEMS(GROWTH_SIZES); k++) {
    int64_t n = GROWTH_SIZES[k];
    if (n > APPEND_BUILD_LIMIT) continue;
    OspreyList *l = make_range_list(0, n);
    int64_t *m = model_range(0, n);
    int64_t *rm = join_model(m, n, NULL, 0);
    reverse_model(rm, n);
    OspreyList *r = osprey_list_reverse(l);
    check_elems(r, rm, n);
    check_elems(l, m, n);
    check_elems(osprey_list_reverse(r), m, n); /* involution */
    free(m);
    free(rm);
  }
}

static void test_prepend(void) {
  OspreyList *l = osprey_list_empty();
  int64_t *m = new_model(PREPEND_N);
  for (int64_t i = 0; i < PREPEND_N; i++) {
    l = osprey_list_prepend(l, i);
    memmove(m + 1, m, (size_t)i * sizeof(int64_t));
    m[0] = i;
    check_elems(l, m, i + 1);
  }
  /* Prepending never disturbs the version it was derived from. */
  OspreyList *p = osprey_list_prepend(l, SET_SENTINEL);
  CHECK(osprey_list_get(p, 0) == SET_SENTINEL);
  check_elems(l, m, PREPEND_N);
  free(m);
}

/* Sharing: many independent versions derived from one ancestor must all stay
   valid at once (path copying + node refcounting). */
#define BRANCH_FAN 24
#define BRANCH_N 200
#define BRANCH_STRIDE 7

static void test_branching_persistence(void) {
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

static void test_random_ops(void) {
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
static void test_builder_paths(void) {
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

void run_list_tests(void) {
  test_empty_singleton();
  test_null_arguments();
  test_growth_sizes();
  test_set_exhaustive();
  test_set_boundaries();
  test_views();
  test_view_derived_ops();
  test_concat_matrix();
  test_reverse();
  test_prepend();
  test_branching_persistence();
  test_random_ops();
  test_builder_paths();
}

/* collection_tests.c supplies its own main and calls run_list_tests(); define
   OSPREY_TESTS_EXTERNAL_MAIN when linking against it. */
#ifndef OSPREY_TESTS_EXTERNAL_MAIN
int main(void) {
  run_list_tests();
  printf("[ok] list_runtime: %lld assertions\n", (long long)g_checks);
  return 0;
}
#endif
