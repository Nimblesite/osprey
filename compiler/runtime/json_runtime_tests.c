// Assertion-driven tests for json_runtime.c — the recursive-descent parser and
// path accessors behind [BUILTIN-JSON] (docs/specs/0012). Standalone link (the
// unit is self-contained C11 + pthread).
//
// The contract under test: json_parse returns a 1-based handle or an EXACT
// negative error (-1 NULL, -2 malformed, -3 trailing garbage, -4 table full);
// json_get yields a scalar's text or NULL (arrays/objects are not scalars);
// json_length counts arrays/objects or yields -1; json_free succeeds exactly
// once. Escape decoding — including \uXXXX and surrogate pairs — must produce
// exact UTF-8 bytes.
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "json_tests_shared.h"
#include "test_alloc.h"

long g_checks = 0;
#define MAX_DOCS 1024 // mirrors MAX_JSON_DOCS
#define BIG_ARRAY_LEN 1000

// json_get result must equal `want`, exactly, and is caller-freed.
void expect_get(int64_t h, const char *path, const char *want) {
  char *got = json_get(h, (char *)(uintptr_t)path);
  if (want == NULL) {
    CHECK(got == NULL);
    return;
  }
  CHECK(got != NULL);
  CHECK(strcmp(got, want) == 0);
  free(got);
}

int64_t parse_ok(const char *src) {
  int64_t h = json_parse((char *)(uintptr_t)src);
  CHECK(h >= 1);
  return h;
}

// Scalars round-trip as their exact source text (numbers keep their raw
// spelling); true/false/null print canonically.
static void t_scalars(void) {
  int64_t h = parse_ok("42");
  expect_get(h, "", "42");
  expect_get(h, NULL, "42"); // NULL path means the root
  CHECK(json_length(h, "") == -1); // scalars have no length
  CHECK(json_free(h) == 0);
  const char *nums[][2] = {{"-3.5e2", "-3.5e2"}, {"1E+5", "1E+5"},
                           {"-0.5", "-0.5"},     {"0", "0"}};
  for (unsigned i = 0; i < sizeof(nums) / sizeof(nums[0]); i++) {
    int64_t n = parse_ok(nums[i][0]);
    expect_get(n, "", nums[i][1]);
    CHECK(json_free(n) == 0);
  }
  int64_t t = parse_ok("true");
  int64_t f = parse_ok("false");
  int64_t z = parse_ok("null");
  expect_get(t, "", "true");
  expect_get(f, "", "false");
  expect_get(z, "", "null");
  CHECK(json_free(t) == 0 && json_free(f) == 0 && json_free(z) == 0);
}

// Escape decoding produces EXACT bytes: simple escapes, \uXXXX, multi-byte
// UTF-8, and a surrogate pair assembling one astral code point.
static void t_string_escapes(void) {
  int64_t h = parse_ok("{\"s\":\"a\\\"b\\\\c\\/d\\n\\t\\r\\b\\f\"}");
  expect_get(h, "s", "a\"b\\c/d\n\t\r\b\f");
  CHECK(json_free(h) == 0);
  int64_t u = parse_ok("\"\\u0041\\u00e9\\u20AC\"");
  expect_get(u, "", "A\xC3\xA9\xE2\x82\xAC"); // A, é (2B), € (3B)
  CHECK(json_free(u) == 0);
  int64_t p = parse_ok("\"\\uD83D\\uDE00\"");
  expect_get(p, "", "\xF0\x9F\x98\x80"); // 😀 via surrogate pair, 4 bytes
  CHECK(json_free(p) == 0);
  int64_t key = parse_ok("{\"\\u0041\":7}"); // escapes decode in KEYS too
  expect_get(key, "A", "7");
  CHECK(json_free(key) == 0);
}

