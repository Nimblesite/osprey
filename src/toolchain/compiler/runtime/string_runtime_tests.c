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

/* One row of a scalar-predicate table: fn(s, arg) must return want. */
typedef struct {
    const char *s;
    const char *arg;
    int64_t want;
} pred_row;

/* Drives every row through fn ([BUILTIN-STRING-INSPECTION],
   [BUILTIN-STRING-SEARCH]); the first failing row aborts the suite. */
static void check_pred(int64_t (*fn)(const char *, const char *),
                       const pred_row *rows, size_t count) {
    for (size_t i = 0; i < count; i++) {
        assert(fn(rows[i].s, rows[i].arg) == rows[i].want);
    }
}

static void test_starts_with(void) {
    const pred_row rows[] = {
        {"hello world", "hello", 1}, {"hello world", "world", 0},
        {"hello world", "", 1},      {"", "", 1}, {"", "x", 0},
        {"hi", "hello", 0}, /* prefix longer than s */
        {"GET /api", "GET ", 1},
        {NULL, "x", 0}, {"x", NULL, 0},
    };
    check_pred(osp_string_starts_with, rows, sizeof rows / sizeof rows[0]);
    printf("  ok  starts_with\n");
}

static void test_ends_with(void) {
    const pred_row rows[] = {
        {"hello world", "world", 1}, {"hello world", "hello", 0},
        {"hello world", "", 1},      {"", "", 1}, {"", "x", 0},
        {"hi", "hello", 0}, /* suffix longer than s */
        {"image.png", ".png", 1},
        {"image.PNG", ".png", 0}, /* case-sensitive */
    };
    check_pred(osp_string_ends_with, rows, sizeof rows / sizeof rows[0]);
    printf("  ok  ends_with\n");
}

static void test_index_of(void) {
    const pred_row rows[] = {
        {"hello", "ell", 1}, {"hello", "h", 0}, {"hello", "o", 4},
        {"hello", "xyz", -1}, /* not found */
        {"hello", "", 0},    /* empty needle */
        {"foo=bar=baz", "=", 3}, /* first occurrence */
        {NULL, "x", -1}, {"x", NULL, -1},
    };
    check_pred(osp_string_index_of, rows, sizeof rows / sizeof rows[0]);
    printf("  ok  index_of\n");
}

/* ---------- substring helpers ---------- */

static void test_take(void) {
    text_is(osp_string_take("hello", 3), "hel");
    text_is(osp_string_take("hello", 0), "");
    text_is(osp_string_take("hello", -5), "");
    text_is(osp_string_take("hello", 5), "hello");
    text_is(osp_string_take("hello", 99), "hello"); /* clamp */
    text_is(osp_string_take("", 3), "");
    text_is(osp_string_take(NULL, 3), "");
    /* [BUILTIN-STRING-TAKE]: a cut inside a character moves past it. */
    const char *quote = "\xE2\x80\x9Cok\xE2\x80\x9D"; /* 8 bytes */
    for (int64_t n = 1; n <= 3; n++) {
        text_is(osp_string_take(quote, n), "\xE2\x80\x9C");
    }
    text_is(osp_string_take(quote, 4), "\xE2\x80\x9Co");
    text_is(osp_string_take(quote, 6), quote);
    text_is(osp_string_take("\xF0\x9F\xA6\x85x", 2), "\xF0\x9F\xA6\x85");
    printf("  ok  take\n");
}

static void test_drop(void) {
    text_is(osp_string_drop("hello", 3), "lo");
    text_is(osp_string_drop("hello", 0), "hello");
    text_is(osp_string_drop("hello", -5), "hello");
    text_is(osp_string_drop("hello", 5), ""); /* exact */
    text_is(osp_string_drop("hello", 99), ""); /* clamp */
    text_is(osp_string_drop("", 3), "");
    text_is(osp_string_drop(NULL, 3), "");
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
    text_is(osp_string_substring("hello", 1, 4), "ell");
    text_is(osp_string_substring("hello", 0, 5), "hello");
    text_is(osp_string_substring("hello", 0, 0), "");
    text_is(osp_string_substring("hello", 5, 5), "");
    /* error cases: must return NULL */
    assert(osp_string_substring("hello", -1, 3) == NULL); /* start < 0 */
    assert(osp_string_substring("hello", 0, 99) == NULL); /* end > len */
    assert(osp_string_substring("hello", 4, 2)  == NULL); /* end < start */
    assert(osp_string_substring(NULL, 0, 1)     == NULL);
    printf("  ok  substring\n");
}

/* ---------- transformation ---------- */

/* One row of a text-transform table: fn(in) must produce want. */
typedef struct {
    const char *in;
    const char *want;
} text_row;

/* Drives every row through fn, asserting and releasing each result
   ([BUILTIN-STRING-TRANSFORM], [BUILTIN-STRING-TOUPPERCASE]). */
