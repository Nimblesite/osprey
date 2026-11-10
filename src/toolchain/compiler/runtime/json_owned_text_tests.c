// Owned text with embedded NULs across the string runtime the json suite links:
// cursor bounds, survival across collection and growth, finalizer reads, search,
// slices, trims and lists, and bounded search against a reference. Third
// translation unit of the json suite; see json_tests_shared.h.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "json_tests_shared.h"
#include "memory_hooks.h"
#include "string_runtime.h"

void osp_collect(void);

static void *g_finalizer_text;
static size_t g_text_finalized;

// [PORT-STRING-NUL-OWNERSHIP] The existing allocation owns the full text length.
void t_owned_text_preserves_nul_cursor_bounds(void) {
  CHECK(osp_alloc_tagged(0, 7) == NULL && osp_alloc_tagged_noinit(-1, 7) == NULL);
  CHECK(osp_alloc_tagged(INT64_MAX, 7) == NULL);
  const char *samples[] = {"head\0tail", "\0lead", "tail\0", "é\0😀", "first\0middle\0last", ""};
  const size_t lengths[] = {9, 5, 5, 7, 17, 0};
  for (size_t i = 0; i < sizeof(lengths) / sizeof(*lengths); i++) {
    char *text = osp_alloc_tagged((int64_t)(lengths[i] + 1), 7); // STRING layout ABI, independent of its implementation.
    CHECK(text != NULL);
    memcpy(text, samples[i], lengths[i] + 1);
    CHECK(osp_strlen(text) == (int64_t)lengths[i]);
    CHECK(osp_string_byte_length(text) == (int64_t)lengths[i]);
    CHECK(osp_string_is_empty(text) == (lengths[i] == 0));
    for (size_t at = 0; at < lengths[i]; at++) {
      int64_t byte = -1;
      CHECK(osp_string_byte_at(text, (int64_t)at, &byte) == NULL && byte == (unsigned char)samples[i][at]);
      if (samples[i][at] == '\0') CHECK(osp_string_codepoint_at(text, (int64_t)at, &byte) == NULL && byte == 0);
    }
    osp_release(text);
  }
}

void t_owned_text_survives_collection_and_growth(void) {
  void *volatile roots[14000];
  for (size_t i = 0; i < sizeof(roots) / sizeof(*roots); i++) {
    char *text = osp_alloc_tagged(10, 7);
    CHECK(text != NULL);
    memcpy(text, "head\0tail", 10);
    roots[i] = text;
  }
  osp_collect();
  for (size_t i = 0; i < sizeof(roots) / sizeof(*roots); i++) {
    CHECK(osp_strlen(roots[i]) == 9);
    CHECK(memcmp(roots[i], "head\0tail", 10) == 0);
    osp_release(roots[i]);
  }
}

static void inspect_finalizer_text(void *body) {
  (void)body;
  CHECK(osp_mem_string_length(g_finalizer_text) == 9);
  g_text_finalized++;
}

void t_owned_text_finalizer_reads_do_not_deadlock(void) {
  osp_mem_notify_multithreaded();
  g_finalizer_text = osp_alloc_tagged(10, 7);
  CHECK(g_finalizer_text != NULL);
  memcpy(g_finalizer_text, "head\0tail", 10);
  osp_mem_set_finalizer(inspect_finalizer_text);
  for (size_t i = 0; i < 4096; i++) {
    void *owner = osp_alloc_tagged(16, OSP_MEM_LIST_HDR_EXTERN);
    CHECK(owner != NULL);
    memset(owner, 0, 16);
    osp_release(owner);
  }
  osp_collect();
  CHECK(g_text_finalized > 4000);
  osp_mem_set_finalizer(NULL);
  osp_release(g_finalizer_text);
  g_finalizer_text = NULL;
}

// [PORT-STRING-NUL-OPERATIONS] Same prefix before NUL must not alias the full value.
void t_owned_text_search_and_slices(void) {
  char *text = owned_bytes("head\0tail", 9), *wrong = owned_bytes("head\0X", 6);
  char *nul = owned_bytes("\0", 1), *prefix = owned_bytes("head\0t", 6);
  CHECK(osp_string_starts_with(text, wrong) == 0);
  CHECK(osp_string_starts_with(text, prefix) == 1 && osp_string_ends_with(text, "tail") == 1);
  CHECK(osp_string_index_of(text, nul) == 4 && osp_string_index_of(text, "tail") == 5);
  CHECK(osp_string_index_of(text, wrong) == -1);
  check_text(osp_string_take(text, 7), "head\0ta", 7);
  check_text(osp_string_drop(text, 4), "\0tail", 5);
  check_text(osp_string_substring(text, 3, 8), "d\0tai", 5);
  check_text(osp_string_to_upper(text), "HEAD\0TAIL", 9);
  check_text(osp_string_reverse(text), "liat\0daeh", 9);
  check_text(osp_string_replace(text, nul, "::"), "head::tail", 10);
  check_text(osp_string_repeat(nul, 3), "\0\0\0", 3);
  check_text(osp_string_pad_start(text, 12, nul), "\0\0\0head\0tail", 12);
  check_text(osp_string_pad_end(text, 12, nul), "head\0tail\0\0\0", 12);
  osp_release(text); osp_release(wrong); osp_release(nul); osp_release(prefix);
}

