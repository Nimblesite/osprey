// Adversarial strings: very long inputs, round trips, raw UTF-8 bytes,
// pathological replace/split and pad overflow guards. Part of the string_runtime suite.

#include "string_tests_shared.h"

/* Build a string of `n` 'a' chars heap-allocated. Caller frees. */
static char *make_repeat_a(size_t n) {
    char *s = (char *)malloc(n + 1);
    assert(s != NULL);
    for (size_t i = 0; i < n; i++) s[i] = 'a';
    s[n] = '\0';
    return s;
}

void test_long_strings(void) {
    /* length 10000: ensure no stack/buffer assumption breaks */
    const size_t big = 10000;
    char *s = make_repeat_a(big);

    char *out = osp_string_to_upper(s);
    assert(out != NULL);
    for (size_t i = 0; i < big; i++) assert(out[i] == 'A');
    assert(out[big] == '\0');
    release_test_buffer(out);

    out = osp_string_reverse(s);
    assert(out != NULL);
    for (size_t i = 0; i < big; i++) assert(out[i] == 'a');
    release_test_buffer(out);

    out = osp_string_take(s, 1000);
    assert(out != NULL);
    assert(strlen(out) == 1000);
    release_test_buffer(out);

    out = osp_string_drop(s, 9999);
    assert(strlen(out) == 1);
    assert(out[0] == 'a');
    release_test_buffer(out);

    /* trim no-op on a long non-whitespace string */
    out = osp_string_trim(s);
    assert(strlen(out) == big);
    release_test_buffer(out);

    release_test_buffer(s);
    printf("  ok  long_strings (10000 chars)\n");
}

void test_round_trips(void) {
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
        release_test_buffer(rebuilt);
        release_test_buffer(left);
        release_test_buffer(right);
    }

    /* reverse(reverse(s)) == s for every length up to 64 */
    char buf[65];
    for (size_t len = 0; len <= 64; len++) {
        for (size_t i = 0; i < len; i++) buf[i] = (char)('a' + (i % 26));
        buf[len] = '\0';
        char *r1 = osp_string_reverse(buf);
        char *r2 = osp_string_reverse(r1);
        assert(strcmp(r2, buf) == 0);
        release_test_buffer(r1);
        release_test_buffer(r2);
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
            release_test_buffer(back);
            osp_string_list_free(list);
        }
    }

    printf("  ok  round_trips (take/drop, reverse twice, split/join)\n");
}

void test_unicode_bytes(void) {
    /* The general string operations are byte-oriented; the separate cursor
     * operations validate and decode UTF-8 codepoints. */
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

void test_repeated_replace(void) {
    /* replace many times, growing or shrinking */
    char *out;
    out = osp_string_replace("xxxxxxxxxx", "x", "ab");
    assert(strcmp(out, "abababababababababab") == 0);
    release_test_buffer(out);
    out = osp_string_replace("xxxxxxxxxx", "x", "");
    assert(strcmp(out, "") == 0);
    release_test_buffer(out);
    /* needle longer than replacement, exactly N matches */
    out = osp_string_replace("xyxyxyxy", "xy", "z");
    assert(strcmp(out, "zzzz") == 0);
    release_test_buffer(out);
    /* overlapping-ish needles */
    out = osp_string_replace("aaaa", "aa", "b");
    assert(strcmp(out, "bb") == 0);  /* non-overlapping leftmost */
    release_test_buffer(out);
    /* multi-char replacement that contains the needle (no infinite recursion) */
    out = osp_string_replace("aaa", "a", "aa");
    assert(strcmp(out, "aaaaaa") == 0);
    release_test_buffer(out);
    printf("  ok  repeated_replace\n");
}

void test_split_pathological(void) {
    /* separator at very start */
    list_is(osp_string_split(",a,b", ","), (const char *[]){"", "a", "b", NULL});

    /* separator at very end */
    list_is(osp_string_split("a,b,", ","), (const char *[]){"a", "b", "", NULL});

    /* separator IS the whole string */
    list_is(osp_string_split(",", ","), (const char *[]){"", "", NULL});

    /* empty input */
    list_is(osp_string_split("", ","), (const char *[]){"", NULL});

    /* sep longer than input */
    list_is(osp_string_split("ab", "abcdef"), (const char *[]){"ab", NULL});

    printf("  ok  split_pathological\n");
}

void test_pad_overflow_guards(void) {
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
    release_test_buffer(out);

    out = osp_string_pad_end("x", 100, "abc");
    assert(out != NULL);
    assert(strlen(out) == 100);
    assert(out[0] == 'x');
    assert(out[1] == 'a');
    assert(out[2] == 'b');
    assert(out[3] == 'c');
    assert(out[4] == 'a');
    release_test_buffer(out);

    /* exact-match target_length == strlen(s) returns dup of s */
    out = osp_string_pad_start("abc", 3, "x");
    assert(strcmp(out, "abc") == 0);
    release_test_buffer(out);
    printf("  ok  pad_overflow_guards\n");
}
