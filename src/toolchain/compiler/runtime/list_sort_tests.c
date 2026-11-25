#include "collection_runtime.h"

#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

/* The suite's units allocate through the GC shim; the injector replaces the
   real allocator underneath it, so a large block's refusal reaches the list. */
#undef malloc
#undef calloc
#undef realloc
#undef free
#include "test_alloc.h"

/*
 * Vanilla-C suite for osprey_list_sort, the runtime half of `listSort`
 * [BUILTIN-LIST-SORT] (docs/specs/runtime/0012-Built-InFunctions.md): a merge
 * sort over the list's words — numeric for a scalar list, strcmp over the
 * pointed-at text for a managed one. A list shorter than two comes back as
 * itself (the aliased, retain-on-return result); every longer length up to
 * past the trie's tail is checked ascending with repeats and negatives, so the
 * odd split and both merge tails run. The source list is never reordered.
 *
 * Linked into the list_tests suite: run_list_tests() calls
 * run_list_sort_tests() and adds its assertion count to the report.
 */

static int64_t g_checks = 0;
#define CHECK(cond)                                                            \
  do {                                                                         \
    g_checks++;                                                                \
    assert(cond);                                                              \
  } while (0)
#define NELEMS(a) (sizeof(a) / sizeof((a)[0]))
/* > 2 * OSPREY_LIST_BRANCH: the longest input crosses the builder's tail. */
#define SORT_MAX_N 70
#define SCRAMBLE_STRIDE 7919
#define SCRAMBLE_MODULUS 13
#define SCRAMBLE_SHIFT 6

/* `n` words with repeats and negatives, in a non-monotonic order. */
static int64_t scrambled_word(int64_t i) {
  return (i * SCRAMBLE_STRIDE) % SCRAMBLE_MODULUS - SCRAMBLE_SHIFT;
}

static OspreyList *scrambled(int64_t n) {
  OspreyListBuilder *b = osprey_list_builder_new_of(0);
  for (int64_t i = 0; i < n; i++) {
    osprey_list_builder_push_of(b, scrambled_word(i), 0);
  }
  return osprey_list_builder_seal(b);
}

static void check_ascending(OspreyList *sorted, int64_t n) {
  CHECK(osprey_list_length(sorted) == n);
  CHECK(osprey_list_elem_managed(sorted) == 0);
  for (int64_t i = 1; i < n; i++) {
    CHECK(osprey_list_get(sorted, i - 1) <= osprey_list_get(sorted, i));
  }
}

static void test_short_lists_come_back_as_themselves(void) {
  OspreyList *e = osprey_list_empty();
  CHECK(osprey_list_sort(e) == e);
<<<<<<< HEAD
  OspreyListBuilder *b = osprey_list_builder_new();
=======
  OspreyListBuilder *b = osprey_list_builder_new_of(0);
>>>>>>> origin/fixes
  osprey_list_builder_push_of(b, 7, 0);
  OspreyList *one = osprey_list_builder_seal(b);
  CHECK(osprey_list_sort(one) == one);
  CHECK(osprey_list_get(osprey_list_sort(one), 0) == 7);
}

static void test_scalar_lists_sort_ascending_at_every_length(void) {
  for (int64_t n = 2; n <= SORT_MAX_N; n++) {
    OspreyList *source = scrambled(n);
    check_ascending(osprey_list_sort(source), n);
    /* persistence: the source keeps its order */
    for (int64_t i = 0; i < n; i++) {
      CHECK(osprey_list_get(source, i) == scrambled_word(i));
    }
  }
}

static void test_managed_lists_sort_by_text(void) {
  static const char *const words[] = {"pear", "apple", "fig", "banana",
                                      "apple"};
  static const char *const expected[] = {"apple", "apple", "banana", "fig",
                                         "pear"};
  OspreyListBuilder *b = osprey_list_builder_new_of(1);
  for (size_t i = 0; i < NELEMS(words); i++) {
    osprey_list_builder_push_borrowed(b, (int64_t)(uintptr_t)words[i]);
  }
  OspreyList *sorted = osprey_list_sort(osprey_list_builder_seal(b));
  CHECK(osprey_list_elem_managed(sorted) == 1);
  CHECK(osprey_list_length(sorted) == (int64_t)NELEMS(expected));
  for (size_t i = 0; i < NELEMS(expected); i++) {
    int64_t word = osprey_list_get(sorted, (int64_t)i);
    CHECK(strcmp((const char *)(uintptr_t)word, expected[i]) == 0);
  }
}

/* Words in a list whose sort buffers are larger blocks than a GC page serves,
   so each is one real allocation of exactly SORT_REFUSED_BYTES. */
#define SORT_REFUSED_N 1024
#define SORT_REFUSED_BYTES ((size_t)SORT_REFUSED_N * sizeof(int64_t))
static OspreyList *g_refused_source;

/* Refuse the sort's SECOND buffer of that size: the scratch half after src. */
static void osp_alloc_fail_next_of_size(void) { osp_alloc_fail_size(SORT_REFUSED_BYTES); }

static void refuse_scratch(void) {
  atomic_store(&osp_alloc_failure_size, SORT_REFUSED_BYTES);
  atomic_store(&osp_alloc_successes_left, 1);
}

static void sort_refused_source(void) { (void)osprey_list_sort(g_refused_source); }

/* [FAILURE-EXPLICIT] A refused sort buffer ends the run naming the request,
   never a store through NULL (#409). */
static void sorts_refused(void (*arm)(void)) {
  CHECK(osp_child_refusal_names(arm, sort_refused_source,
                                "osprey: allocation refused: listSort request_bytes=8192"));
}

static void test_refused_sort_buffers_end_the_run_by_name(void) {
  g_refused_source = scrambled(SORT_REFUSED_N);
  sorts_refused(osp_alloc_fail_next_of_size);
  sorts_refused(refuse_scratch);
  /* the parent's injector never armed: the same sort still succeeds here */
  check_ascending(osprey_list_sort(g_refused_source), SORT_REFUSED_N);
}

int64_t run_list_sort_tests(void) {
  test_short_lists_come_back_as_themselves();
  test_scalar_lists_sort_ascending_at_every_length();
  test_managed_lists_sort_by_text();
  test_refused_sort_buffers_end_the_run_by_name();
  return g_checks;
}
