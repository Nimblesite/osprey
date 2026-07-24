/*
 * Verifies [BUILTIN-STRING-INSPECTION], [BUILTIN-STRING-SEARCH],
 * [BUILTIN-STRING-CURSOR], [BUILTIN-STRING-SUBSTRINGS], [BUILTIN-STRING-LIST],
 * [BUILTIN-STRING-TRANSFORM], and [BUILTIN-STRING-PARSING].
 *
 * Strict assertion-driven tests for every helper in string_runtime.c and
 * string_runtime_list.c. Each test exercises both the happy path AND
 * every documented error/edge case. A failure aborts (assert) — the test
 * binary's exit status is the verdict.
 *
 * Run by `make test` via the root Makefile `_test_c_runtime` target (hardened
 * flags, executable build). Covers the Result error-message contract
 * ([ERR-PAYLOAD]) exhaustively.
 */

#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "string_runtime.h"

/* ---------- scalar predicates ---------- */

static void test_is_empty(void) {
    assert(osp_string_is_empty("") == 1);
    assert(osp_string_is_empty("a") == 0);
    assert(osp_string_is_empty("hello world") == 0);
    assert(osp_string_is_empty(NULL) == 1); /* NULL defended */
    printf("  ok  is_empty\n");
}

static void test_starts_with(void) {
    assert(osp_string_starts_with("hello world", "hello") == 1);
    assert(osp_string_starts_with("hello world", "world") == 0);
    assert(osp_string_starts_with("hello world", "") == 1);
    assert(osp_string_starts_with("", "") == 1);
    assert(osp_string_starts_with("", "x") == 0);
    assert(osp_string_starts_with("hi", "hello") == 0); /* prefix longer than s */
    assert(osp_string_starts_with("GET /api", "GET ") == 1);
    assert(osp_string_starts_with(NULL, "x") == 0);
    assert(osp_string_starts_with("x", NULL) == 0);
    printf("  ok  starts_with\n");
}

static void test_ends_with(void) {
    assert(osp_string_ends_with("hello world", "world") == 1);
    assert(osp_string_ends_with("hello world", "hello") == 0);
    assert(osp_string_ends_with("hello world", "") == 1);
    assert(osp_string_ends_with("", "") == 1);
    assert(osp_string_ends_with("", "x") == 0);
    assert(osp_string_ends_with("hi", "hello") == 0); /* suffix longer than s */
    assert(osp_string_ends_with("image.png", ".png") == 1);
    assert(osp_string_ends_with("image.PNG", ".png") == 0); /* case-sensitive */
    printf("  ok  ends_with\n");
}

static void test_index_of(void) {
    assert(osp_string_index_of("hello", "ell") == 1);
    assert(osp_string_index_of("hello", "h") == 0);
    assert(osp_string_index_of("hello", "o") == 4);
    assert(osp_string_index_of("hello", "xyz") == -1);     /* not found */
    assert(osp_string_index_of("hello", "") == 0);          /* empty needle */
    assert(osp_string_index_of("foo=bar=baz", "=") == 3);   /* first occurrence */
    assert(osp_string_index_of(NULL, "x") == -1);
    assert(osp_string_index_of("x", NULL) == -1);
    printf("  ok  index_of\n");
}

/* ---------- substring helpers ---------- */

static void test_take(void) {
    char *out;
    out = osp_string_take("hello", 3);  assert(strcmp(out, "hel") == 0);   free(out);
    out = osp_string_take("hello", 0);  assert(strcmp(out, "")    == 0);   free(out);
    out = osp_string_take("hello", -5); assert(strcmp(out, "")    == 0);   free(out);
    out = osp_string_take("hello", 5);  assert(strcmp(out, "hello") == 0); free(out);
    out = osp_string_take("hello", 99); assert(strcmp(out, "hello") == 0); free(out); /* clamp */
    out = osp_string_take("", 3);       assert(strcmp(out, "")    == 0);   free(out);
    out = osp_string_take(NULL, 3);     assert(strcmp(out, "")    == 0);   free(out);
    printf("  ok  take\n");
}

static void test_drop(void) {
    char *out;
    out = osp_string_drop("hello", 3);  assert(strcmp(out, "lo")   == 0); free(out);
    out = osp_string_drop("hello", 0);  assert(strcmp(out, "hello") == 0); free(out);
    out = osp_string_drop("hello", -5); assert(strcmp(out, "hello") == 0); free(out);
    out = osp_string_drop("hello", 5);  assert(strcmp(out, "")    == 0);  free(out); /* exact */
    out = osp_string_drop("hello", 99); assert(strcmp(out, "")    == 0);  free(out); /* clamp */
    out = osp_string_drop("", 3);       assert(strcmp(out, "")    == 0);  free(out);
    out = osp_string_drop(NULL, 3);     assert(strcmp(out, "")    == 0);  free(out);
    printf("  ok  drop\n");
}