static void check_text(char *(*fn)(const char *), const text_row *rows, size_t count) {
    for (size_t i = 0; i < count; i++) text_is(fn(rows[i].in), rows[i].want);
}

static void test_to_upper_lower(void) {
    const text_row upper[] = {
        {"hello", "HELLO"}, {"", ""}, {"AbC", "ABC"},
        {"1!@a", "1!@A"}, {NULL, ""},
    };
    const text_row lower[] = {{"HELLO", "hello"}, {"", ""}, {"AbC", "abc"}};
    check_text(osp_string_to_upper, upper, sizeof upper / sizeof upper[0]);
    check_text(osp_string_to_lower, lower, sizeof lower / sizeof lower[0]);
    printf("  ok  to_upper/to_lower\n");
}

/* One row for trim_start/trim_end, which share their inputs: each row pins
   both trimmed sides of `in` ([BUILTIN-STRING-TRIMSTART],
   [BUILTIN-STRING-TRIMEND]). */
typedef struct {
    const char *in;
    const char *start;
    const char *end;
} trim_row;

static void check_trim_sides(const trim_row *rows, size_t count) {
    for (size_t i = 0; i < count; i++) {
        text_is(osp_string_trim_start(rows[i].in), rows[i].start);
        text_is(osp_string_trim_end(rows[i].in), rows[i].end);
    }
}

static void test_trim(void) {
    const text_row trim[] = {
        {"  hello  ", "hello"}, {"\t\n hi \r\n", "hi"}, {"hello", "hello"},
        {"", ""}, {"   ", ""}, /* all ws */ {NULL, ""},
    };
    const trim_row sides[] = {
        {"  hello  ", "hello  ", "  hello"},
        {"hello", "hello", "hello"},
        {"   ", "", ""},
    };
    check_text(osp_string_trim, trim, sizeof trim / sizeof trim[0]);
    check_trim_sides(sides, sizeof sides / sizeof sides[0]);
    printf("  ok  trim/trim_start/trim_end\n");
}

static void test_reverse(void) {
    text_is(osp_string_reverse("abc"), "cba");
    text_is(osp_string_reverse("a"), "a");
    text_is(osp_string_reverse(""), "");
    text_is(osp_string_reverse("12345"), "54321");
    text_is(osp_string_reverse(NULL), "");
    printf("  ok  reverse\n");
}

static void test_replace(void) {
    text_is(osp_string_replace("a-b-c", "-", "_"), "a_b_c");
    text_is(osp_string_replace("aaa", "a", "bb"), "bbbbbb");
    text_is(osp_string_replace("hello", "xyz", "Q"), "hello"); /* no match */
    text_is(osp_string_replace("hello", "l", ""), "heo"); /* shrink */
    text_is(osp_string_replace("", "x", "y"), "");
    /* error cases */
    assert(osp_string_replace("hello", "", "x")    == NULL); /* empty needle */
    assert(osp_string_replace(NULL,    "x", "y")   == NULL);
    assert(osp_string_replace("h",     NULL, "y")  == NULL);
    assert(osp_string_replace("h",     "x", NULL)  == NULL);
    printf("  ok  replace\n");
}

static void test_repeat(void) {
    text_is(osp_string_repeat("ab", 3), "ababab");
    text_is(osp_string_repeat("x", 5), "xxxxx");
    text_is(osp_string_repeat("ab", 0), ""); /* n=0 -> empty */
    text_is(osp_string_repeat("ab", 1), "ab");
    text_is(osp_string_repeat("", 99), ""); /* empty source */
    /* error cases */
    assert(osp_string_repeat("ab", -1) == NULL);
    assert(osp_string_repeat(NULL,  3) == NULL);
    printf("  ok  repeat\n");
}

/* One row of a padding table: fn(in, target_length, fill) must produce want. */
typedef struct {
    const char *in;
    int64_t target_length;
    const char *fill;
    const char *want;
} pad_row;

static void check_pad(char *(*fn)(const char *, int64_t, const char *),
                      const pad_row *rows, size_t count) {
    for (size_t i = 0; i < count; i++) {
        text_is(fn(rows[i].in, rows[i].target_length, rows[i].fill), rows[i].want);
    }
}

static void test_pad(void) {
    const pad_row start[] = {
        {"7", 3, "0", "007"}, {"42", 5, "ab", "aba42"},
        {"hi", 2, "x", "hi"}, /* no pad needed */
        {"hi", 1, "x", "hi"}, /* target < len */
        {"", 3, "0", "000"},
    };
    const pad_row end[] = {
        {"7", 3, ".", "7.."}, {"42", 5, "ab", "42aba"},
        {"hi", 2, "x", "hi"},
    };
    check_pad(osp_string_pad_start, start, sizeof start / sizeof start[0]);
    check_pad(osp_string_pad_end, end, sizeof end / sizeof end[0]);

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
