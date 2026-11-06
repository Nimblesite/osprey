/*
 * Implements [BUILTIN-STRING-INSPECTION], [BUILTIN-STRING-SEARCH],
 * [BUILTIN-STRING-CURSOR], [BUILTIN-STRING-SUBSTRINGS],
 * [BUILTIN-STRING-TRANSFORM], and [BUILTIN-STRING-PARSING].
 * Scalar (string -> string / bool / int) helpers exposed to Osprey IR.
 * List-returning helpers live in string_runtime_list.c.
 *
 * Conventions: owned text retains its byte length in GC/ARC metadata; borrowed
 * C text is terminated. Produced text is managed, and callers release its owner.
 */

#include <ctype.h>
#include <float.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <stdarg.h>
#include <stdio.h>

#include "float_spelling.h"
#include "string_runtime.h"
#include "memory_hooks.h"
#include "string_search.h"

// [PORT-STRING-NUL-OPERATIONS] Every produced text value owns its exact byte length.
char *osp_string_alloc_internal(size_t n) {
    if (n >= UINT32_MAX) return NULL;
    char *out = osp_alloc_tagged((int64_t)(n + 1), OSP_MEM_STRING);
    if (out) out[n] = '\0';
    return out;
}

char *osp_string_dup_internal(const char *s, size_t n) {
    char *out = osp_string_alloc_internal(n);
    if (out && n) memcpy(out, s, n);
    return out;
}

/* [IOS-HOST-IMPORTS] Copy borrowed host text into the selected managed backend. */
char *osp_host_string_copy(const char *s) {
    return s ? osp_string_dup_internal(s, (size_t)osp_strlen(s)) : NULL;
}

char *osp_string_empty_internal(void) { return osp_string_dup_internal("", 0); }

int osp_is_ws_internal(unsigned char c) {
    return c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\v' || c == '\f';
}

/* ---------- inspection / search (total) ---------- */

/* Byte length as a fixed-width int64 (see header). NULL is length 0. */
int64_t osp_strlen(const char *s) {
    if (!s) return 0;
    int64_t owned = osp_mem_string_length(s);
    return owned >= 0 ? owned : (int64_t)strlen(s);
}

// [PORT-STRING-NUL-CODEGEN] Compare and concatenate complete byte spans.
int32_t osp_string_compare(const char *left, const char *right) {
    if (left == right) return 0;
    if (!left || !right) return left ? 1 : -1;
    size_t n = (size_t)osp_strlen(left), m = (size_t)osp_strlen(right);
    int prefix = memcmp(left, right, n < m ? n : m);
    return prefix != 0 ? prefix : (n > m) - (n < m);
}

char *osp_string_concat(const char *left, const char *right) {
    if (!left || !right) return NULL;
    size_t n = (size_t)osp_strlen(left), m = (size_t)osp_strlen(right);
    char *out = n < UINT32_MAX && m <= UINT32_MAX - 1 - n
        ? osp_string_alloc_internal(n + m) : NULL;
    if (!out) return NULL;
    memcpy(out, left, n);
    memcpy(out + n, right, m);
    return out;
}

int64_t osp_string_is_empty(const char *s) {
    if (!s) return 1;
    return osp_strlen(s) == 0;
}

int64_t osp_string_starts_with(const char *s, const char *prefix) {
    if (!s || !prefix) return 0;
    size_t n = (size_t)osp_strlen(prefix);
    return (size_t)osp_strlen(s) >= n && memcmp(s, prefix, n) == 0;
}

int64_t osp_string_ends_with(const char *s, const char *suffix) {
    if (!s || !suffix) return 0;
    size_t n = (size_t)osp_strlen(s), tail = (size_t)osp_strlen(suffix);
    return n >= tail && memcmp(s + n - tail, suffix, tail) == 0;
}