void t_owned_text_trim_and_lists(void) {
  char *text = owned_bytes(" \thead\0tail\r\n", 13), *nul = owned_bytes("\0", 1);
  check_text(osp_string_trim(text), "head\0tail", 9);
  check_text(osp_string_trim_start(text), "head\0tail\r\n", 11);
  check_text(osp_string_trim_end(text), " \thead\0tail", 11);
  char *words = owned_bytes("a\0b c\0d", 7);
  osp_string_list *parts = osp_string_words(words);
  CHECK(parts != NULL && parts->length == 2);
  CHECK(osp_strlen(parts->items[0]) == 3 && memcmp(parts->items[0], "a\0b", 3) == 0);
  CHECK(osp_strlen(parts->items[1]) == 3 && memcmp(parts->items[1], "c\0d", 3) == 0);
  check_text(osp_string_join(parts, nul), "a\0b\0c\0d", 7);
  osp_string_list_free(parts);
  parts = osp_string_split(words, nul);
  CHECK(parts != NULL && parts->length == 3);
  CHECK(osp_strlen(parts->items[1]) == 3 && memcmp(parts->items[1], "b c", 3) == 0);
  osp_string_list_free(parts);
  osp_release(text); osp_release(nul); osp_release(words);
}

static int64_t reference_byte_find(const char *text, size_t n, const char *pattern, size_t m) {
  if (m > n) return -1;
  for (size_t i = 0; i <= n - m; i++) if (memcmp(text + i, pattern, m) == 0) return (int64_t)i;
  return -1;
}

void t_bounded_search_permutations(void) {
  uint32_t seed = 1469598103;
  char text[64], pattern[32];
  for (size_t trial = 0; trial < 50000; trial++) {
    seed = seed * 1664525u + 1013904223u; size_t n = seed % sizeof(text);
    seed = seed * 1664525u + 1013904223u; size_t m = (seed >> 8) % sizeof(pattern);
    for (size_t i = 0; i < n; i++) { seed = seed * 1664525u + 1013904223u; text[i] = (char)((seed >> 8) % 4); }
    for (size_t i = 0; i < m; i++) { seed = seed * 1664525u + 1013904223u; pattern[i] = (char)((seed >> 8) % 4); }
    if (trial % 3 == 0 && m <= n) memcpy(pattern, text + seed % (n - m + 1), m);
    const char *found = osp_string_find_internal(text, n, pattern, m);
    CHECK((found ? (int64_t)(found - text) : -1) == reference_byte_find(text, n, pattern, m));
  }
}

void t_bounded_search_long_repetitions(void) {
  size_t n = 1048576, m = 32768;
  char *text = malloc(n), *pattern = malloc(m); CHECK(text != NULL && pattern != NULL);
  memset(text, 'a', n); memset(pattern, 'a', m); pattern[m - 1] = 'b';
  CHECK(osp_string_find_internal(text, n, pattern, m) == NULL);
  text[n - 1] = 'b';
  CHECK(osp_string_find_internal(text, n, pattern, m) == text + n - m);
  free(text); free(pattern);
}

void t_bounded_search_exhaustive(void) {
  char text[8], pattern[6];
  for (size_t n = 0; n <= sizeof(text); n++) for (size_t word = 0; word < ((size_t)1 << n); word++) {
    for (size_t i = 0; i < n; i++) text[i] = (char)((word >> i) & 1);
    for (size_t m = 0; m <= sizeof(pattern); m++) for (size_t key = 0; key < ((size_t)1 << m); key++) {
      for (size_t i = 0; i < m; i++) pattern[i] = (char)((key >> i) & 1);
      const char *found = osp_string_find_internal(text, n, pattern, m);
      CHECK((found ? (int64_t)(found - text) : -1) == reference_byte_find(text, n, pattern, m));
    }
  }
}