static void test_substring(void) {
    char *out;
    out = osp_string_substring("hello", 1, 4); assert(strcmp(out, "ell") == 0); free(out);
    out = osp_string_substring("hello", 0, 5); assert(strcmp(out, "hello") == 0); free(out);
    out = osp_string_substring("hello", 0, 0); assert(strcmp(out, "") == 0); free(out);
    out = osp_string_substring("hello", 5, 5); assert(strcmp(out, "") == 0); free(out);
    /* error cases: must return NULL */
    assert(osp_string_substring("hello", -1, 3) == NULL); /* start < 0 */
    assert(osp_string_substring("hello", 0, 99) == NULL); /* end > len */
    assert(osp_string_substring("hello", 4, 2)  == NULL); /* end < start */
    assert(osp_string_substring(NULL, 0, 1)     == NULL);
    printf("  ok  substring\n");
}

/* ---------- transformation ---------- */

static void test_to_upper_lower(void) {
    char *out;
    out = osp_string_to_upper("hello"); assert(strcmp(out, "HELLO") == 0); free(out);
    out = osp_string_to_upper("");      assert(strcmp(out, "")      == 0); free(out);
    out = osp_string_to_upper("AbC");   assert(strcmp(out, "ABC")   == 0); free(out);
    out = osp_string_to_upper("1!@a");  assert(strcmp(out, "1!@A")  == 0); free(out);
    out = osp_string_to_upper(NULL);    assert(strcmp(out, "")      == 0); free(out);

    out = osp_string_to_lower("HELLO"); assert(strcmp(out, "hello") == 0); free(out);
    out = osp_string_to_lower("");      assert(strcmp(out, "")      == 0); free(out);
    out = osp_string_to_lower("AbC");   assert(strcmp(out, "abc")   == 0); free(out);
    printf("  ok  to_upper/to_lower\n");
}

static void test_trim(void) {
    char *out;
    out = osp_string_trim("  hello  ");      assert(strcmp(out, "hello") == 0); free(out);
    out = osp_string_trim("\t\n hi \r\n");   assert(strcmp(out, "hi")    == 0); free(out);
    out = osp_string_trim("hello");          assert(strcmp(out, "hello") == 0); free(out);
    out = osp_string_trim("");               assert(strcmp(out, "")      == 0); free(out);
    out = osp_string_trim("   ");            assert(strcmp(out, "")      == 0); free(out); /* all ws */
    out = osp_string_trim(NULL);             assert(strcmp(out, "")      == 0); free(out);

    out = osp_string_trim_start("  hello  "); assert(strcmp(out, "hello  ") == 0); free(out);
    out = osp_string_trim_start("hello");     assert(strcmp(out, "hello")   == 0); free(out);
    out = osp_string_trim_start("   ");       assert(strcmp(out, "")        == 0); free(out);

    out = osp_string_trim_end("  hello  ");   assert(strcmp(out, "  hello") == 0); free(out);
    out = osp_string_trim_end("hello");       assert(strcmp(out, "hello")   == 0); free(out);
    out = osp_string_trim_end("   ");         assert(strcmp(out, "")        == 0); free(out);
    printf("  ok  trim/trim_start/trim_end\n");
}

static void test_reverse(void) {
    char *out;
    out = osp_string_reverse("abc");   assert(strcmp(out, "cba")    == 0); free(out);
    out = osp_string_reverse("a");     assert(strcmp(out, "a")      == 0); free(out);
    out = osp_string_reverse("");      assert(strcmp(out, "")       == 0); free(out);
    out = osp_string_reverse("12345"); assert(strcmp(out, "54321")  == 0); free(out);
    out = osp_string_reverse(NULL);    assert(strcmp(out, "")       == 0); free(out);
    printf("  ok  reverse\n");
}

static void test_replace(void) {
    char *out;
    out = osp_string_replace("a-b-c", "-", "_");        assert(strcmp(out, "a_b_c") == 0); free(out);
    out = osp_string_replace("aaa", "a", "bb");         assert(strcmp(out, "bbbbbb") == 0); free(out);
    out = osp_string_replace("hello", "xyz", "Q");      assert(strcmp(out, "hello") == 0); free(out); /* no match */
    out = osp_string_replace("hello", "l", "");         assert(strcmp(out, "heo") == 0); free(out); /* shrink */
    out = osp_string_replace("", "x", "y");             assert(strcmp(out, "") == 0); free(out);
    /* error cases */
    assert(osp_string_replace("hello", "", "x")    == NULL); /* empty needle */
    assert(osp_string_replace(NULL,    "x", "y")   == NULL);
    assert(osp_string_replace("h",     NULL, "y")  == NULL);
    assert(osp_string_replace("h",     "x", NULL)  == NULL);
    printf("  ok  replace\n");
}

static void test_repeat(void) {
    char *out;
    out = osp_string_repeat("ab", 3);   assert(strcmp(out, "ababab") == 0); free(out);
    out = osp_string_repeat("x", 5);    assert(strcmp(out, "xxxxx") == 0); free(out);
    out = osp_string_repeat("ab", 0);   assert(strcmp(out, "") == 0); free(out); /* n=0 -> empty */
    out = osp_string_repeat("ab", 1);   assert(strcmp(out, "ab") == 0); free(out);
    out = osp_string_repeat("", 99);    assert(strcmp(out, "") == 0); free(out); /* empty source */
    /* error cases */
    assert(osp_string_repeat("ab", -1) == NULL);
    assert(osp_string_repeat(NULL,  3) == NULL);
    printf("  ok  repeat\n");
}

