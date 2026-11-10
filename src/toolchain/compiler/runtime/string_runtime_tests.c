/*
 * Verifies [BUILTIN-STRING-INSPECTION], [BUILTIN-STRING-SEARCH],
 * [BUILTIN-STRING-CURSOR], [BUILTIN-STRING-SUBSTRINGS], [BUILTIN-STRING-LIST],
 * [BUILTIN-STRING-TRANSFORM], [BUILTIN-STRING-TOUPPERCASE],
 * [BUILTIN-STRING-TRIMSTART], [BUILTIN-STRING-TRIMEND], and
 * [BUILTIN-STRING-PARSING].
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

#include "string_tests_shared.h"

// Runtime-produced text belongs to the selected manager; fixture buffers remain libc-owned.
void release_test_buffer(void *p) {
    if (osp_mem_string_length(p) >= 0) osp_release(p);
    else free(p);
}

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
    out = osp_string_take("hello", 3);  assert(strcmp(out, "hel") == 0);   release_test_buffer(out);
    out = osp_string_take("hello", 0);  assert(strcmp(out, "")    == 0);   release_test_buffer(out);
    out = osp_string_take("hello", -5); assert(strcmp(out, "")    == 0);   release_test_buffer(out);
    out = osp_string_take("hello", 5);  assert(strcmp(out, "hello") == 0); release_test_buffer(out);
    out = osp_string_take("hello", 99); assert(strcmp(out, "hello") == 0); release_test_buffer(out); /* clamp */
    out = osp_string_take("", 3);       assert(strcmp(out, "")    == 0);   release_test_buffer(out);
    out = osp_string_take(NULL, 3);     assert(strcmp(out, "")    == 0);   release_test_buffer(out);
    /* [BUILTIN-STRING-TAKE]: a cut inside a character moves past it. */
    const char *quote = "\xE2\x80\x9Cok\xE2\x80\x9D"; /* 8 bytes */
    for (int64_t n = 1; n <= 3; n++) {
        out = osp_string_take(quote, n); assert(strcmp(out, "\xE2\x80\x9C") == 0); release_test_buffer(out);
    }
    out = osp_string_take(quote, 4); assert(strcmp(out, "\xE2\x80\x9Co") == 0); release_test_buffer(out);
    out = osp_string_take(quote, 6); assert(strcmp(out, quote) == 0); release_test_buffer(out);
    out = osp_string_take("\xF0\x9F\xA6\x85x", 2); assert(strcmp(out, "\xF0\x9F\xA6\x85") == 0); release_test_buffer(out);
    printf("  ok  take\n");
}

static void test_drop(void) {
    char *out;
    out = osp_string_drop("hello", 3);  assert(strcmp(out, "lo")   == 0); release_test_buffer(out);
    out = osp_string_drop("hello", 0);  assert(strcmp(out, "hello") == 0); release_test_buffer(out);
    out = osp_string_drop("hello", -5); assert(strcmp(out, "hello") == 0); release_test_buffer(out);
    out = osp_string_drop("hello", 5);  assert(strcmp(out, "")    == 0);  release_test_buffer(out); /* exact */
    out = osp_string_drop("hello", 99); assert(strcmp(out, "")    == 0);  release_test_buffer(out); /* clamp */
    out = osp_string_drop("", 3);       assert(strcmp(out, "")    == 0);  release_test_buffer(out);
    out = osp_string_drop(NULL, 3);     assert(strcmp(out, "")    == 0);  release_test_buffer(out);
    printf("  ok  drop\n");
    /* [BUILTIN-STRING-DROP]: the complement of take at every byte count. */
    const char *quote = "\xE2\x80\x9Cok\xE2\x80\x9D";
    for (int64_t n = 0; n <= 9; n++) {
        char *head = osp_string_take(quote, n);
        char *tail = osp_string_drop(quote, n);
        assert(strlen(head) + strlen(tail) == strlen(quote));
        assert(strncmp(quote, head, strlen(head)) == 0);
        assert(strcmp(quote + strlen(head), tail) == 0);
        release_test_buffer(head);
        release_test_buffer(tail);
    }
}

