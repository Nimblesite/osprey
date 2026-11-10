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
    assert(osp_parse_float_strict("",        &out) != 0);
    assert(osp_parse_float_strict("abc",     &out) != 0);
    assert(osp_parse_float_strict("3.14x",   &out) != 0); /* trailing junk */
    assert(osp_parse_float_strict(" 3.14",   &out) != 0); /* surrounding whitespace */
    assert(osp_parse_float_strict("3.14 ",   &out) != 0);
    assert(osp_parse_float_strict("nan",     &out) != 0); /* non-decimal spellings */
    assert(osp_parse_float_strict("NaN",     &out) != 0);
    assert(osp_parse_float_strict("inf",     &out) != 0);
    assert(osp_parse_float_strict("-Infinity", &out) != 0);
    assert(osp_parse_float_strict("0x1p2",   &out) != 0); /* hexadecimal float */
    assert(osp_parse_float_strict("1e9999",  &out) != 0); /* non-finite result */
    assert(osp_parse_float_strict(".",       &out) != 0);
    assert(osp_parse_float_strict("1e",      &out) != 0);
    assert(osp_parse_float_strict("1e+",     &out) != 0);
    assert(osp_parse_float_strict(NULL,       &out) != 0);
    assert(osp_parse_float_strict("1.0",     NULL) != 0);
    printf("  ok  parse_float_strict\n");
}

void test_split(void) {
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

void test_lines(void) {
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

void test_words(void) {
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
