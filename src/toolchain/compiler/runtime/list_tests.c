#include "collection_runtime.h"

#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

/*
 * Vanilla-C suite for list_runtime.c — the immutable bitmapped vector trie
 * behind List<T>. assert()-driven: any failure aborts with a non-zero exit.
 *
 * Covers the runtime half of the list builtins [BUILTIN-LIST],
 * [BUILTIN-LIST-GET], [BUILTIN-LIST-APPEND], [BUILTIN-LIST-PREPEND],
 * [BUILTIN-LIST-CONCAT], [BUILTIN-LIST-REVERSE], [BUILTIN-COLLECTION-LENGTH]
 * (docs/specs/runtime/0012-Built-InFunctions.md), plus
 * [TYPE-LIST], [TYPE-LIST-OPS] and the container node/element
 * refcounting of [MEM-BACKENDS] / [MEM-BACKENDS-ELEMENTS]
 * (docs/specs/compiler/0018-MemoryManagement.md):
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

#include "list_tests_shared.h"

int64_t g_checks = 0;

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
  /* A prepend-built list keeps its newest block in the front chunk: every set
     and every view of it (with that view's own appends and sets) still holds. */
  check_sets(l, m, PREPEND_N);
  for (int64_t k = 1; k < PREPEND_N; k++) check_view_at(l, m, PREPEND_N, k, 1);
  for (int64_t k = 1; k < PREPEND_N; k++) {
    OspreyList *v = osprey_list_prepend(osprey_list_drop(l, k), SET_SENTINEL);
    CHECK(osprey_list_get(v, 0) == SET_SENTINEL);
    check_elems(osprey_list_drop(v, 1), m + k, PREPEND_N - k);
  }
  check_elems(l, m, PREPEND_N);
  free(m);
}

/* Prepends interleaved with appends across every front-growth boundary, kept
   as a deque model: `front` grows downward from `mid`, `back` upward. */
static void test_prepend_deep(void) {
  int64_t cap = DEEP_PREPEND_N + MODEL_SLACK;
  int64_t *m = new_model(2 * cap);
  int64_t lo = cap, hi = cap;
  OspreyList *l = osprey_list_empty();
  for (int64_t i = 0; i < DEEP_PREPEND_N; i++) {
    if (i % PREPEND_APPEND_EVERY == 0) {
      l = osprey_list_append(l, APPEND_SENTINEL - i);
      m[hi++] = APPEND_SENTINEL - i;
    } else {
      l = osprey_list_prepend(l, i);
      m[--lo] = i;
    }
    for (size_t k = 0; k < NELEMS(GROWTH_SIZES); k++) {
      if (hi - lo == GROWTH_SIZES[k]) check_elems(l, m + lo, hi - lo);
    }
  }
  int64_t n = hi - lo;
  check_elems(l, m + lo, n);
  /* Views of the prepended list take sets, appends and prepends of their own;
     the source survives each one. */
  for (int64_t k = 1; k < n; k += PREPEND_VIEW_STEP) {
    OspreyList *v = osprey_list_drop(l, k);
    check_elems(v, m + lo + k, n - k);
    OspreyList *s = osprey_list_set(v, 0, SET_SENTINEL);
    CHECK(osprey_list_get(s, 0) == SET_SENTINEL);
    CHECK(osprey_list_get(s, n - k - 1) == m[hi - 1]);
    OspreyList *p = osprey_list_prepend(v, SET_SENTINEL - k);
    CHECK(osprey_list_length(p) == n - k + 1);
    CHECK(osprey_list_get(p, 0) == SET_SENTINEL - k);
    CHECK(osprey_list_get(p, 1) == m[lo + k]);
    check_appends(v, m + lo + k, n - k);
    check_elems(v, m + lo + k, n - k);
  }
  check_elems(l, m + lo, n);
  free(m);
}

/* [BUILTIN-LIST-PREPEND] is O(log32 n): cons-building a long list is linear. */
static void test_prepend_scales(void) {
  clock_t start = clock();
  OspreyList *l = osprey_list_empty();
  for (int64_t i = 0; i < PREPEND_SCALE_N; i++) l = osprey_list_prepend(l, i);
  CHECK(osprey_list_length(l) == PREPEND_SCALE_N);
  for (int64_t i = 0; i < PREPEND_SCALE_N; i++) {
    CHECK(osprey_list_get(l, i) == PREPEND_SCALE_N - 1 - i);
  }
  CHECK((clock() - start) / CLOCKS_PER_SEC < PREPEND_SCALE_CPU_SECONDS);
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
  test_prepend_deep();
  test_prepend_scales();
  test_branching_persistence();
  test_random_ops();
  test_builder_paths();
  test_element_kind_flags();
  g_checks += run_list_sort_tests();
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