/* Returns first byte-index of needle in s, or -1 if absent. Empty needle = 0. */
int64_t osp_string_index_of(const char *s, const char *needle) {
    if (!s || !needle) return -1;
    const char *hit = osp_string_find_internal(s, (size_t)osp_strlen(s), needle, (size_t)osp_strlen(needle));
    return hit ? (int64_t)(hit - s) : -1;
}

/* ---------- substrings (total) ---------- */

/* A cut never splits a UTF-8 character: it moves forward past the character's
 * continuation bytes, so take(s, n) + drop(s, n) == s and drop(s, 1) advances. */
static size_t osp_string_cut(const char *s, size_t cut) {
    while (((unsigned char)s[cut] & 0xC0) == 0x80) cut++;
    return cut;
}

char *osp_string_take(const char *s, int64_t n) {
    if (!s) return osp_string_empty_internal();
    size_t len = (size_t)osp_strlen(s);
    if (n <= 0) return osp_string_empty_internal();
    if ((size_t)n >= len) return osp_string_dup_internal(s, len);
    return osp_string_dup_internal(s, osp_string_cut(s, (size_t)n));
}

char *osp_string_drop(const char *s, int64_t n) {
    if (!s) return osp_string_empty_internal();
    size_t len = (size_t)osp_strlen(s);
    if (n <= 0) return osp_string_dup_internal(s, len);
    if ((size_t)n >= len) return osp_string_empty_internal();
    size_t cut = osp_string_cut(s, (size_t)n);
    return osp_string_dup_internal(s + cut, len - cut);
}

/* substring: returns NULL on out-of-range or inverted indices.
 * Caller emits IndexOutOfRange when NULL. */
char *osp_string_substring(const char *s, int64_t start, int64_t end) {
    if (!s) return NULL;
    size_t len = (size_t)osp_strlen(s);
    if (start < 0 || end < start || (size_t)end > len) return NULL;
    return osp_string_dup_internal(s + start, (size_t)(end - start));
}

/* ---------- transformation (total) ---------- */

/* ASCII case conversion [BUILTIN-STRING-TOUPPERCASE]. */
static char *string_case(const char *s, int (*convert)(int)) {
    if (!s) return osp_string_empty_internal();
    size_t n = (size_t)osp_strlen(s);
    char *out = osp_string_dup_internal(s, n);
    if (out) for (size_t i = 0; i < n; i++) out[i] = (char)convert((unsigned char)out[i]);
    return out;
}

char *osp_string_to_upper(const char *s) { return string_case(s, toupper); }
char *osp_string_to_lower(const char *s) { return string_case(s, tolower); }

/* One-sided ASCII trimming [BUILTIN-STRING-TRIMSTART]. */
char *osp_string_trim_start(const char *s) {
    if (!s) return osp_string_empty_internal();
    size_t n = (size_t)osp_strlen(s), start = 0;
    while (start < n && osp_is_ws_internal((unsigned char)s[start])) start++;
    return osp_string_dup_internal(s + start, n - start);
}

/* One-sided ASCII trimming [BUILTIN-STRING-TRIMEND]. */
char *osp_string_trim_end(const char *s) {
    if (!s) return osp_string_empty_internal();
    size_t len = (size_t)osp_strlen(s);
    while (len > 0 && osp_is_ws_internal((unsigned char)s[len - 1])) len--;
    return osp_string_dup_internal(s, len);
}

char *osp_string_trim(const char *s) {
    if (!s) return osp_string_empty_internal();
    size_t end = (size_t)osp_strlen(s), start = 0;
    while (start < end && osp_is_ws_internal((unsigned char)s[start])) start++;
    while (end > start && osp_is_ws_internal((unsigned char)s[end - 1])) end--;
    return osp_string_dup_internal(s + start, end - start);
}

char *osp_string_reverse(const char *s) {
    if (!s) return osp_string_empty_internal();
    size_t len = (size_t)osp_strlen(s);
    char *out = osp_string_alloc_internal(len);
    if (!out) return NULL;
    for (size_t i = 0; i < len; i++) out[i] = s[len - 1 - i];
    out[len] = '\0';
    return out;
}

