// The string cursor [BUILTIN-STRING-CURSOR]: byte length and access, code
// points and their widths, and exact error messages. Part of the string_runtime suite.

#include "string_tests_shared.h"

/* A `want` of WANT_REJECTED means the builtin must report an error, not a value. */
enum { WANT_REJECTED = -1 };

/* One (string, index, result) probe of a fallible cursor builtin. */
typedef struct { const char *s; int64_t index; int64_t want; } cursor_probe;
typedef const char *(*cursor_at_fn)(const char *, int64_t, int64_t *);

/* Asserts `err` is NULL and `got == want`, or that the value was rejected. */
static void assert_value(const char *err, int64_t got, int64_t want) {
    const int rejected = want == WANT_REJECTED;
    assert(rejected ? err != NULL : err == NULL && got == want);
}

/* Runs every probe against one fallible cursor builtin. */
static void assert_cursor_probes(cursor_at_fn at, const cursor_probe *probes, size_t n) {
    for (size_t i = 0; i < n; i++) {
        int64_t out = 0;
        const char *err = at(probes[i].s, probes[i].index, &out); /* sequenced before `out` is read */
        assert_value(err, out, probes[i].want);
    }
}

/* Asserts `cp` encodes to exactly the NUL-terminated `bytes`, then frees it. */
static void assert_encodes(int64_t cp, const char *bytes) {
    char *e = osp_string_from_codepoint(cp);
    const size_t len = strlen(bytes);
    assert(e != NULL && osp_string_byte_length(e) == (int64_t)len);
    for (size_t i = 0; i < len; i++) assert((unsigned char)e[i] == (unsigned char)bytes[i]);
    release_test_buffer(e);
}

void test_cursor_byte_length(void) {
    /* byte count per string; longer than its code point count when multi-byte. */
    static const struct { const char *s; int64_t n; } lens[] = {
        {"", 0}, {"a", 1}, {"abc", 3}, {"héllo", 6},                  /* é = 2 bytes */
        {"\xc3\xa9", 2}, {"\xe4\xb8\x96", 3}, {"\xf0\x9f\x98\x80", 4},  /* é, 世, 😀 */
        {"a\xc3\xa9\xe4\xb8\x96\xf0\x9f\x98\x80", 10}, {NULL, 0},
    };
    for (size_t i = 0; i < sizeof lens / sizeof lens[0]; i++)
        assert(osp_string_byte_length(lens[i].s) == lens[i].n);
    char big[1025];
    for (int i = 0; i < 1024; i++) big[i] = 'x';
    big[1024] = '\0';
    assert(osp_string_byte_length(big) == 1024);
    printf("  ok  cursor.byteLength\n");
}

void test_cursor_byte_at(void) {
    int64_t out;
    /* every position of an ASCII string returns its exact byte */
    const char *s = "Hello";
    for (int i = 0; i < 5; i++) {
        assert(osp_string_byte_at(s, i, &out) == NULL);
        assert(out == (int64_t)(unsigned char)s[i]);
    }
    static const cursor_probe probes[] = {
        /* the high byte 0xFF survives as 255, not a sign-extended negative */
        {"\xff", 0, 255},
        /* multi-byte: each raw byte of é (0xC3 0xA9) is addressable */
        {"héllo", 1, 0xC3}, {"héllo", 2, 0xA9}, {"héllo", 3, 'l'},
        /* error positions: -1, len, len+99, empty, NULL — all reported */
        {"abc", -1, WANT_REJECTED}, {"abc", 3, WANT_REJECTED}, {"abc", 99, WANT_REJECTED},
        {"", 0, WANT_REJECTED}, {NULL, 0, WANT_REJECTED},
    };
    assert_cursor_probes(osp_string_byte_at, probes, sizeof probes / sizeof probes[0]);
    printf("  ok  cursor.byteAt\n");
}

void test_cursor_codepoint_at(void) {
    /* "Aé世😀" is U+0041, U+00E9, U+4E16, U+1F600 at those byte offsets. */
    static const cursor_probe probes[] = {
        {"Aé世😀", 0, 0x41}, {"Aé世😀", 1, 0xE9},
        {"Aé世😀", 3, 0x4E16}, {"Aé世😀", 6, 0x1F600},
        /* every boundary codepoint of each width decodes exactly */
        {"\x7f", 0, 0x7F}, {"\xc2\x80", 0, 0x80},
        {"\xdf\xbf", 0, 0x7FF}, {"\xe0\xa0\x80", 0, 0x800},
        {"\xef\xbf\xbf", 0, 0xFFFF}, {"\xf0\x90\x80\x80", 0, 0x10000},
        {"\xf4\x8f\xbf\xbf", 0, 0x10FFFF},
        /* errors: mid-codepoint (0xA9 lead), out of range, truncated, bad bytes */
        {"héllo", 2, WANT_REJECTED}, {"abc", 3, WANT_REJECTED},
        {"abc", -1, WANT_REJECTED}, {"\xf0", 0, WANT_REJECTED},
        {"\xe4\xb8", 0, WANT_REJECTED}, {"\xc3\x20", 0, WANT_REJECTED},
        {"\xff", 0, WANT_REJECTED}, {"\x80", 0, WANT_REJECTED},
        /* structurally complete but non-canonical / non-scalar UTF-8:
         * overlong NUL, overlong, overlong, surrogate, > U+10FFFF, bad lead */
        {"\xc0\x80", 0, WANT_REJECTED}, {"\xe0\x80\x80", 0, WANT_REJECTED},
        {"\xf0\x80\x80\x80", 0, WANT_REJECTED}, {"\xed\xa0\x80", 0, WANT_REJECTED},
        {"\xf4\x90\x80\x80", 0, WANT_REJECTED}, {"\xf5\x80\x80\x80", 0, WANT_REJECTED},
    };
    assert_cursor_probes(osp_string_codepoint_at, probes, sizeof probes / sizeof probes[0]);
    printf("  ok  cursor.codePointAt\n");
}

