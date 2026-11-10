// The string cursor [BUILTIN-STRING-CURSOR]: byte length and access, code
// points and their widths, and exact error messages. Part of the string_runtime suite.

#include "string_tests_shared.h"

void test_cursor_byte_length(void) {
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

void test_cursor_byte_at(void) {
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

void test_cursor_codepoint_at(void) {
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
    /* structurally complete but non-canonical / non-scalar UTF-8 */
    assert(osp_string_codepoint_at("\xc0\x80", 0, &out) != NULL); /* overlong NUL */
    assert(osp_string_codepoint_at("\xe0\x80\x80", 0, &out) != NULL); /* overlong */
    assert(osp_string_codepoint_at("\xf0\x80\x80\x80", 0, &out) != NULL); /* overlong */
    assert(osp_string_codepoint_at("\xed\xa0\x80", 0, &out) != NULL); /* surrogate */
    assert(osp_string_codepoint_at("\xf4\x90\x80\x80", 0, &out) != NULL); /* > U+10FFFF */
    assert(osp_string_codepoint_at("\xf5\x80\x80\x80", 0, &out) != NULL); /* bad lead */
    printf("  ok  cursor.codePointAt\n");
}

void test_cursor_codepoint_width(void) {
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

void test_cursor_from_codepoint(void) {
    /* exact byte encodings for each width */
    char *e;
    e = osp_string_from_codepoint(0x41);                 /* 'A' */
    assert(e && strcmp(e, "A") == 0 && osp_string_byte_length(e) == 1); release_test_buffer(e);
    e = osp_string_from_codepoint(0xE9);                 /* é */
    assert(e && (unsigned char)e[0] == 0xC3 && (unsigned char)e[1] == 0xA9 &&
           osp_string_byte_length(e) == 2); release_test_buffer(e);
    e = osp_string_from_codepoint(0x4E16);               /* 世 */
    assert(e && (unsigned char)e[0] == 0xE4 && (unsigned char)e[1] == 0xB8 &&
           (unsigned char)e[2] == 0x96 && osp_string_byte_length(e) == 3); release_test_buffer(e);
    e = osp_string_from_codepoint(0x1F600);              /* 😀 */
    assert(e && (unsigned char)e[0] == 0xF0 && (unsigned char)e[1] == 0x9F &&
           (unsigned char)e[2] == 0x98 && (unsigned char)e[3] == 0x80 &&
           osp_string_byte_length(e) == 4); release_test_buffer(e);
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
void test_cursor_error_messages(void) {
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
    assert(strcmp(osp_string_codepoint_at("\xc0\x80", 0, &out),
                  "codePointAt: invalid UTF-8 sequence") == 0);
    assert(strcmp(osp_string_codepoint_width(0x110000, &out),
                  "codePointWidth: code point out of range") == 0);
    assert(strcmp(osp_string_codepoint_width(0xD800, &out),
                  "codePointWidth: surrogate is not a scalar") == 0);
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