static void test_pad(void) {
    char *out;
    out = osp_string_pad_start("7",   3, "0");  assert(strcmp(out, "007") == 0); free(out);
    out = osp_string_pad_start("42",  5, "ab"); assert(strcmp(out, "aba42") == 0); free(out);
    out = osp_string_pad_start("hi",  2, "x");  assert(strcmp(out, "hi") == 0); free(out); /* no pad needed */
    out = osp_string_pad_start("hi",  1, "x");  assert(strcmp(out, "hi") == 0); free(out); /* target < len */
    out = osp_string_pad_start("",    3, "0");  assert(strcmp(out, "000") == 0); free(out);

    out = osp_string_pad_end  ("7",   3, ".");  assert(strcmp(out, "7..") == 0); free(out);
    out = osp_string_pad_end  ("42",  5, "ab"); assert(strcmp(out, "42aba") == 0); free(out);
    out = osp_string_pad_end  ("hi",  2, "x");  assert(strcmp(out, "hi") == 0); free(out);

    /* error: empty fill */
    assert(osp_string_pad_start("hi", 5, "") == NULL);
    assert(osp_string_pad_end  ("hi", 5, "") == NULL);
    assert(osp_string_pad_start("hi", 5, NULL) == NULL);
    printf("  ok  pad_start/pad_end\n");
}

/* ---------- parsing ---------- */

static void test_parse_int(void) {
    int64_t out;
    assert(osp_parse_int_strict("42", &out) == 0);     assert(out == 42);
    assert(osp_parse_int_strict("-42", &out) == 0);    assert(out == -42);
    assert(osp_parse_int_strict("+42", &out) == 0);    assert(out == 42);
    assert(osp_parse_int_strict("0", &out) == 0);      assert(out == 0);
    assert(osp_parse_int_strict("9223372036854775807", &out) == 0); /* INT64_MAX */
    assert(out == 9223372036854775807LL);
    assert(osp_parse_int_strict("-9223372036854775808", &out) == 0); /* INT64_MIN */
    assert(out == (-9223372036854775807LL - 1));

    /* rejections */
    assert(osp_parse_int_strict("",        &out) != 0);
    assert(osp_parse_int_strict("abc",     &out) != 0);
    assert(osp_parse_int_strict("12abc",   &out) != 0);
    assert(osp_parse_int_strict("abc12",   &out) != 0);
    assert(osp_parse_int_strict(" 42",     &out) != 0); /* leading space */
    assert(osp_parse_int_strict("42 ",     &out) != 0); /* trailing space */
    assert(osp_parse_int_strict("-",       &out) != 0); /* sign with no digits */
    assert(osp_parse_int_strict("+",       &out) != 0);
    assert(osp_parse_int_strict("9223372036854775808",  &out) != 0); /* overflow */
    assert(osp_parse_int_strict("-9223372036854775809", &out) != 0); /* underflow */
    assert(osp_parse_int_strict(NULL,      &out) != 0);
    printf("  ok  parse_int_strict\n");
}

static void test_parse_float(void) {
    double out;
    assert(osp_parse_float_strict("3.14", &out) == 0);   assert(out > 3.13 && out < 3.15);
    assert(osp_parse_float_strict("0", &out) == 0);      assert(out == 0.0);
    assert(osp_parse_float_strict("-2.5", &out) == 0);   assert(out > -2.51 && out < -2.49);
    assert(osp_parse_float_strict("1e3", &out) == 0);    assert(out > 999.9 && out < 1000.1);

    /* rejections */
    assert(osp_parse_float_strict("",      &out) != 0);
    assert(osp_parse_float_strict("abc",   &out) != 0);
    assert(osp_parse_float_strict("3.14x", &out) != 0); /* trailing junk */
    assert(osp_parse_float_strict(NULL,    &out) != 0);
    printf("  ok  parse_float_strict\n");
}

/* ---------- list-returning ---------- */

static void test_split(void) {
    osp_string_list *list;

    list = osp_string_split("a,b,c", ",");
    assert(list != NULL); assert(list->length == 3);
    assert(strcmp(list->items[0], "a") == 0);
    assert(strcmp(list->items[1], "b") == 0);
    assert(strcmp(list->items[2], "c") == 0);
    osp_string_list_free(list);

    list = osp_string_split("hello", ",");
    assert(list != NULL); assert(list->length == 1);
    assert(strcmp(list->items[0], "hello") == 0);
    osp_string_list_free(list);

    list = osp_string_split(",,", ",");
    assert(list != NULL); assert(list->length == 3); /* "", "", "" */
    assert(strcmp(list->items[0], "") == 0);
    assert(strcmp(list->items[1], "") == 0);
    assert(strcmp(list->items[2], "") == 0);
    osp_string_list_free(list);

    list = osp_string_split("foo::bar::baz", "::");
    assert(list != NULL); assert(list->length == 3);
    assert(strcmp(list->items[0], "foo") == 0);
    assert(strcmp(list->items[1], "bar") == 0);
    assert(strcmp(list->items[2], "baz") == 0);
    osp_string_list_free(list);

    /* error: empty separator */
    assert(osp_string_split("hello", "") == NULL);
    assert(osp_string_split(NULL,    ",") == NULL);
    printf("  ok  split\n");
}

