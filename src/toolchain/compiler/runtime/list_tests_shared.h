// Shared by the translation units of the list_tests suite: the assertion
// counter, the structural sizes every case walks, and the flat-array model and
// checks each list is compared against after every derived operation.
#ifndef OSPREY_LIST_TESTS_SHARED_H
#define OSPREY_LIST_TESTS_SHARED_H

#include "collection_runtime.h"

#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

extern int64_t g_checks;
#define CHECK(cond)                                                            \
  do {                                                                         \
    g_checks++;                                                                \
    assert(cond);                                                              \
  } while (0)
#define NELEMS(a) (sizeof(a) / sizeof((a)[0]))

/* list_sort_tests.c: the listSort half of the suite, counted here. */
int64_t run_list_sort_tests(void);

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
/* Front growth: three levels of room opened before physical index 0 (32, 1024,
   32768), with an append every PREPEND_APPEND_EVERY prepends. */
#define DEEP_PREPEND_N 33900
#define PREPEND_APPEND_EVERY 7
#define PREPEND_VIEW_STEP 4099
/* A prepend path-copies O(log32 n) nodes; rebuilding the list per prepend made
   this many take minutes of CPU, so the bound has two orders of slack. */
#define PREPEND_SCALE_N 300000
#define PREPEND_SCALE_CPU_SECONDS 10

/* Test helpers ----------------------------------------------------- */
static inline OspreyList *make_range_list(int64_t start, int64_t end) {
  OspreyListBuilder *b = osprey_list_builder_new();
  for (int64_t i = start; i < end; i++) osprey_list_builder_push(b, i);
  return osprey_list_builder_seal(b);
}

static inline OspreyList *append_range_list(int64_t n) {
  OspreyList *l = osprey_list_empty();
  for (int64_t i = 0; i < n; i++) l = osprey_list_append(l, i);
  return l;
}

/* Models are flat arrays with MODEL_SLACK spare cells, so appends never realloc. */
static inline int64_t *new_model(int64_t n) {
  /* calloc, not malloc: zero-fills the model buffer so an n==0 range still
     yields provably-defined memory. Without it, gcc -O2 -Wmaybe-uninitialized
     false-positives on the inlined empty-range paths (check_range at n==0),
     reading the model pointer as possibly uninitialized. */
  int64_t *m = (int64_t *)calloc((size_t)(n + MODEL_SLACK), sizeof(int64_t));
  CHECK(m != NULL);
  return m;
}
static inline int64_t *join_model(const int64_t *a, int64_t an, const int64_t *b,
                           int64_t bn) {
  int64_t *m = new_model(an + bn);
  if (an > 0) memcpy(m, a, (size_t)an * sizeof(int64_t));
  if (bn > 0) memcpy(m + an, b, (size_t)bn * sizeof(int64_t));
  return m;
}

static inline int64_t *model_range(int64_t start, int64_t n) {
  int64_t *m = new_model(n);
  for (int64_t i = 0; i < n; i++) m[i] = start + i;
  return m;
}

static inline void reverse_model(int64_t *m, int64_t n) {
  for (int64_t i = 0, t = 0; i < n / 2; i++) {
    t = m[i], m[i] = m[n - 1 - i], m[n - 1 - i] = t;
  }
}

static inline void check_iter(OspreyList *l, const int64_t *m, int64_t n) {
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
static inline void check_elems(OspreyList *l, const int64_t *m, int64_t n) {
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

static inline void check_range(OspreyList *l, int64_t start, int64_t n) {
  int64_t *m = model_range(start, n);
  check_elems(l, m, n);
  free(m);
}

static inline void check_sets(OspreyList *l, const int64_t *m, int64_t n) {
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

/* Appends past the end of a (possibly shared/view) list; source must survive. */
static inline void check_appends(OspreyList *l, const int64_t *m, int64_t n) {
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
static inline void check_view_at(OspreyList *l, const int64_t *m, int64_t n, int64_t k,
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

// list_persistence_tests.c
void test_branching_persistence(void);
void test_random_ops(void);
void test_builder_paths(void);
void test_element_kind_flags(void);

#endif
