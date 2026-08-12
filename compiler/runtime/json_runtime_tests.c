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

int64_t json_parse(char *s);
char *json_get(int64_t handle, char *path);
int64_t json_length(int64_t handle, char *path);
int64_t json_free(int64_t handle);

static long g_checks = 0;
#define CHECK(c)                                                               \
  do {                                                                         \
    g_checks++;                                                                \
    assert(c);                                                                 \
  } while (0)

#define ERR_NULL ((int64_t)-1)
#define ERR_MALFORMED ((int64_t)-2)
#define ERR_TRAILING ((int64_t)-3)
#define ERR_TABLE_FULL ((int64_t)-4)
#define MAX_DOCS 1024 // mirrors MAX_JSON_DOCS
#define BIG_ARRAY_LEN 1000

// json_get result must equal `want`, exactly, and is caller-freed.
static void expect_get(int64_t h, const char *path, const char *want) {
  char *got = json_get(h, (char *)(uintptr_t)path);
  if (want == NULL) {
    CHECK(got == NULL);
    return;
  }
  CHECK(got != NULL);
  CHECK(strcmp(got, want) == 0);
  free(got);
}

static int64_t parse_ok(const char *src) {
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
  int64_t lax = parse_ok("\"\\x\""); // unknown escape passes the char through
  expect_get(lax, "", "x");
  CHECK(json_free(lax) == 0);
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
                       "{\"a\":}", "[1 2]",  "{1:2}",    "@"};
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

int main(void) {
  t_scalars();
  t_string_escapes();
  t_paths_and_lengths();
  t_parse_errors();
  t_handle_lifecycle();
  t_table_capacity_exact();
  t_big_array();
  printf("[ok] json_runtime: %ld assertions\n", g_checks);
  return 0;
}
