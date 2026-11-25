// JSON string escaping with no allocation of its own: the UTF-8 validator and
// the escape alphabet the parser (json_text.c) and every writer share, so what
// one rejects no writer can produce. A writer quotes into its own buffer through
// `json_quote_to`, which keeps each allocation with the allocator that made it
// (the JSON unit is built against the ARC shim, the socket servers are not).
// Implements [BUILTIN-JSON-STRING].
#ifndef OSPREY_JSON_ESCAPE_H
#define OSPREY_JSON_ESCAPE_H

#include <stdbool.h>
#include <stddef.h>
#include <string.h>

// Surrogate halves are not code points: they exist only as the two-unit
// spelling of one code point above U+FFFF. Encoding a half on its own emits
// CESU-8, which no UTF-8 reader accepts, and combining a high half with
// something that is not a low half underflows `lo - 0xDC00` through zero and
// lands the result somewhere in the millions [BUILTIN-JSON-STRING].
#define SURROGATE_HIGH_FIRST 0xD800UL
#define SURROGATE_HIGH_LAST 0xDBFFUL
#define SURROGATE_LOW_FIRST 0xDC00UL
#define SURROGATE_LOW_LAST 0xDFFFUL
#define SURROGATE_PAIR_BASE 0x10000UL
#define SURROGATE_HIGH_SHIFT 10

static inline bool is_high_surrogate(unsigned long cp) {
  return cp >= SURROGATE_HIGH_FIRST && cp <= SURROGATE_HIGH_LAST;
}

static inline bool is_low_surrogate(unsigned long cp) {
  return cp >= SURROGATE_LOW_FIRST && cp <= SURROGATE_LOW_LAST;
}

// The LITERAL path. JSON text is UTF-8 by definition and every scalar leaves
// this unit as a string the rest of the runtime reads as UTF-8, so copying
// whatever bytes arrived would carry an overlong form, a surrogate spelled in
// UTF-8, a truncated sequence or a stray continuation byte straight into a
// value that no downstream reader agrees about -- while the escape path next
// to it rejects the very same characters [BUILTIN-JSON-STRING].
#define UTF8_CONTINUATION_MASK 0xC0u
#define UTF8_CONTINUATION_MARK 0x80u
#define UNICODE_MAX 0x10FFFFUL

// How many bytes the sequence a lead byte introduces occupies; 0 when the byte
// is not a lead byte at all (a continuation byte, or 0xF8..0xFF).
static inline size_t utf8_sequence_length(unsigned char lead) {
  if ((lead & 0xE0u) == 0xC0u) {
    return 2;
  }
  if ((lead & 0xF0u) == 0xE0u) {
    return 3;
  }
  if ((lead & 0xF8u) == 0xF0u) {
    return 4;
  }
  return 0;
}

// The smallest code point each length may legally spell. Anything below is an
// overlong form: a second spelling of a character that already has one, and
// the classic way past a filter that only looked at the short one.
static inline size_t utf8_scalar_length(const char *p, size_t remaining) {
  static const unsigned long UTF8_MIN_FOR_LENGTH[] = {0, 0, 0x80UL, 0x800UL, 0x10000UL};
  unsigned char lead = (unsigned char)*p;
  if (lead < UTF8_CONTINUATION_MARK) {
    return 1;
  }
  size_t len = utf8_sequence_length(lead);
  if (len == 0 || len > remaining) {
    return 0;
  }
  unsigned long cp = lead & (0xFFu >> (len + 1));
  for (size_t i = 1; i < len; i++) {
    unsigned char next = (unsigned char)p[i];
    if ((next & UTF8_CONTINUATION_MASK) != UTF8_CONTINUATION_MARK) {
      return 0; // truncated -- the terminating NUL stops here too
    }
    cp = (cp << 6) | (next & 0x3Fu);
  }
  bool spelled_once = cp >= UTF8_MIN_FOR_LENGTH[len] && cp <= UNICODE_MAX;
  return spelled_once && !is_high_surrogate(cp) && !is_low_surrogate(cp) ? len
                                                                          : 0;
}

// RFC 8259 requires U+0000..U+001F to be escaped inside a string. Taking a raw
// one literally makes this parser disagree with every other reader about where
// the string ends [BUILTIN-JSON-STRING].
#define JSON_LAST_CONTROL_CHAR 0x1F

// The shortest escape RFC 8259 allows for each byte that cannot appear raw:
// the quote, the backslash and U+0000..U+001F. `/` and DEL need none.
static inline size_t json_escape_ascii(unsigned char byte, char *out) {
  static const char HEX[] = "0123456789abcdef";
  const char *named = byte == '"'    ? "\\\""
                      : byte == '\\' ? "\\\\"
                      : byte == '\b'  ? "\\b"
                      : byte == '\f'  ? "\\f"
                      : byte == '\n'  ? "\\n"
                      : byte == '\r'  ? "\\r"
                      : byte == '\t'  ? "\\t"
                                       : NULL;
  if (named) {
    memcpy(out, named, 2);
    return 2;
  }
  if (byte > JSON_LAST_CONTROL_CHAR) {
    return 0;
  }
  memcpy(out, "\\u00", 4);
  out[4] = HEX[byte >> 4];
  out[5] = HEX[byte & 0xFu];
  return 6;
}

// Runs of bytes that need no escape are appended whole, so a typical string
// costs one append for its text plus one per escaped byte. `append` adds bytes
// to the caller's own buffer, so the caller's allocator owns the result; false
// when `s` is not UTF-8 or `append` fails.
typedef bool (*JsonAppend)(void *sink, const char *bytes, size_t n);
static inline bool json_quote_to(JsonAppend append, void *sink, const char *s, size_t length) {
  if (!append(sink, "\"", 1)) {
    return false;
  }
  const char *run = s;
  const char *end = s + length;
  while (s < end) {
    char escaped[6];
    size_t width = utf8_scalar_length(s, (size_t)(end - s));
    size_t n = width == 1 ? json_escape_ascii((unsigned char)*s, escaped) : 0;
    if (width == 0 || (n > 0 && (!append(sink, run, (size_t)(s - run)) || !append(sink, escaped, n)))) {
      return false;
    }
    s += width;
    run = n > 0 ? s : run;
  }
  return append(sink, run, (size_t)(s - run)) && append(sink, "\"", 1);
}

#endif