void test_cursor_codepoint_width(void) {
    static const struct { int64_t cp; int64_t want; } widths[] = {
        {0x00, 1}, {0x7F, 1}, {0x80, 2}, {0x7FF, 2}, {0x800, 3},
        {0xFFFF, 3}, {0x10000, 4}, {0x10FFFF, 4},
        /* surrogate range D800..DFFF rejected at both ends + middle */
        {0xD800, WANT_REJECTED}, {0xDC00, WANT_REJECTED}, {0xDFFF, WANT_REJECTED},
        /* just outside the surrogate block is fine */
        {0xD7FF, 3}, {0xE000, 3},
        /* out of range / negative */
        {0x110000, WANT_REJECTED}, {-1, WANT_REJECTED},
    };
    for (size_t i = 0; i < sizeof widths / sizeof widths[0]; i++) {
        int64_t out = 0;
        const char *err = osp_string_codepoint_width(widths[i].cp, &out); /* sequenced before `out` is read */
        assert_value(err, out, widths[i].want);
    }
    printf("  ok  cursor.codePointWidth\n");
}

void test_cursor_from_codepoint(void) {
    /* exact byte encodings for each width: A, é, 世, 😀 */
    static const struct { int64_t cp; const char *bytes; } encodings[] = {
        {0x41, "A"}, {0xE9, "\xc3\xa9"},
        {0x4E16, "\xe4\xb8\x96"}, {0x1F600, "\xf0\x9f\x98\x80"},
    };
    for (size_t i = 0; i < sizeof encodings / sizeof encodings[0]; i++) {
        assert_encodes(encodings[i].cp, encodings[i].bytes);
    }
    /* invalid scalars rejected */
    assert(osp_string_from_codepoint(0) == NULL); /* C-string ABI cannot represent U+0000 */
    assert(osp_string_from_codepoint(0x110000) == NULL);
    assert(osp_string_from_codepoint(0xD800) == NULL);
    assert(osp_string_from_codepoint(0xDFFF) == NULL);
    assert(osp_string_from_codepoint(-1) == NULL);
    printf("  ok  cursor.fromCodePoint\n");
}

/* The four fallible cursor builtins return EXACT message strings — the text
 * threaded into the Result errmsg slot ([ERR-PAYLOAD]). Pin every one. */
/* Asserts a builtin returned exactly the error message `want`. */
static void assert_message(const char *got, const char *want) {
    assert(got != NULL && strcmp(got, want) == 0);
}

void test_cursor_error_messages(void) {
    int64_t out;
    assert_message(osp_string_byte_at("a", 5, &out), "byteAt: index out of range");
    assert_message(osp_string_byte_at(NULL, 0, &out), "byteAt: null string");
    assert_message(osp_string_codepoint_at("a", 9, &out),
                   "codePointAt: index out of range");
    assert_message(osp_string_codepoint_at("\xff", 0, &out),
                   "codePointAt: invalid UTF-8 lead byte");
    assert_message(osp_string_codepoint_at("\xf0", 0, &out),
                   "codePointAt: truncated codepoint");
    assert_message(osp_string_codepoint_at("\xc3\x20", 0, &out),
                   "codePointAt: invalid continuation byte");
    assert_message(osp_string_codepoint_at("\xc0\x80", 0, &out),
                   "codePointAt: invalid UTF-8 sequence");
    assert_message(osp_string_codepoint_width(0x110000, &out),
                   "codePointWidth: code point out of range");
    assert_message(osp_string_codepoint_width(0xD800, &out),
                   "codePointWidth: surrogate is not a scalar");
    printf("  ok  cursor.error_messages (exact text pinned)\n");
}

/* Exhaustive round-trip: encode then decode must recover the scalar, across
 * the whole code space (skipping surrogates and U+0000). Proves fromCodePoint
 * and codePointAt are exact inverses for every valid scalar. */
void test_cursor_roundtrip_exhaustive(void) {
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
        release_test_buffer(e);
        checked++;
    }
    /* cp ∈ [1, 0x10FFFF] is 0x10FFFF values; minus the 2048 surrogates. */
    assert(checked == 0x10FFFF - (0xDFFF - 0xD800 + 1));
    printf("  ok  cursor.roundtrip_exhaustive (%lld scalars)\n", (long long)checked);
}