static void test_lines(void) {
    osp_string_list *list;

    list = osp_string_lines("a\nb\nc");
    assert(list != NULL); assert(list->length == 3);
    assert(strcmp(list->items[0], "a") == 0);
    assert(strcmp(list->items[1], "b") == 0);
    assert(strcmp(list->items[2], "c") == 0);
    osp_string_list_free(list);

    /* trailing newline does NOT produce an empty final entry */
    list = osp_string_lines("a\nb\n");
    assert(list != NULL); assert(list->length == 2);
    assert(strcmp(list->items[0], "a") == 0);
    assert(strcmp(list->items[1], "b") == 0);
    osp_string_list_free(list);

    list = osp_string_lines("");
    assert(list != NULL); assert(list->length == 0);
    osp_string_list_free(list);

    list = osp_string_lines("single");
    assert(list != NULL); assert(list->length == 1);
    assert(strcmp(list->items[0], "single") == 0);
    osp_string_list_free(list);

    list = osp_string_lines(NULL);
    assert(list != NULL); assert(list->length == 0);
    osp_string_list_free(list);
    printf("  ok  lines\n");
}

static void test_words(void) {
    osp_string_list *list;

    list = osp_string_words("a b c");
    assert(list != NULL); assert(list->length == 3);
    assert(strcmp(list->items[0], "a") == 0);
    assert(strcmp(list->items[1], "b") == 0);
    assert(strcmp(list->items[2], "c") == 0);
    osp_string_list_free(list);

    /* runs of whitespace collapse; empties dropped */
    list = osp_string_words("  hello\t\tworld  \n  goodbye  ");
    assert(list != NULL); assert(list->length == 3);
    assert(strcmp(list->items[0], "hello") == 0);
    assert(strcmp(list->items[1], "world") == 0);
    assert(strcmp(list->items[2], "goodbye") == 0);
    osp_string_list_free(list);

    list = osp_string_words("");
    assert(list != NULL); assert(list->length == 0);
    osp_string_list_free(list);

    list = osp_string_words("   \t\n\r   ");
    assert(list != NULL); assert(list->length == 0);
    osp_string_list_free(list);

    list = osp_string_words(NULL);
    assert(list != NULL); assert(list->length == 0);
    osp_string_list_free(list);
    printf("  ok  words\n");
}

static void test_join(void) {
    /* Build a list manually and join it. */
    osp_string_list *list = osp_string_split("a,b,c", ",");
    char *out = osp_string_join(list, "-");
    assert(strcmp(out, "a-b-c") == 0);
    free(out);
    osp_string_list_free(list);

    /* join round-trips split */
    list = osp_string_split("foo::bar::baz", "::");
    out = osp_string_join(list, "::");
    assert(strcmp(out, "foo::bar::baz") == 0);
    free(out);
    osp_string_list_free(list);

    /* empty list → "" */
    list = osp_string_split("", ",");
    out = osp_string_join(list, "x");
    assert(strcmp(out, "") == 0);
    free(out);
    osp_string_list_free(list);

    printf("  ok  join\n");
}

/* ---------- adversarial: long strings, repeated ops, round-trips ---------- */

/* Build a string of `n` 'a' chars heap-allocated. Caller frees. */
static char *make_repeat_a(size_t n) {
    char *s = (char *)malloc(n + 1);
    assert(s != NULL);
    for (size_t i = 0; i < n; i++) s[i] = 'a';
    s[n] = '\0';
    return s;
}

static void test_long_strings(void) {
    /* length 10000: ensure no stack/buffer assumption breaks */
    const size_t big = 10000;
    char *s = make_repeat_a(big);

    char *out = osp_string_to_upper(s);
    assert(out != NULL);
    for (size_t i = 0; i < big; i++) assert(out[i] == 'A');
    assert(out[big] == '\0');
    free(out);

    out = osp_string_reverse(s);
    assert(out != NULL);
    for (size_t i = 0; i < big; i++) assert(out[i] == 'a');
    free(out);

    out = osp_string_take(s, 1000);
    assert(out != NULL);
    assert(strlen(out) == 1000);
    free(out);

    out = osp_string_drop(s, 9999);
    assert(strlen(out) == 1);
    assert(out[0] == 'a');
    free(out);

    /* trim no-op on a long non-whitespace string */
    out = osp_string_trim(s);
    assert(strlen(out) == big);
    free(out);

    free(s);
    printf("  ok  long_strings (10000 chars)\n");
}