/* ---------- transformation (fallible) ----------
 * NULL return = caller should emit InvalidArgument. */

char *osp_string_replace(const char *s, const char *needle, const char *replacement) {
    if (!s || !needle || !replacement) return NULL;
    size_t slen = (size_t)osp_strlen(s), nlen = (size_t)osp_strlen(needle), rlen = (size_t)osp_strlen(replacement);
    if (nlen == 0) return NULL;
    size_t count = osp_string_count_internal(s, slen, needle, nlen);
    size_t kept = slen - count * nlen;
    if (kept >= UINT32_MAX || (rlen && count > (UINT32_MAX - 1 - kept) / rlen)) return NULL;
    size_t out_len = kept + count * rlen;
    char *out = osp_string_alloc_internal(out_len), *w = out;
    if (!out) return NULL;
    size_t offset = 0;
    const char *hit;
    while ((hit = osp_string_find_internal(s + offset, slen - offset, needle, nlen)) != NULL) {
        size_t before = (size_t)(hit - s) - offset;
        memcpy(w, s + offset, before); w += before;
        memcpy(w, replacement, rlen); w += rlen; offset = (size_t)(hit - s) + nlen;
    }
    memcpy(w, s + offset, slen - offset);
    return out;
}

char *osp_string_repeat(const char *s, int64_t n) {
    if (!s || n < 0) return NULL;
    if (n == 0) return osp_string_empty_internal();
    size_t len = (size_t)osp_strlen(s);
    if (len == 0) return osp_string_empty_internal();
    if ((uint64_t)n > (UINT32_MAX - 1) / len) return NULL;
    size_t out_len = len * (size_t)n;
    char *out = osp_string_alloc_internal(out_len);
    if (!out) return NULL;
    for (int64_t i = 0; i < n; i++) memcpy(out + (size_t)i * len, s, len);
    out[out_len] = '\0';
    return out;
}

static char *string_pad(const char *s, int64_t target, const char *fill, int at_start) {
    if (!s || !fill || osp_strlen(fill) == 0) return NULL;
    if (target > 0 && (uint64_t)target >= UINT32_MAX) return NULL;
    size_t n = (size_t)osp_strlen(s), width = (size_t)osp_strlen(fill);
    if (target <= 0 || (size_t)target <= n) return osp_string_dup_internal(s, n);
    size_t padding = (size_t)target - n;
    char *out = osp_string_alloc_internal((size_t)target);
    if (!out) return NULL;
    memcpy(out + (at_start ? padding : 0), s, n);
    for (size_t i = 0; i < padding; i++) out[(at_start ? 0 : n) + i] = fill[i % width];
    return out;
}

char *osp_string_pad_start(const char *s, int64_t target, const char *fill) {
    return string_pad(s, target, fill, 1);
}

char *osp_string_pad_end(const char *s, int64_t target, const char *fill) {
    return string_pad(s, target, fill, 0);
}
/* ---------- parsing ---------- */

#include "string_parse_int.h"

int64_t osp_parse_int_strict(const char *s, int64_t *out) {
    int64_t length = osp_mem_string_length(s);
    if (length > 0 && memchr(s, '\0', (size_t)length)) return 1;
    return osp_parse_int_cstr(s, out);
}

/* [BUILTIN-STRING-PARSEFLOAT] decimal grammar:
 *   sign? (digits ('.' digits?)? | '.' digits) exponent?
 * where exponent is [eE] sign? digits. Validate before strtod so its
 * whitespace, infinity/NaN, and hexadecimal extensions are not exposed. */
