// String parsing, splitting and joining: parseInt and parseFloat over every
// sign, prefix and byte mutation, split/lines/words, and join. Part of the string_runtime suite.

#include "string_tests_shared.h"

/* One strict-parse verdict per call. 71 is the untouched-output sentinel. */
static void int_is(const char *text, int64_t want) {
    int64_t v = 71;
    assert(osp_parse_int_strict(text, &v) == 0); assert(v == want);
}

static void int_rejects(const char *text) {
    int64_t v = 71;
    assert(osp_parse_int_strict(text, &v) != 0); assert(v == 71);
}

static void float_is(const char *text, double want) {
    double out;
    assert(osp_parse_float_strict(text, &out) == 0); assert(out == want);
}

static void float_between(const char *text, double lo, double hi) {
    double out;
    assert(osp_parse_float_strict(text, &out) == 0); assert(out > lo && out < hi);
}

/* Every NULL-terminated entry is rejected. */
static void all_int_rejected(const char *const *texts) {
    for (size_t i = 0; texts[i] != NULL; i++) int_rejects(texts[i]);
}

static void all_float_rejected(const char *const *texts) {
    double out;
    for (size_t i = 0; texts[i] != NULL; i++) assert(osp_parse_float_strict(texts[i], &out) != 0);
}

void test_parse_int(void) {
    int_is("42", 42);
    int_is("-42", -42);
    int_is("+42", 42);
    int_is("0", 0);
    int_is("9223372036854775807", INT64_MAX);
    int_is("-9223372036854775808", INT64_MIN);

    /* rejections */
    static const char *const rejected[] = {
        "",
        "abc",
        "12abc",
        "abc12",
        " 42", /* leading space */
        "42 ", /* trailing space */
        "-", /* sign with no digits */
        "+",
        "9223372036854775808", /* overflow */
        "-9223372036854775809", /* underflow */
        NULL,
    };
    all_int_rejected(rejected);
    printf("  ok  parse_int_strict\n");
}

void test_parse_float(void) {
    float_between("3.14", 3.13, 3.15);
    float_is("0", 0.0);
    float_between("-2.5", -2.51, -2.49);
    float_between("1e3", 999.9, 1000.1);
    float_is(".5", 0.5);
    float_is("5.", 5.0);
    float_between("+1.25E-2", 0.0124, 0.0126);

    /* rejections */
    static const char *const rejected[] = {
        "",
        "abc",
        "3.14x", /* trailing junk */
        " 3.14", /* surrounding whitespace */
        "3.14 ",
        "nan", /* non-decimal spellings */
        "NaN",
        "inf",
        "-Infinity",
        "0x1p2", /* hexadecimal float */
        "1e9999", /* non-finite result */
        ".",
        "1e",
        "1e+",
        NULL,
    };
    all_float_rejected(rejected);
    assert(osp_parse_float_strict("1.0",     NULL) != 0);
    printf("  ok  parse_float_strict\n");
}

void test_split(void) {
    list_is(osp_string_split("a,b,c", ","), (const char *[]){"a", "b", "c", NULL});
    list_is(osp_string_split("hello", ","), (const char *[]){"hello", NULL});
    list_is(osp_string_split(",,", ","), (const char *[]){"", "", "", NULL});
    list_is(osp_string_split("foo::bar::baz", "::"), (const char *[]){"foo", "bar", "baz", NULL});

    /* error: empty separator */
    assert(osp_string_split("hello", "") == NULL);
    assert(osp_string_split(NULL,    ",") == NULL);
    printf("  ok  split\n");
}

void test_lines(void) {
    list_is(osp_string_lines("a\nb\nc"), (const char *[]){"a", "b", "c", NULL});

    /* trailing newline does NOT produce an empty final entry */
    list_is(osp_string_lines("a\nb\n"), (const char *[]){"a", "b", NULL});
    list_is(osp_string_lines(""), (const char *[]){NULL});
    list_is(osp_string_lines("single"), (const char *[]){"single", NULL});
    list_is(osp_string_lines(NULL), (const char *[]){NULL});
    printf("  ok  lines\n");
}

void test_words(void) {
    list_is(osp_string_words("a b c"), (const char *[]){"a", "b", "c", NULL});

    /* runs of whitespace collapse; empties dropped */
    list_is(osp_string_words("  hello\t\tworld  \n  goodbye  "), (const char *[]){"hello", "world", "goodbye", NULL});
    list_is(osp_string_words(""), (const char *[]){NULL});
    list_is(osp_string_words("   \t\n\r   "), (const char *[]){NULL});
    list_is(osp_string_words(NULL), (const char *[]){NULL});
    printf("  ok  words\n");
}

/* Splits `text` on `sep`, joins with `glue`, expects `want`. */
static void join_round_trip(const char *text, const char *sep, const char *glue, const char *want) {
    osp_string_list *list = osp_string_split(text, sep);
    text_is(osp_string_join(list, glue), want);
    osp_string_list_free(list);
}