static void test_substring(void) {
    char *out;
    out = osp_string_substring("hello", 1, 4); assert(strcmp(out, "ell") == 0); release_test_buffer(out);
    out = osp_string_substring("hello", 0, 5); assert(strcmp(out, "hello") == 0); release_test_buffer(out);
    out = osp_string_substring("hello", 0, 0); assert(strcmp(out, "") == 0); release_test_buffer(out);
    out = osp_string_substring("hello", 5, 5); assert(strcmp(out, "") == 0); release_test_buffer(out);
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
    out = osp_string_to_upper("hello"); assert(strcmp(out, "HELLO") == 0); release_test_buffer(out);
    out = osp_string_to_upper("");      assert(strcmp(out, "")      == 0); release_test_buffer(out);
    out = osp_string_to_upper("AbC");   assert(strcmp(out, "ABC")   == 0); release_test_buffer(out);
    out = osp_string_to_upper("1!@a");  assert(strcmp(out, "1!@A")  == 0); release_test_buffer(out);
    out = osp_string_to_upper(NULL);    assert(strcmp(out, "")      == 0); release_test_buffer(out);

    out = osp_string_to_lower("HELLO"); assert(strcmp(out, "hello") == 0); release_test_buffer(out);
    out = osp_string_to_lower("");      assert(strcmp(out, "")      == 0); release_test_buffer(out);
    out = osp_string_to_lower("AbC");   assert(strcmp(out, "abc")   == 0); release_test_buffer(out);
    printf("  ok  to_upper/to_lower\n");
}

static void test_trim(void) {
    char *out;
    out = osp_string_trim("  hello  ");      assert(strcmp(out, "hello") == 0); release_test_buffer(out);
    out = osp_string_trim("\t\n hi \r\n");   assert(strcmp(out, "hi")    == 0); release_test_buffer(out);
    out = osp_string_trim("hello");          assert(strcmp(out, "hello") == 0); release_test_buffer(out);
    out = osp_string_trim("");               assert(strcmp(out, "")      == 0); release_test_buffer(out);
    out = osp_string_trim("   ");            assert(strcmp(out, "")      == 0); release_test_buffer(out); /* all ws */
    out = osp_string_trim(NULL);             assert(strcmp(out, "")      == 0); release_test_buffer(out);

    out = osp_string_trim_start("  hello  "); assert(strcmp(out, "hello  ") == 0); release_test_buffer(out);
    out = osp_string_trim_start("hello");     assert(strcmp(out, "hello")   == 0); release_test_buffer(out);
    out = osp_string_trim_start("   ");       assert(strcmp(out, "")        == 0); release_test_buffer(out);

    out = osp_string_trim_end("  hello  ");   assert(strcmp(out, "  hello") == 0); release_test_buffer(out);
    out = osp_string_trim_end("hello");       assert(strcmp(out, "hello")   == 0); release_test_buffer(out);
    out = osp_string_trim_end("   ");         assert(strcmp(out, "")        == 0); release_test_buffer(out);
    printf("  ok  trim/trim_start/trim_end\n");
}

static void test_reverse(void) {
    char *out;
    out = osp_string_reverse("abc");   assert(strcmp(out, "cba")    == 0); release_test_buffer(out);
    out = osp_string_reverse("a");     assert(strcmp(out, "a")      == 0); release_test_buffer(out);
    out = osp_string_reverse("");      assert(strcmp(out, "")       == 0); release_test_buffer(out);
    out = osp_string_reverse("12345"); assert(strcmp(out, "54321")  == 0); release_test_buffer(out);
    out = osp_string_reverse(NULL);    assert(strcmp(out, "")       == 0); release_test_buffer(out);
    printf("  ok  reverse\n");
}

static void test_replace(void) {
    char *out;
    out = osp_string_replace("a-b-c", "-", "_");        assert(strcmp(out, "a_b_c") == 0); release_test_buffer(out);
    out = osp_string_replace("aaa", "a", "bb");         assert(strcmp(out, "bbbbbb") == 0); release_test_buffer(out);
    out = osp_string_replace("hello", "xyz", "Q");      assert(strcmp(out, "hello") == 0); release_test_buffer(out); /* no match */
    out = osp_string_replace("hello", "l", "");         assert(strcmp(out, "heo") == 0); release_test_buffer(out); /* shrink */
    out = osp_string_replace("", "x", "y");             assert(strcmp(out, "") == 0); release_test_buffer(out);
    /* error cases */
    assert(osp_string_replace("hello", "", "x")    == NULL); /* empty needle */
    assert(osp_string_replace(NULL,    "x", "y")   == NULL);
    assert(osp_string_replace("h",     NULL, "y")  == NULL);
    assert(osp_string_replace("h",     "x", NULL)  == NULL);
    printf("  ok  replace\n");
}