static void test_round_trips(void) {
    /* take(s, n) + drop(s, n) reconstructs s */
    const char *base = "the quick brown fox jumps over the lazy dog";
    size_t n = strlen(base);
    for (size_t i = 0; i <= n; i++) {
        char *left  = osp_string_take(base, (int64_t)i);
        char *right = osp_string_drop(base, (int64_t)i);
        size_t total = strlen(left) + strlen(right);
        assert(total == n);
        char *rebuilt = (char *)malloc(n + 1);
        memcpy(rebuilt, left, strlen(left));
        memcpy(rebuilt + strlen(left), right, strlen(right));
        rebuilt[n] = '\0';
        assert(strcmp(rebuilt, base) == 0);
        free(rebuilt);
        free(left);
        free(right);
    }

    /* reverse(reverse(s)) == s for every length up to 64 */
    char buf[65];
    for (size_t len = 0; len <= 64; len++) {
        for (size_t i = 0; i < len; i++) buf[i] = (char)('a' + (i % 26));
        buf[len] = '\0';
        char *r1 = osp_string_reverse(buf);
        char *r2 = osp_string_reverse(r1);
        assert(strcmp(r2, buf) == 0);
        free(r1);
        free(r2);
    }

    /* split + join round-trips */
    const char *seps[] = {",", "::", "-", " ", "|", NULL};
    const char *texts[] = {
        "a,b,c", "foo::bar::baz", "-a--b-",
        "this is a sentence", "", "single", "a||b||c", NULL};
    for (size_t i = 0; seps[i]; i++) {
        for (size_t j = 0; texts[j]; j++) {
            /* skip incompatible sep choices */
            osp_string_list *list = osp_string_split(texts[j], seps[i]);
            assert(list != NULL);
            char *back = osp_string_join(list, seps[i]);
            assert(back != NULL);
            assert(strcmp(back, texts[j]) == 0);
            free(back);
            osp_string_list_free(list);
        }
    }

    printf("  ok  round_trips (take/drop, reverse twice, split/join)\n");
}

static void test_unicode_bytes(void) {
    /* The runtime is byte-oriented today — assert that documented byte
     * behaviour holds end-to-end. UTF-8 codepoint awareness is a future
     * workstream (see plan). */
    const char *utf8 = "héllo";  /* h=1 byte, é=2 bytes, l,l,o each 1 */
    /* length = 6 bytes (5 codepoints) */
    osp_string_list *parts = osp_string_split(utf8, "l");
    assert(parts != NULL);
    assert(parts->length == 3);
    assert(strcmp(parts->items[0], "h\xc3\xa9") == 0); /* "hé" */
    assert(strcmp(parts->items[1], "") == 0);
    assert(strcmp(parts->items[2], "o") == 0);
    osp_string_list_free(parts);
    /* indexOf still works on raw bytes */
    assert(osp_string_index_of(utf8, "l") == 3);  /* byte offset, not codepoint */
    assert(osp_string_index_of(utf8, "o") == 5);
    printf("  ok  unicode_bytes (byte-level behaviour locked in)\n");
}

static void test_repeated_replace(void) {
    /* replace many times, growing or shrinking */
    char *out;
    out = osp_string_replace("xxxxxxxxxx", "x", "ab");
    assert(strcmp(out, "abababababababababab") == 0);
    free(out);
    out = osp_string_replace("xxxxxxxxxx", "x", "");
    assert(strcmp(out, "") == 0);
    free(out);
    /* needle longer than replacement, exactly N matches */
    out = osp_string_replace("xyxyxyxy", "xy", "z");
    assert(strcmp(out, "zzzz") == 0);
    free(out);
    /* overlapping-ish needles */
    out = osp_string_replace("aaaa", "aa", "b");
    assert(strcmp(out, "bb") == 0);  /* non-overlapping leftmost */
    free(out);
    /* multi-char replacement that contains the needle (no infinite recursion) */
    out = osp_string_replace("aaa", "a", "aa");
    assert(strcmp(out, "aaaaaa") == 0);
    free(out);
    printf("  ok  repeated_replace\n");
}

static void test_split_pathological(void) {
    osp_string_list *list;

    /* separator at very start */
    list = osp_string_split(",a,b", ",");
    assert(list->length == 3);
    assert(strcmp(list->items[0], "") == 0);
    assert(strcmp(list->items[1], "a") == 0);
    assert(strcmp(list->items[2], "b") == 0);
    osp_string_list_free(list);

    /* separator at very end */
    list = osp_string_split("a,b,", ",");
    assert(list->length == 3);
    assert(strcmp(list->items[0], "a") == 0);
    assert(strcmp(list->items[1], "b") == 0);
    assert(strcmp(list->items[2], "") == 0);
    osp_string_list_free(list);

    /* separator IS the whole string */
    list = osp_string_split(",", ",");
    assert(list->length == 2);
    assert(strcmp(list->items[0], "") == 0);
    assert(strcmp(list->items[1], "") == 0);
    osp_string_list_free(list);

    /* empty input */
    list = osp_string_split("", ",");
    assert(list->length == 1);
    assert(strcmp(list->items[0], "") == 0);
    osp_string_list_free(list);

    /* sep longer than input */
    list = osp_string_split("ab", "abcdef");
    assert(list->length == 1);
    assert(strcmp(list->items[0], "ab") == 0);
    osp_string_list_free(list);

    printf("  ok  split_pathological\n");
}