static int osp_is_decimal_float(const char *s) {
    const unsigned char *p = (const unsigned char *)s;
    if (*p == '+' || *p == '-') p++;

    int whole_digits = 0;
    while (*p >= '0' && *p <= '9') {
        whole_digits++;
        p++;
    }

    int fractional_digits = 0;
    if (*p == '.') {
        p++;
        while (*p >= '0' && *p <= '9') {
            fractional_digits++;
            p++;
        }
    }
    if (whole_digits == 0 && fractional_digits == 0) return 0;

    if (*p == 'e' || *p == 'E') {
        p++;
        if (*p == '+' || *p == '-') p++;
        int exponent_digits = 0;
        while (*p >= '0' && *p <= '9') {
            exponent_digits++;
            p++;
        }
        if (exponent_digits == 0) return 0;
    }
    return *p == '\0';
}

int64_t osp_parse_float_strict(const char *s, double *out) {
    if (!s || !out || memchr(s, '\0', (size_t)osp_strlen(s)) || !osp_is_decimal_float(s)) return 1;
    char *endp = NULL;
    double v = strtod(s, &endp);
    if (!endp || *endp != '\0' || endp == s) return 1;
    if (v != v || v > DBL_MAX || v < -DBL_MAX) return 1;
    *out = v;
    return 0;
}

/* Formats a spelled float so whole-valued floats keep a visible ".0" —
 * without this, 3.0 yields "3", indistinguishable from int 3 in user output
 * [TYPE-NUMERIC-PRINT]. The copy is malloc'd and owned by the caller. NaN /
 * +inf / -inf pass through unchanged. */
int osp_float_marker_internal(char *buf, int n, size_t cap) {
    if (n < 0 || (size_t)n >= cap) return -1;
    int has_marker = 0;
    for (const char *p = buf; *p; p++) {
        if (*p == '.' || *p == 'e' || *p == 'E' || *p == 'n' || *p == 'i') {
            has_marker = 1;
            break;
        }
    }
    if (!has_marker && (size_t)n + 2 >= cap) return -1;
    if (!has_marker) {
        memcpy(buf + n, ".0", 3);
        n += 2;
    }
    return n;
}

static char *float_string(char *buf, int n, size_t cap) {
    n = osp_float_marker_internal(buf, n, cap);
    return n < 0 ? NULL : osp_string_dup_internal(buf, (size_t)n);
}

char *osp_float_to_string(double d) {
    char buf[64];
    return float_string(buf, osp_float_spelling(d, buf, sizeof(buf)), sizeof(buf));
}

char *osp_float32_to_string(float value) {
    char buf[64];
    return float_string(buf, osp_float32_spelling(value, buf, sizeof(buf)), sizeof(buf));
}

char *osp_float16_to_string(uint16_t bits) {
    char buf[64];
    return float_string(buf, osp_float16_spelling(bits, buf, sizeof(buf)), sizeof(buf));
}

/* ---------- O(1) byte / codepoint cursor (BUILTIN-STRING-CURSOR) ----------
 * The irreducible primitive layer a pure-Osprey parser walks input with; every
 * higher op (take/drop/substring) allocates, these do not. The fallible three
 * report through `int64_t *out` + a returned `const char *` message (NULL on
 * success); the message is a static .rodata string (process lifetime). The
 * codegen side (strings.rs `cursor_int`) lowers that into the Result errmsg
 * slot — see docs/specs/runtime/0012-Built-InFunctions.md (Cursor Access). */

int64_t osp_string_byte_length(const char *s) {
    if (!s) return 0;
    return osp_strlen(s);
}

const char *osp_string_byte_at(const char *s, int64_t i, int64_t *out) {
    if (!s || !out) return "byteAt: null string";
    int64_t len = osp_strlen(s);
    if (i < 0 || i >= len) return "byteAt: index out of range";
    *out = (int64_t)(unsigned char)s[i];
    return NULL;
}

/* UTF-8 bytes a lead byte begins, or 0 when it is not a valid lead. */
static int osp_utf8_lead_width(unsigned char b) {
    if (b < 0x80) return 1;
    if ((b & 0xE0) == 0xC0) return 2;
    if ((b & 0xF0) == 0xE0) return 3;
    if ((b & 0xF8) == 0xF0) return 4;
    return 0;
}