static void test_repeat(void) {
    char *out;
    out = osp_string_repeat("ab", 3);   assert(strcmp(out, "ababab") == 0); release_test_buffer(out);
    out = osp_string_repeat("x", 5);    assert(strcmp(out, "xxxxx") == 0); release_test_buffer(out);
    out = osp_string_repeat("ab", 0);   assert(strcmp(out, "") == 0); release_test_buffer(out); /* n=0 -> empty */
    out = osp_string_repeat("ab", 1);   assert(strcmp(out, "ab") == 0); release_test_buffer(out);
    out = osp_string_repeat("", 99);    assert(strcmp(out, "") == 0); release_test_buffer(out); /* empty source */
    /* error cases */
    assert(osp_string_repeat("ab", -1) == NULL);
    assert(osp_string_repeat(NULL,  3) == NULL);
    printf("  ok  repeat\n");
}

static void test_pad(void) {
    char *out;
    out = osp_string_pad_start("7",   3, "0");  assert(strcmp(out, "007") == 0); release_test_buffer(out);
    out = osp_string_pad_start("42",  5, "ab"); assert(strcmp(out, "aba42") == 0); release_test_buffer(out);
    out = osp_string_pad_start("hi",  2, "x");  assert(strcmp(out, "hi") == 0); release_test_buffer(out); /* no pad needed */
    out = osp_string_pad_start("hi",  1, "x");  assert(strcmp(out, "hi") == 0); release_test_buffer(out); /* target < len */
    out = osp_string_pad_start("",    3, "0");  assert(strcmp(out, "000") == 0); release_test_buffer(out);

    out = osp_string_pad_end  ("7",   3, ".");  assert(strcmp(out, "7..") == 0); release_test_buffer(out);
    out = osp_string_pad_end  ("42",  5, "ab"); assert(strcmp(out, "42aba") == 0); release_test_buffer(out);
    out = osp_string_pad_end  ("hi",  2, "x");  assert(strcmp(out, "hi") == 0); release_test_buffer(out);

    /* error: empty fill */
    assert(osp_string_pad_start("hi", 5, "") == NULL);
    assert(osp_string_pad_end  ("hi", 5, "") == NULL);
    assert(osp_string_pad_start("hi", 5, NULL) == NULL);
    printf("  ok  pad_start/pad_end\n");
}

/* `join` over the RUNTIME list layout (listAppend, toList, a settled
   `let`-bound string list) must agree with the flat-literal join. */

/* ---------- length, interpolation, internals ---------- */

/* [BUILTIN-STRING-LENGTH] osp_strlen wraps strlen behind a fixed int64. */
static void test_strlen(void) {
    assert(osp_strlen("") == 0);
    assert(osp_strlen("a") == 1);
    assert(osp_strlen("hello world") == 11);
    assert(osp_strlen("caf\xC3\xA9") == 5); /* BYTES, not codepoints */
}

/* osp_float_to_string keeps float-ness visible: whole values gain ".0",
   scientific/NaN/inf spellings pass through, %.10g precision holds. */

/* The spelling toString(float) has always had, as the plain definition the
   fast path must reproduce byte for byte: the smallest precision from 10 to
   17 whose %g spelling reads back as the same double, plus ".0" when that
   spelling would otherwise look like an integer. */

/* toString(float) runs for every interpolated float and every float a JSON
   document encodes, so it must not search every precision in turn -- yet it
   must keep the exact spelling. This pins the fast path to the reference
   over every power of two (where the round-trip interval is lopsided), the
   whole-number boundary, short decimals, subnormals and a million random
   bit patterns. [BUILTIN-STRING] */

/* [STRING-INTERPOLATION] the measure/write pair: size matches the write
   exactly, the write NUL-terminates within cap, and hostile caps are safe. */
static void test_format_measure_write(void) {
    int64_t need = osp_format_size("%s|%s", "ab", "cde");
    assert(need == 6);
    char buf[16];
    memset(buf, 'Z', sizeof(buf));
    osp_format_into(buf, (int64_t)sizeof(buf), "%s|%s", "ab", "cde");
    assert(strcmp(buf, "ab|cde") == 0);
    assert(osp_format_size("%s", "") == 0);
    osp_format_into(buf, 4, "%s", "abcdef"); /* truncates, still terminated */
    assert(strcmp(buf, "abc") == 0);
    osp_format_into(NULL, 8, "%s", "x");  /* NULL buffer: no-op */
    osp_format_into(buf, 0, "%s", "x");   /* zero cap: no-op */
    osp_format_into(buf, -5, "%s", "x");  /* negative cap: no-op */
    assert(strcmp(buf, "abc") == 0);      /* untouched by the no-ops */
}