static void test_pad_overflow_guards(void) {
    char *out;
    /* huge target_length but reasonable */
    out = osp_string_pad_start("x", 100, "ab");
    assert(out != NULL);
    assert(strlen(out) == 100);
    assert(out[99] == 'x');
    /* fill cycle covers exactly */
    assert(out[0] == 'a');
    assert(out[1] == 'b');
    assert(out[2] == 'a');
    free(out);

    out = osp_string_pad_end("x", 100, "abc");
    assert(out != NULL);
    assert(strlen(out) == 100);
    assert(out[0] == 'x');
    assert(out[1] == 'a');
    assert(out[2] == 'b');
    assert(out[3] == 'c');
    assert(out[4] == 'a');
    free(out);

    /* exact-match target_length == strlen(s) returns dup of s */
    out = osp_string_pad_start("abc", 3, "x");
    assert(strcmp(out, "abc") == 0);
    free(out);
    printf("  ok  pad_overflow_guards\n");
}

static void test_parse_int_signs(void) {
    int64_t v;
    /* every legal digit pair */
    assert(osp_parse_int_strict("000", &v) == 0); assert(v == 0);
    assert(osp_parse_int_strict("-000", &v) == 0); assert(v == 0);
    assert(osp_parse_int_strict("+000", &v) == 0); assert(v == 0);
    assert(osp_parse_int_strict("1", &v) == 0); assert(v == 1);
    assert(osp_parse_int_strict("-1", &v) == 0); assert(v == -1);
    /* boundary at INT64_MAX one-off */
    assert(osp_parse_int_strict("9223372036854775806", &v) == 0); /* MAX-1 */
    assert(v == 9223372036854775806LL);
    assert(osp_parse_int_strict("9223372036854775807", &v) == 0); /* MAX */
    assert(v == 9223372036854775807LL);
    /* one digit past MAX still rejects */
    assert(osp_parse_int_strict("9223372036854775808", &v) != 0);
    /* INT64_MIN */
    assert(osp_parse_int_strict("-9223372036854775808", &v) == 0);
    assert(v == (-9223372036854775807LL - 1));
    /* one digit past MIN rejects */
    assert(osp_parse_int_strict("-9223372036854775809", &v) != 0);
    /* leading zeros are OK */
    assert(osp_parse_int_strict("00042", &v) == 0); assert(v == 42);
    assert(osp_parse_int_strict("-00042", &v) == 0); assert(v == -42);
    /* lone sign rejected */
    assert(osp_parse_int_strict("-", &v) != 0);
    assert(osp_parse_int_strict("+", &v) != 0);
    /* sign followed by non-digit rejected */
    assert(osp_parse_int_strict("-x", &v) != 0);
    /* embedded sign rejected */
    assert(osp_parse_int_strict("1-2", &v) != 0);
    /* hex rejected */
    assert(osp_parse_int_strict("0x10", &v) != 0);
    /* unicode digit rejected (we accept ASCII 0-9 only) */
    assert(osp_parse_int_strict("\xef\xbc\x91", &v) != 0); /* full-width '1' */
    printf("  ok  parse_int_signs (boundary + sign + leading zeros + rejection)\n");
}

/* ---------- O(1) byte / codepoint cursor (BUILTIN-STRING-CURSOR) ---------- */

static void test_cursor_byte_length(void) {
    assert(osp_string_byte_length("") == 0);
    assert(osp_string_byte_length("a") == 1);
    assert(osp_string_byte_length("abc") == 3);
    assert(osp_string_byte_length("héllo") == 6);                 /* é = 2 bytes */
    assert(osp_string_byte_length("\xc3\xa9") == 2);              /* é alone */
    assert(osp_string_byte_length("\xe4\xb8\x96") == 3);          /* 世 (3 bytes) */
    assert(osp_string_byte_length("\xf0\x9f\x98\x80") == 4);      /* 😀 (4 bytes) */
    assert(osp_string_byte_length("a\xc3\xa9\xe4\xb8\x96\xf0\x9f\x98\x80") == 10);
    assert(osp_string_byte_length(NULL) == 0);
    /* byteLength == length only for ASCII; longer for multi-byte. */
    char big[1025];
    for (int i = 0; i < 1024; i++) big[i] = 'x';
    big[1024] = '\0';
    assert(osp_string_byte_length(big) == 1024);
    printf("  ok  cursor.byteLength\n");
}