// Path navigation: dotted keys, indexed arrays, arbitrary nesting, exact
// lengths, and NULL for everything that is absent or not a scalar.
static void t_paths_and_lengths(void) {
  int64_t h = parse_ok(" { \"a\" : { \"b\" : [ 10 , 20 , { \"c\" : \"hi\" } ] },"
                       " \"empty\" : [] , \"eobj\" : {} } ");
  expect_get(h, "a.b[2].c", "hi");
  expect_get(h, ".a.b[0]", "10"); // leading dots are tolerated
  expect_get(h, "a.b[1]", "20");
  CHECK(json_length(h, "a.b") == 3);
  CHECK(json_length(h, "") == 3);        // root object: three keys
  CHECK(json_length(h, "empty") == 0);   // empty array
  CHECK(json_length(h, "eobj") == 0);    // empty object
  CHECK(json_length(h, "a.b[0]") == -1); // scalar
  CHECK(json_length(h, "missing") == -1);
  expect_get(h, "a", NULL);        // object is not a scalar
  expect_get(h, "a.b", NULL);      // array is not a scalar
  expect_get(h, "a.b[3]", NULL);   // index out of bounds
  expect_get(h, "a.b[-1]", NULL);  // malformed index
  expect_get(h, "a.b[x]", NULL);   // non-numeric index
  expect_get(h, "a.b[0", NULL);    // unclosed bracket
  expect_get(h, "a.missing", NULL);
  expect_get(h, "a.b[0].c", NULL); // indexing into a scalar
  CHECK(json_free(h) == 0);
  int64_t deep = parse_ok("[[[[[[42]]]]]]");
  expect_get(deep, "[0][0][0][0][0][0]", "42");
  CHECK(json_free(deep) == 0);
  int64_t dup = parse_ok("{\"k\":1,\"k\":2}");
  expect_get(dup, "k", "1"); // FIRST duplicate wins, deterministically
  CHECK(json_free(dup) == 0);
}

// Every malformed input maps to its EXACT error code, and no error perturbs
// later parses.
static void t_parse_errors(void) {
  CHECK(json_parse(NULL) == ERR_NULL);
  const char *bad[] = {"",       "   ",      "{",        "[1,",  "tru",
                       "fals",   "nul",      "\"open",   "{\"a\" 1}",
                       "{\"a\":}", "[1 2]",  "{1:2}",    "@",
                       // a member that is neither followed by ',' nor closed
                       "{\"a\":1 \"b\":2}", "{\"a\":1 2}", "{\"a\":1]"};
  for (unsigned i = 0; i < sizeof(bad) / sizeof(bad[0]); i++) {
    CHECK(json_parse((char *)(uintptr_t)bad[i]) == ERR_MALFORMED);
  }
  CHECK(json_parse((char *)(uintptr_t) "1 2") == ERR_TRAILING);
  CHECK(json_parse((char *)(uintptr_t) "{} []") == ERR_TRAILING);
  int64_t h = parse_ok("[3]"); // the table is intact after every rejection
  CHECK(json_length(h, "") == 1);
  CHECK(json_free(h) == 0);
}

// Handles are 1-based, sequential from the lowest free slot, and freed slots
// are reused; every invalid/double free reports -1 and frees exactly once.
static void t_handle_lifecycle(void) {
  int64_t a = json_parse((char *)(uintptr_t) "1");
  int64_t b = json_parse((char *)(uintptr_t) "2");
  CHECK(a == 1 && b == 2); // lowest free slots, in order
  expect_get(a, "", "1");
  expect_get(b, "", "2");
  CHECK(json_free(a) == 0);
  CHECK(json_free(a) == -1); // double free
  CHECK(json_get(a, (char *)(uintptr_t) "") == NULL); // freed: no scalar
  CHECK(json_length(a, (char *)(uintptr_t) "") == -1);
  int64_t c = json_parse((char *)(uintptr_t) "3");
  CHECK(c == 1); // freed slot 1 is reused first
  CHECK(json_free(0) == -1 && json_free(-5) == -1);
  CHECK(json_free(MAX_DOCS) == -1 && json_free(99999) == -1);
  CHECK(json_free(b) == 0 && json_free(c) == 0);
}

// The document table holds exactly MAX_DOCS-1 documents; the next parse
// reports table-full, and freeing restores capacity.
static void t_table_capacity_exact(void) {
  int64_t handles[MAX_DOCS];
  for (int64_t i = 1; i < MAX_DOCS; i++) {
    handles[i] = json_parse((char *)(uintptr_t) "0");
    CHECK(handles[i] == i);
  }
  CHECK(json_parse((char *)(uintptr_t) "0") == ERR_TABLE_FULL);
  for (int64_t i = 1; i < MAX_DOCS; i++) {
    CHECK(json_free(handles[i]) == 0);
  }
  int64_t again = parse_ok("7"); // capacity fully restored
  CHECK(again == 1);
  CHECK(json_free(again) == 0);
}

// A large flat array counts and indexes exactly.
static void t_big_array(void) {
  char *src = malloc((size_t)BIG_ARRAY_LEN * 8 + 16);
  CHECK(src != NULL);
  size_t off = 0;
  src[off++] = '[';
  for (int i = 0; i < BIG_ARRAY_LEN; i++) {
    off += (size_t)sprintf(src + off, i ? ",%d" : "%d", i);
  }
  src[off++] = ']';
  src[off] = '\0';
  int64_t h = parse_ok(src);
  CHECK(json_length(h, "") == BIG_ARRAY_LEN);
  expect_get(h, "[0]", "0");
  expect_get(h, "[999]", "999");
  expect_get(h, "[500]", "500");
  CHECK(json_free(h) == 0);
  free(src);
}