const char *osp_string_codepoint_at(const char *s, int64_t byte_index, int64_t *out) {
    if (!s || !out) return "codePointAt: null string";
    int64_t len = osp_strlen(s);
    if (byte_index < 0 || byte_index >= len) return "codePointAt: index out of range";
    const unsigned char *p = (const unsigned char *)s + byte_index;
    int width = osp_utf8_lead_width(p[0]);
    if (width == 0) return "codePointAt: invalid UTF-8 lead byte";
    if (byte_index + width > len) return "codePointAt: truncated codepoint";
    int64_t cp = (int64_t)(p[0] & (0x7F >> width));
    for (int k = 1; k < width; k++) {
        if ((p[k] & 0xC0) != 0x80) return "codePointAt: invalid continuation byte";
        cp = (cp << 6) | (int64_t)(p[k] & 0x3F);
    }
    if (width == 1) cp = p[0];
    const int64_t minimum = width == 1 ? 0 : width == 2 ? 0x80 : width == 3 ? 0x800 : 0x10000;
    if (cp < minimum || cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF)) {
        return "codePointAt: invalid UTF-8 sequence";
    }
    *out = cp;
    return NULL;
}

const char *osp_string_codepoint_width(int64_t cp, int64_t *out) {
    if (!out) return "codePointWidth: null out";
    if (cp < 0 || cp > 0x10FFFF) return "codePointWidth: code point out of range";
    if (cp >= 0xD800 && cp <= 0xDFFF) return "codePointWidth: surrogate is not a scalar";
    *out = cp <= 0x7F ? 1 : cp <= 0x7FF ? 2 : cp <= 0xFFFF ? 3 : 4;
    return NULL;
}

char *osp_string_from_codepoint(int64_t cp) {
    /* [BUILTIN-STRING-FROMCODEPOINT] U+0000 cannot inhabit this runtime's
     * NUL-terminated string ABI: returning "\0" would be the empty string. */
    if (cp <= 0 || cp > 0x10FFFF) return NULL;
    if (cp >= 0xD800 && cp <= 0xDFFF) return NULL;
    char buf[5];
    int n;
    if (cp <= 0x7F) {
        buf[0] = (char)cp;
        n = 1;
    } else if (cp <= 0x7FF) {
        buf[0] = (char)(0xC0 | (cp >> 6));
        buf[1] = (char)(0x80 | (cp & 0x3F));
        n = 2;
    } else if (cp <= 0xFFFF) {
        buf[0] = (char)(0xE0 | (cp >> 12));
        buf[1] = (char)(0x80 | ((cp >> 6) & 0x3F));
        buf[2] = (char)(0x80 | (cp & 0x3F));
        n = 3;
    } else {
        buf[0] = (char)(0xF0 | (cp >> 18));
        buf[1] = (char)(0x80 | ((cp >> 12) & 0x3F));
        buf[2] = (char)(0x80 | ((cp >> 6) & 0x3F));
        buf[3] = (char)(0x80 | (cp & 0x3F));
        n = 4;
    }
    buf[n] = '\0';
    return osp_string_dup_internal(buf, (size_t)n);
}

/* Interpolation formatting (see string_runtime.h for why these exist). The
   format string is codegen-built and contains only `%s` holes plus escaped
   literal text, so the varargs are always `const char *`. */
int64_t osp_format_size(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(NULL, 0, fmt, ap);
    va_end(ap);
    return n < 0 ? 0 : (int64_t)n;
}

void osp_format_into(char *buf, int64_t cap, const char *fmt, ...) {
    if (buf == NULL || cap <= 0) return;
    va_list ap;
    va_start(ap, fmt);
    (void)vsnprintf(buf, (size_t)cap, fmt, ap);
    va_end(ap);
}