static void test_cursor_byte_at(void) {
    int64_t out;
    /* every position of an ASCII string returns its exact byte */
    const char *s = "Hello";
    for (int i = 0; i < 5; i++) {
        assert(osp_string_byte_at(s, i, &out) == NULL);
        assert(out == (int64_t)(unsigned char)s[i]);
    }
    /* the high byte 0xFF survives as 255, not a sign-extended negative */
    assert(osp_string_byte_at("\xff", 0, &out) == NULL && out == 255);
    /* multi-byte: each raw byte of é (0xC3 0xA9) is addressable */
    assert(osp_string_byte_at("héllo", 1, &out) == NULL && out == 0xC3);
    assert(osp_string_byte_at("héllo", 2, &out) == NULL && out == 0xA9);
    assert(osp_string_byte_at("héllo", 3, &out) == NULL && out == 'l');
    /* error positions: -1, len, len+99, empty, NULL — all reported */
    assert(osp_string_byte_at("abc", -1, &out) != NULL);
    assert(osp_string_byte_at("abc", 3, &out) != NULL);
    assert(osp_string_byte_at("abc", 99, &out) != NULL);
    assert(osp_string_byte_at("", 0, &out) != NULL);
    assert(osp_string_byte_at(NULL, 0, &out) != NULL);
    printf("  ok  cursor.byteAt\n");
}

static void test_cursor_codepoint_at(void) {
    int64_t out;
    /* walk every codepoint of a mixed-script string, asserting scalars. */
    const char *mixed = "Aé世😀";   /* U+0041, U+00E9, U+4E16, U+1F600 */
    assert(osp_string_codepoint_at(mixed, 0, &out) == NULL && out == 0x41);
    assert(osp_string_codepoint_at(mixed, 1, &out) == NULL && out == 0xE9);
    assert(osp_string_codepoint_at(mixed, 3, &out) == NULL && out == 0x4E16);
    assert(osp_string_codepoint_at(mixed, 6, &out) == NULL && out == 0x1F600);
    /* every boundary codepoint of each width decodes exactly */
    assert(osp_string_codepoint_at("\x7f", 0, &out) == NULL && out == 0x7F);
    assert(osp_string_codepoint_at("\xc2\x80", 0, &out) == NULL && out == 0x80);
    assert(osp_string_codepoint_at("\xdf\xbf", 0, &out) == NULL && out == 0x7FF);
    assert(osp_string_codepoint_at("\xe0\xa0\x80", 0, &out) == NULL && out == 0x800);
    assert(osp_string_codepoint_at("\xef\xbf\xbf", 0, &out) == NULL && out == 0xFFFF);
    assert(osp_string_codepoint_at("\xf0\x90\x80\x80", 0, &out) == NULL && out == 0x10000);
    assert(osp_string_codepoint_at("\xf4\x8f\xbf\xbf", 0, &out) == NULL && out == 0x10FFFF);
    /* error cases */
    assert(osp_string_codepoint_at("héllo", 2, &out) != NULL); /* mid-codepoint (0xA9 lead) */
    assert(osp_string_codepoint_at("abc", 3, &out) != NULL);   /* out of range */
    assert(osp_string_codepoint_at("abc", -1, &out) != NULL);
    assert(osp_string_codepoint_at("\xf0", 0, &out) != NULL);  /* truncated 4-byte */
    assert(osp_string_codepoint_at("\xe4\xb8", 0, &out) != NULL); /* truncated 3-byte */
    assert(osp_string_codepoint_at("\xc3\x20", 0, &out) != NULL); /* bad continuation */
    assert(osp_string_codepoint_at("\xff", 0, &out) != NULL);  /* invalid lead 0xFF */
    assert(osp_string_codepoint_at("\x80", 0, &out) != NULL);  /* lone continuation */
    printf("  ok  cursor.codePointAt\n");
}

static void test_cursor_codepoint_width(void) {
    int64_t out;
    assert(osp_string_codepoint_width(0x00, &out) == NULL && out == 1);
    assert(osp_string_codepoint_width(0x7F, &out) == NULL && out == 1);
    assert(osp_string_codepoint_width(0x80, &out) == NULL && out == 2);
    assert(osp_string_codepoint_width(0x7FF, &out) == NULL && out == 2);
    assert(osp_string_codepoint_width(0x800, &out) == NULL && out == 3);
    assert(osp_string_codepoint_width(0xFFFF, &out) == NULL && out == 3);
    assert(osp_string_codepoint_width(0x10000, &out) == NULL && out == 4);
    assert(osp_string_codepoint_width(0x10FFFF, &out) == NULL && out == 4);
    /* surrogate range D800..DFFF rejected at both ends + middle */
    assert(osp_string_codepoint_width(0xD800, &out) != NULL);
    assert(osp_string_codepoint_width(0xDC00, &out) != NULL);
    assert(osp_string_codepoint_width(0xDFFF, &out) != NULL);
    /* just outside the surrogate block is fine */
    assert(osp_string_codepoint_width(0xD7FF, &out) == NULL && out == 3);
    assert(osp_string_codepoint_width(0xE000, &out) == NULL && out == 3);
    /* out of range / negative */
    assert(osp_string_codepoint_width(0x110000, &out) != NULL);
    assert(osp_string_codepoint_width(-1, &out) != NULL);
    printf("  ok  cursor.codePointWidth\n");
}