void test_join(void) {
    /* Build a list manually and join it. */
    join_round_trip("a,b,c", ",", "-", "a-b-c");

    /* join round-trips split */
    join_round_trip("foo::bar::baz", "::", "::", "foo::bar::baz");

    /* empty list → "" */
    join_round_trip("", ",", "x", "");

    printf("  ok  join\n");
}

/* Append a borrowed word to a runtime list the way codegen stores strings. */
static OspreyList *push_word(OspreyList *list, const char *word) {
    return osprey_list_append(list, (int64_t)(intptr_t)word);
}

void test_join_runtime_list(void) {
    /* NULL list → "" */
    text_is(osp_string_join_list(NULL, ", "), "");

    /* empty runtime list → "" */
    OspreyList *list = osprey_list_empty();
    text_is(osp_string_join_list(list, ", "), "");

    /* one word: the separator is never applied */
    list = push_word(list, "solo");
    text_is(osp_string_join_list(list, "-"), "solo");

    /* two words with a separator */
    const char second[] = "duo";
    list = push_word(list, second);
    char *out = osp_string_join_list(list, "-");
    assert(strcmp(out, "solo-duo") == 0);
    /* the result is a fresh allocation the caller owns, never an element */
    assert(out != second && out != (char *)(intptr_t)osprey_list_get(list, 0));
    out[0] = 'S';
    assert(strcmp(second, "duo") == 0);
    release_test_buffer(out);

    printf("  ok  join_runtime_list\n");
}

void test_parse_int_signs(void) {
    /* every legal digit pair */
    int_is("000", 0);
    int_is("-000", 0);
    int_is("+000", 0);
    int_is("1", 1);
    int_is("-1", -1);
    /* boundary at INT64_MAX one-off */
    int_is("9223372036854775806", 9223372036854775806LL); /* MAX-1 */
    int_is("9223372036854775807", INT64_MAX); /* MAX */
    /* one digit past MAX still rejects */
    int_rejects("9223372036854775808");
    /* INT64_MIN */
    int_is("-9223372036854775808", INT64_MIN);
    /* one digit past MIN rejects */
    int_rejects("-9223372036854775809");
    /* leading zeros are OK */
    int_is("00042", 42);
    int_is("-00042", -42);
    /* lone sign, sign + non-digit, embedded sign, hex, unicode digit rejected */
    static const char *const rejected[] = {"-", "+", "-x", "1-2", "0x10",
        "\xef\xbc\x91", /* full-width '1' */ NULL};
    all_int_rejected(rejected);
    printf("  ok  parse_int_signs (boundary + sign + leading zeros + rejection)\n");
}

void test_parse_int_long_prefixes(void) {
    const char *accepted[] = {"0000000000000000000000000000000000000",
        "+000000000000000000009223372036854775807",
        "-000000000000000000009223372036854775808", "999999999", "1000000000",
        "1000000001", "999999999999999999", "1000000000000000000"};
    const int64_t expected[] = {0, INT64_MAX, INT64_MIN, 999999999, 1000000000,
        1000000001, 999999999999999999LL, 1000000000000000000LL};
    const char *const rejected[] = {"000000000000000000009223372036854775808",
        "-000000000000000000009223372036854775809",
        "10000000000000000000", "-10000000000000000000",
        "00000000000000000000x", "123456789012345678x", "-000+1", NULL};
    for (size_t i = 0; i < sizeof(accepted) / sizeof(*accepted); i++) int_is(accepted[i], expected[i]);
    all_int_rejected(rejected);
}

/* libc supplies the independent value/range oracle; Osprey rejects whitespace. */
static void check_parse_int_oracle(const char *text) {
    char *end = NULL;
    errno = 0;
    long long expected = strtoll(text, &end, 10);
    int valid = end != text && *end == 0 && errno != ERANGE && !isspace((unsigned char)*text);
    int64_t value = 71;
    assert((osp_parse_int_strict(text, &value) == 0) == valid);
    assert(value == (valid ? (int64_t)expected : 71));
}

void test_parse_int_byte_mutations(void) {
    const char *seeds[] = {"9223372036854775807", "-9223372036854775808",
        "+000000000000000000000000000000000000000000009223372036854775807"};
    for (size_t seed = 0; seed < sizeof(seeds) / sizeof(*seeds); seed++) {
        for (size_t at = 0; at < strlen(seeds[seed]); at++) {
            for (unsigned byte = 0; byte <= 255; byte++) {
                char text[96];
                strcpy(text, seeds[seed]);
                text[at] = (char)byte;
                check_parse_int_oracle(text);
            }
        }
    }
    assert(osp_parse_int_strict("1", NULL) != 0);
}