// Deep enough to run PAST the last allocation either operation makes, which is
// what `survived` below asserts. A sweep that never gets past the allocator
// proves the arms it reached and silently says nothing about the rest.
#define ALLOC_SWEEP_DEPTH 300

// Every construct the parser allocates for, in one document: nested objects
// and arrays, each scalar kind, an escape run, and a surrogate pair.
#define ALLOC_SWEEP_DOC                                                        \
  "{\"a\":[1,2.5,-3e4,true,false,null,\"\\u00e9\\ud83d\\ude00\\t\\\\\"],"      \
  "\"b\":{\"c\":{\"d\":[{\"e\":\"f\"}]}},\"g\":\"h\","                        \
  "\"long\":\"past the sixteen byte initial string buffer, twice over\"}"

// Failing the Nth allocation of a parse must produce the DOCUMENTED rejection
// — never a handle onto a half-built document, never a crash. Every
// `if (!p) { free(...); ok = false; return NULL; }` arm in this parser is a
// real contract, and no input can reach one: they exist for the day the
// allocator says no. [BUILTIN-JSON]
static void t_parse_survives_every_allocation_failure(void) {
  // The parser's very first allocation failing is the same rejection as
  // malformed input, not a different one: no input distinguishes them, so
  // neither may the caller.
  osp_alloc_fail_next();
  CHECK(json_parse((char *)(uintptr_t) "{\"k\":\"v\"}") == ERR_MALFORMED);
  osp_alloc_fail_off();
  long refused = 0;
  long survived = 0;
  const long live_before = osp_alloc_live();
  for (long nth = 0; nth < ALLOC_SWEEP_DEPTH; nth++) {
    osp_alloc_fail_after(nth);
    int64_t handle = json_parse((char *)(uintptr_t)ALLOC_SWEEP_DOC);
    osp_alloc_fail_off();
    if (handle >= 1) {
      survived++;
      // A parse that survived is WHOLE: the same document as an unfailed one.
      expect_get(handle, "b.c.d[0].e", "f");
      expect_get(handle, "a[3]", "true");
      CHECK(json_length(handle, "a") == 7);
      CHECK(json_free(handle) == 0);
      continue;
    }
    refused++;
    CHECK(handle == ERR_MALFORMED); // not -1, -3 or -4: the parse ran and failed
  }
  CHECK(refused > 0); // the sweep really did reach the allocator
  CHECK(survived > 0); // ...and ran past the last allocation the parse makes
  // An abandoned parse must free everything it built. Leaking the half-tree
  // instead is the other way to fail this contract, and it is invisible to a
  // return value.
  CHECK(osp_alloc_live() == live_before);
}

// The same sweep over the accessors. A get that cannot allocate its result
// must answer NULL; one that answers at all must be byte-exact, because a
// truncated scalar is indistinguishable from a shorter one in the document.
static void t_accessors_survive_every_allocation_failure(void) {
  int64_t handle = parse_ok(ALLOC_SWEEP_DOC);
  long refused = 0;
  long survived = 0;
  const long live_before = osp_alloc_live();
  for (long nth = 0; nth < ALLOC_SWEEP_DEPTH; nth++) {
    osp_alloc_fail_after(nth);
    char *got = json_get(handle, (char *)(uintptr_t) "a[0]");
    osp_alloc_fail_off();
    if (got == NULL) {
      refused++;
      continue;
    }
    survived++;
    CHECK(strcmp(got, "1") == 0);
    free(got);
    // Counting never allocates, so it must answer through a failed allocation.
    osp_alloc_fail_after(nth);
    int64_t length = json_length(handle, (char *)(uintptr_t) "a");
    osp_alloc_fail_off();
    CHECK(length == 7);
  }
  CHECK(refused > 0);
  CHECK(survived > 0);
  CHECK(osp_alloc_live() == live_before); // a refused get leaks nothing either
  CHECK(json_free(handle) == 0);
  CHECK(osp_alloc_live() < live_before); // ...and the document really is gone
}

// A \u escape is four HEX digits. Decoding a non-hex digit as zero accepted
// "\uZZZZ" as U+0000 — malformed input turned into an embedded NUL that
// truncates the string the caller reads back, with no error anywhere.
int main(void) {
  t_scalars();
  t_string_escapes();
  t_paths_and_lengths();
  t_parse_errors();
  t_handle_lifecycle();
  t_table_capacity_exact();
  t_big_array();
  t_string_grammar_rejects_everything_outside_it();
  t_number_grammar();
  t_parse_survives_every_allocation_failure();
  t_accessors_survive_every_allocation_failure();
  printf("[ok] json_runtime: %ld assertions\n", g_checks);
  return 0;
}