static void test_cursor_from_codepoint(void) {
    /* exact byte encodings for each width */
    char *e;
    e = osp_string_from_codepoint(0x41);                 /* 'A' */
    assert(e && strcmp(e, "A") == 0 && osp_string_byte_length(e) == 1); free(e);
    e = osp_string_from_codepoint(0xE9);                 /* é */
    assert(e && (unsigned char)e[0] == 0xC3 && (unsigned char)e[1] == 0xA9 &&
           osp_string_byte_length(e) == 2); free(e);
    e = osp_string_from_codepoint(0x4E16);               /* 世 */
    assert(e && (unsigned char)e[0] == 0xE4 && (unsigned char)e[1] == 0xB8 &&
           (unsigned char)e[2] == 0x96 && osp_string_byte_length(e) == 3); free(e);
    e = osp_string_from_codepoint(0x1F600);              /* 😀 */
    assert(e && (unsigned char)e[0] == 0xF0 && (unsigned char)e[1] == 0x9F &&
           (unsigned char)e[2] == 0x98 && (unsigned char)e[3] == 0x80 &&
           osp_string_byte_length(e) == 4); free(e);
    /* invalid scalars rejected */
    assert(osp_string_from_codepoint(0x110000) == NULL);
    assert(osp_string_from_codepoint(0xD800) == NULL);
    assert(osp_string_from_codepoint(0xDFFF) == NULL);
    assert(osp_string_from_codepoint(-1) == NULL);
    printf("  ok  cursor.fromCodePoint\n");
}

/* The four fallible cursor builtins return EXACT message strings — the text
 * threaded into the Result errmsg slot ([ERR-PAYLOAD]). Pin every one. */
static void test_cursor_error_messages(void) {
    int64_t out;
    assert(strcmp(osp_string_byte_at("a", 5, &out), "byteAt: index out of range") == 0);
    assert(strcmp(osp_string_byte_at(NULL, 0, &out), "byteAt: null string") == 0);
    assert(strcmp(osp_string_codepoint_at("a", 9, &out),
                  "codePointAt: index out of range") == 0);
    assert(strcmp(osp_string_codepoint_at("\xff", 0, &out),
                  "codePointAt: invalid UTF-8 lead byte") == 0);
    assert(strcmp(osp_string_codepoint_at("\xf0", 0, &out),
                  "codePointAt: truncated codepoint") == 0);
    assert(strcmp(osp_string_codepoint_at("\xc3\x20", 0, &out),
                  "codePointAt: invalid continuation byte") == 0);
    assert(strcmp(osp_string_codepoint_width(0x110000, &out),
                  "codePointWidth: code point out of range") == 0);
    assert(strcmp(osp_string_codepoint_width(0xD800, &out),
                  "codePointWidth: surrogate is not a scalar") == 0);
    printf("  ok  cursor.error_messages (exact text pinned)\n");
}

/* Exhaustive round-trip: encode then decode must recover the scalar, across
 * the whole code space (skipping surrogates and U+0000). Proves fromCodePoint
 * and codePointAt are exact inverses for every valid scalar. */
static void test_cursor_roundtrip_exhaustive(void) {
    int64_t checked = 0;
    for (int64_t cp = 1; cp <= 0x10FFFF; cp++) {
        if (cp >= 0xD800 && cp <= 0xDFFF) continue; /* surrogates are not scalars */
        char *e = osp_string_from_codepoint(cp);
        assert(e != NULL);
        int64_t dec;
        const char *err = osp_string_codepoint_at(e, 0, &dec);
        assert(err == NULL && dec == cp);
        /* width agrees with the encoded byte count */
        int64_t w;
        assert(osp_string_codepoint_width(cp, &w) == NULL);
        assert(osp_string_byte_length(e) == w);
        free(e);
        checked++;
    }
    /* cp ∈ [1, 0x10FFFF] is 0x10FFFF values; minus the 2048 surrogates. */
    assert(checked == 0x10FFFF - (0xDFFF - 0xD800 + 1));
    printf("  ok  cursor.roundtrip_exhaustive (%lld scalars)\n", (long long)checked);
}

/* ---------- entry point ---------- */

int main(void) {
    printf("🧪 string_runtime_tests\n");
    test_is_empty();
    test_starts_with();
    test_ends_with();
    test_index_of();
    test_take();
    test_drop();
    test_substring();
    test_to_upper_lower();
    test_trim();
    test_reverse();
    test_replace();
    test_repeat();
    test_pad();
    test_parse_int();
    test_parse_float();
    test_split();
    test_lines();
    test_words();
    test_join();
    /* adversarial coverage */
    test_long_strings();
    test_round_trips();
    test_unicode_bytes();
    test_repeated_replace();
    test_split_pathological();
    test_pad_overflow_guards();
    test_parse_int_signs();
    test_cursor_byte_length();
    test_cursor_byte_at();
    test_cursor_codepoint_at();
    test_cursor_codepoint_width();
    test_cursor_from_codepoint();
    test_cursor_error_messages();
    test_cursor_roundtrip_exhaustive();
    printf("✅ all string_runtime tests passed\n");
    return 0;
}
