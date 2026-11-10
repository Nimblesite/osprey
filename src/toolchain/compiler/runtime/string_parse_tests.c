// String parsing, splitting and joining: parseInt and parseFloat over every
// sign, prefix and byte mutation, split/lines/words, and join. Part of the string_runtime suite.

#include "string_tests_shared.h"

void test_parse_int(void) {
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
    for (size_t i = 0; i < sizeof rejected / sizeof rejected[0]; i++) {
        assert(osp_parse_int_strict(rejected[i], &out) != 0);
    }
    printf("  ok  parse_int_strict\n");
}

void test_parse_float(void) {
    double out;
    assert(osp_parse_float_strict("3.14", &out) == 0);   assert(out > 3.13 && out < 3.15);
    assert(osp_parse_float_strict("0", &out) == 0);      assert(out == 0.0);
    assert(osp_parse_float_strict("-2.5", &out) == 0);   assert(out > -2.51 && out < -2.49);
    assert(osp_parse_float_strict("1e3", &out) == 0);    assert(out > 999.9 && out < 1000.1);
    assert(osp_parse_float_strict(".5", &out) == 0);     assert(out == 0.5);
    assert(osp_parse_float_strict("5.", &out) == 0);     assert(out == 5.0);
    assert(osp_parse_float_strict("+1.25E-2", &out) == 0);
    assert(out > 0.0124 && out < 0.0126);

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
    for (size_t i = 0; i < sizeof rejected / sizeof rejected[0]; i++) {
        assert(osp_parse_float_strict(rejected[i], &out) != 0);
    }
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

void test_join(void) {
    /* Build a list manually and join it. */
    osp_string_list *list = osp_string_split("a,b,c", ",");
    char *out = osp_string_join(list, "-");
    assert(strcmp(out, "a-b-c") == 0);
    release_test_buffer(out);
    osp_string_list_free(list);

    /* join round-trips split */
    list = osp_string_split("foo::bar::baz", "::");
    out = osp_string_join(list, "::");
    assert(strcmp(out, "foo::bar::baz") == 0);
    release_test_buffer(out);
    osp_string_list_free(list);

    /* empty list → "" */
    list = osp_string_split("", ",");
    out = osp_string_join(list, "x");
    assert(strcmp(out, "") == 0);
    release_test_buffer(out);
    osp_string_list_free(list);

    printf("  ok  join\n");
}

/* Append a borrowed word to a runtime list the way codegen stores strings. */
static OspreyList *push_word(OspreyList *list, const char *word) {
    return osprey_list_append(list, (int64_t)(intptr_t)word);
}

void test_join_runtime_list(void) {
    /* NULL list → "" */
    char *out = osp_string_join_list(NULL, ", ");
    assert(out != NULL && strcmp(out, "") == 0);
    release_test_buffer(out);

    /* empty runtime list → "" */
    OspreyList *list = osprey_list_empty();
    out = osp_string_join_list(list, ", ");
    assert(out != NULL && strcmp(out, "") == 0);
    release_test_buffer(out);

    /* one word: the separator is never applied */
    list = push_word(list, "solo");
    out = osp_string_join_list(list, "-");
    assert(strcmp(out, "solo") == 0);
    release_test_buffer(out);

    /* two words with a separator */
    const char second[] = "duo";
    list = push_word(list, second);
    out = osp_string_join_list(list, "-");
    assert(strcmp(out, "solo-duo") == 0);
    /* the result is a fresh allocation the caller owns, never an element */
    assert(out != second && out != (char *)(intptr_t)osprey_list_get(list, 0));
    out[0] = 'S';
    assert(strcmp(second, "duo") == 0);
    release_test_buffer(out);

    printf("  ok  join_runtime_list\n");
}

void test_parse_int_signs(void) {
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

void test_parse_int_long_prefixes(void) {
    const char *accepted[] = {"0000000000000000000000000000000000000",
        "+000000000000000000009223372036854775807",
        "-000000000000000000009223372036854775808", "999999999", "1000000000",
        "1000000001", "999999999999999999", "1000000000000000000"};
    const int64_t expected[] = {0, INT64_MAX, INT64_MIN, 999999999, 1000000000,
        1000000001, 999999999999999999LL, 1000000000000000000LL};
    const char *rejected[] = {"000000000000000000009223372036854775808",
        "-000000000000000000009223372036854775809",
        "10000000000000000000", "-10000000000000000000",
        "00000000000000000000x", "123456789012345678x", "-000+1"};
    int64_t value = 0;
    for (size_t i = 0; i < sizeof(accepted) / sizeof(*accepted); i++) {
        assert(osp_parse_int_strict(accepted[i], &value) == 0);
        assert(value == expected[i]);
    }
    for (size_t i = 0; i < sizeof(rejected) / sizeof(*rejected); i++) {
        value = 71;
        assert(osp_parse_int_strict(rejected[i], &value) != 0);
        assert(value == 71);
    }
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