/* The internal helpers shared across TUs: bounded dup, fresh empty string,
   and the exact whitespace set trim/words rely on. */
static void test_internal_helpers(void) {
    char *d = osp_string_dup_internal("hello", 3);
    assert(d != NULL && strcmp(d, "hel") == 0);
    release_test_buffer(d);
    char *z = osp_string_dup_internal("", 0);
    assert(z != NULL && z[0] == '\0');
    release_test_buffer(z);
    char *e1 = osp_string_empty_internal();
    char *e2 = osp_string_empty_internal();
    assert(e1 != NULL && e2 != NULL && e1[0] == '\0' && e2[0] == '\0');
    release_test_buffer(e1);
    release_test_buffer(e2);
    assert(osp_is_ws_internal(' ') != 0);
    assert(osp_is_ws_internal('\t') != 0);
    assert(osp_is_ws_internal('\n') != 0);
    assert(osp_is_ws_internal('\r') != 0);
    assert(osp_is_ws_internal('a') == 0);
    assert(osp_is_ws_internal('0') == 0);
    assert(osp_is_ws_internal(0xC3) == 0); /* UTF-8 lead byte is not ws */
}

/* [PORT-STRING-NUL-CODEGEN] Exercise the native comparison and join ABI. */
static void test_compare_and_concat(void) {
    const char *const bytes[] = {"", "\0", "a", "a\0", "a\0b", "a\0c", "b", "\xc3\xa9"};
    const size_t lengths[] = {0, 1, 1, 2, 3, 3, 1, 2};
    char *strings[sizeof lengths / sizeof lengths[0]];
    for (size_t i = 0; i < sizeof strings / sizeof strings[0]; ++i) {
        strings[i] = osp_string_dup_internal(bytes[i], lengths[i]);
        assert(strings[i] != NULL && osp_string_compare(strings[i], strings[i]) == 0);
    }
    for (size_t i = 0; i < sizeof strings / sizeof strings[0]; ++i) {
        for (size_t j = 0; j < sizeof strings / sizeof strings[0]; ++j) {
            int order = osp_string_compare(strings[i], strings[j]);
            assert(((order > 0) - (order < 0)) == ((i > j) - (i < j)));
            char *joined = osp_string_concat(strings[i], strings[j]);
            assert(joined != NULL && osp_strlen(joined) == (int64_t)(lengths[i] + lengths[j]));
            assert(memcmp(joined, bytes[i], lengths[i]) == 0);
            assert(memcmp(joined + lengths[i], bytes[j], lengths[j]) == 0);
            assert(joined[lengths[i] + lengths[j]] == '\0');
            release_test_buffer(joined);
        }
    }
    assert(osp_string_compare(NULL, NULL) == 0);
    assert(osp_string_compare(NULL, strings[0]) < 0 && osp_string_compare(strings[0], NULL) > 0);
    assert(osp_string_concat(NULL, strings[0]) == NULL && osp_string_concat(strings[0], NULL) == NULL);
    for (size_t i = 0; i < sizeof strings / sizeof strings[0]; ++i) release_test_buffer(strings[i]);
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
    test_join_runtime_list();
    /* adversarial coverage */
    test_long_strings();
    test_round_trips();
    test_unicode_bytes();
    test_repeated_replace();
    test_split_pathological();
    test_pad_overflow_guards();
    test_parse_int_signs();
    test_parse_int_long_prefixes();
    test_parse_int_byte_mutations();
    test_cursor_byte_length();
    test_cursor_byte_at();
    test_cursor_codepoint_at();
    test_cursor_codepoint_width();
    test_cursor_from_codepoint();
    test_cursor_error_messages();
    test_cursor_roundtrip_exhaustive();
    test_strlen();
    test_float_to_string();
    test_float16_conversion();
    test_float16_spelling_roundtrip();
    test_float32_spelling_roundtrip();
    test_sized_float_strings();
    test_float_to_string_matches_reference();
    test_format_measure_write();
    test_internal_helpers();
    test_compare_and_concat();
    printf("✅ all string_runtime tests passed\n");
    return 0;
}
