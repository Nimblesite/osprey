/*
 * Implements [BUILTIN-STRING-INSPECTION], [BUILTIN-STRING-SEARCH],
 * [BUILTIN-STRING-CURSOR], [BUILTIN-STRING-SUBSTRINGS], [BUILTIN-STRING-LIST],
 * [BUILTIN-STRING-TRANSFORM], and [BUILTIN-STRING-PARSING].
 * Shared declarations for string_runtime.c and string_runtime_list.c.
 */

#ifndef OSP_STRING_RUNTIME_H
#define OSP_STRING_RUNTIME_H

#include <stddef.h>
#include <stdint.h>

typedef struct {
    int64_t length;
    char **items;
} osp_string_list;

/* internal helpers shared between TUs */
char *osp_string_dup_internal(const char *s, size_t n);
char *osp_string_empty_internal(void);
int osp_is_ws_internal(unsigned char c);

/* scalar API — see string_runtime.c */
/* Byte length as a fixed-width int64. Wraps libc strlen so the size_t->int64
 * cast happens in C (correct on every target), keeping the emitted IR
 * pointer-width-stable — libc strlen returns size_t, 32-bit on wasm32 but
 * 64-bit on native. [BUILTIN-STRING-LENGTH] */
int64_t osp_strlen(const char *s);
int64_t osp_string_is_empty(const char *s);
int64_t osp_string_starts_with(const char *s, const char *prefix);
int64_t osp_string_ends_with(const char *s, const char *suffix);
int64_t osp_string_index_of(const char *s, const char *needle);
char *osp_string_take(const char *s, int64_t n);
char *osp_string_drop(const char *s, int64_t n);
char *osp_string_substring(const char *s, int64_t start, int64_t end);
char *osp_string_to_upper(const char *s);
char *osp_string_to_lower(const char *s);
char *osp_string_trim(const char *s);
char *osp_string_trim_start(const char *s);
char *osp_string_trim_end(const char *s);
char *osp_string_reverse(const char *s);
char *osp_string_replace(const char *s, const char *needle, const char *replacement);
char *osp_string_repeat(const char *s, int64_t n);
char *osp_string_pad_start(const char *s, int64_t target_length, const char *fill);
char *osp_string_pad_end(const char *s, int64_t target_length, const char *fill);
int64_t osp_parse_int_strict(const char *s, int64_t *out);
int64_t osp_parse_float_strict(const char *s, double *out);
char *osp_float_to_string(double d);

/* O(1) byte / codepoint cursor — BUILTIN-STRING-CURSOR. The fallible three
 * write their result through `*out` and return NULL on success, else a static
 * error message string. */
int64_t osp_string_byte_length(const char *s);
const char *osp_string_byte_at(const char *s, int64_t i, int64_t *out);
const char *osp_string_codepoint_at(const char *s, int64_t byte_index, int64_t *out);
const char *osp_string_codepoint_width(int64_t cp, int64_t *out);
char *osp_string_from_codepoint(int64_t cp);

/* String-interpolation formatting, two-pass: measure, then write into an
   exactly-sized buffer. Codegen calls THESE rather than snprintf directly
   because the emitted IR is target-neutral and `size_t` is not: it is 32-bit on
   wasm32 and 64-bit natively, so a literal size type in the IR mismatches
   wasi-libc's snprintf signature at wasm-ld time. `int64_t` is the same
   everywhere. [BUILTIN-STRING-INTERP] */
int64_t osp_format_size(const char *fmt, ...);
void osp_format_into(char *buf, int64_t cap, const char *fmt, ...);

/* list API — see string_runtime_list.c */
osp_string_list *osp_string_split(const char *s, const char *sep);
osp_string_list *osp_string_lines(const char *s);
osp_string_list *osp_string_words(const char *s);
char *osp_string_join(const osp_string_list *list, const char *sep);
void osp_string_list_free(osp_string_list *list);

#endif
