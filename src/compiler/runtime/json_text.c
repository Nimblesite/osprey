// JSON string text in both directions. The parser decodes string literals
// here and the writer escapes them here, so what one rejects the other can
// never produce: the same UTF-8 validator and the same escape alphabet serve
// both. Implements [BUILTIN-JSON-STRING].
#include "json_internal.h"

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

// Encodes a Unicode code point as UTF-8 into out (up to 4 bytes); returns count.
static size_t utf8_encode(unsigned long cp, char *out) {
  if (cp <= 0x7F) {
    out[0] = (char)cp;
    return 1;
  }
  if (cp <= 0x7FF) {
    out[0] = (char)(0xC0 | (cp >> 6));
    out[1] = (char)(0x80 | (cp & 0x3F));
    return 2;
  }
  if (cp <= 0xFFFF) {
    out[0] = (char)(0xE0 | (cp >> 12));
    out[1] = (char)(0x80 | ((cp >> 6) & 0x3F));
    out[2] = (char)(0x80 | (cp & 0x3F));
    return 3;
  }
  out[0] = (char)(0xF0 | (cp >> 18));
  out[1] = (char)(0x80 | ((cp >> 12) & 0x3F));
  out[2] = (char)(0x80 | ((cp >> 6) & 0x3F));
  out[3] = (char)(0x80 | (cp & 0x3F));
  return 4;
}

// Out of band for a 4-bit value, so a non-hex character is reported rather
// than folded into the digit set. Returning 0 for one silently decoded
// "\uZZZZ" as U+0000 — malformed JSON accepted as an embedded NUL, which then
// truncates the very string the caller reads back [BUILTIN-JSON].
#define HEX_NIBBLE_INVALID 16u

static unsigned hex_nibble(char ch) {
  if (ch >= '0' && ch <= '9') {
    return (unsigned)(ch - '0');
  }
  if (ch >= 'a' && ch <= 'f') {
    return (unsigned)(ch - 'a' + 10);
  }
  if (ch >= 'A' && ch <= 'F') {
    return (unsigned)(ch - 'A' + 10);
  }
  return HEX_NIBBLE_INVALID;
}

// Consume the four hex digits of a \u escape, advancing the cursor over them.
// False — leaving the cursor where it stopped — when any is missing or is not
// a hex digit. The terminating NUL is not a hex digit either, so a truncated
// escape stops here instead of walking off the end of the text.
static bool parse_hex4(Cursor *c, unsigned long *out) {
  unsigned long cp = 0;
  for (int i = 0; i < 4; i++) {
    c->p++;
    unsigned nibble = hex_nibble(*c->p);
    if (nibble == HEX_NIBBLE_INVALID) {
      return false;
    }
    cp = (cp << 4) | nibble;
  }
  *out = cp;
  return true;
}

// The complete escape alphabet of RFC 8259. Anything outside it -- `\q`,
// `\x41`, a backslash before the closing quote -- is malformed input. Dropping
// the backslash and keeping the letter turned every one of them into a
// plausible-looking string no other JSON reader agrees with
// [BUILTIN-JSON-STRING].
static bool simple_escape(char ch, char *out) {
  switch (ch) {
  case '"': *out = '"'; return true;
  case '\\': *out = '\\'; return true;
  case '/': *out = '/'; return true;
  case 'b': *out = '\b'; return true;
  case 'f': *out = '\f'; return true;
  case 'n': *out = '\n'; return true;
  case 'r': *out = '\r'; return true;
  case 't': *out = '\t'; return true;
  default: return false;
  }
}

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

static bool is_high_surrogate(unsigned long cp) {
  return cp >= SURROGATE_HIGH_FIRST && cp <= SURROGATE_HIGH_LAST;
}

static bool is_low_surrogate(unsigned long cp) {
  return cp >= SURROGATE_LOW_FIRST && cp <= SURROGATE_LOW_LAST;
}

// Decode one `\uXXXX` escape with the cursor on its `u`, leaving the cursor on
// the last consumed character. False for every malformed spelling.
static bool decode_unicode_escape(Cursor *c, char *buf, size_t *n) {
  unsigned long cp = 0;
  if (!parse_hex4(c, &cp)) {
    return false;
  }
  if (cp == 0) {
    // Every scalar leaves this unit as a NUL-terminated C string, so a decoded
    // U+0000 does not embed a NUL -- it TRUNCATES the value, and the document
    // {"k":"a\\u0000b"} reads back as "a" with no error reported anywhere
    // [BUILTIN-JSON-STRING].
    return false;
  }
  if (is_low_surrogate(cp)) {
    return false; // a low half with no high half in front of it
  }
  if (is_high_surrogate(cp)) {
    if (c->p[1] != '\\' || c->p[2] != 'u') {
      return false; // a high half with no escape at all after it
    }
    c->p += 2;
    unsigned long lo = 0;
    if (!parse_hex4(c, &lo) || !is_low_surrogate(lo)) {
      return false; // ... or an escape that is not the other half
    }
    cp = SURROGATE_PAIR_BASE +
         ((cp - SURROGATE_HIGH_FIRST) << SURROGATE_HIGH_SHIFT) +
         (lo - SURROGATE_LOW_FIRST);
  }
  *n = utf8_encode(cp, buf);
  return true;
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
static size_t utf8_sequence_length(unsigned char lead) {
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
static const unsigned long UTF8_MIN_FOR_LENGTH[] = {0, 0, 0x80UL, 0x800UL,
                                                    0x10000UL};

size_t json_utf8_scalar_length(const char *p) {
  unsigned char lead = (unsigned char)*p;
  if (lead < UTF8_CONTINUATION_MARK) {
    return lead == '\0' ? 0 : 1;
  }
  size_t len = utf8_sequence_length(lead);
  if (len == 0) {
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

static bool string_rejected(Cursor *c, const char *why) {
  json_reject(c, why);
  return false;
}

// Printable ASCII other than the quote and the backslash stands for itself.
static bool plain_ascii(unsigned char byte) {
  return byte > JSON_LAST_CONTROL_CHAR && byte < UTF8_CONTINUATION_MARK &&
         byte != '"' && byte != '\\';
}

// Consume the run of bytes at the cursor that need no decoding -- plain ASCII
// and well-formed UTF-8 -- and append it whole, so a string without escapes
// costs one append. Stops on the quote, a backslash, a control character,
// malformed UTF-8 or the end of the text.
static bool copy_run(Cursor *c, JsonBuf *out) {
  const char *start = c->p;
  for (;;) {
    unsigned char byte = (unsigned char)*c->p;
    size_t width = plain_ascii(byte) ? 1
                   : byte >= UTF8_CONTINUATION_MARK
                       ? json_utf8_scalar_length(c->p)
                       : 0;
    if (width == 0) {
      break;
    }
    c->p += width;
  }
  return json_buf_append(out, start, (size_t)(c->p - start));
}

// Decode the escape at the cursor onto `out`. False for anything else that
// stopped a run: a raw control character, malformed UTF-8, or an escape
// outside the RFC 8259 alphabet.
static bool decode_escape(Cursor *c, JsonBuf *out) {
  if (*c->p != '\\') {
    return false;
  }
  c->p++;
  char buf[4];
  size_t n = 1;
  bool ok = *c->p == 'u' ? decode_unicode_escape(c, buf, &n)
                         : simple_escape(*c->p, buf);
  c->p++;
  return ok && json_buf_append(out, buf, n);
}

bool json_decode_string(Cursor *c, JsonBuf *out) {
  if (*c->p != '"') {
    return string_rejected(c, "expected a string");
  }
  c->p++;
  for (;;) {
    if (!copy_run(c, out)) {
      return string_rejected(c, "out of memory");
    }
    if (*c->p == '"') {
      c->p++;
      return json_buf_append(out, "", 1) ||
             string_rejected(c, "out of memory");
    }
    if (*c->p == '\0') {
      return string_rejected(c, "unterminated string");
    }
    if (!decode_escape(c, out)) {
      return string_rejected(c, "invalid character or escape in string");
    }
  }
}

// ---- writing ----------------------------------------------------------------

bool json_buf_append(JsonBuf *out, const char *bytes, size_t n) {
  if (out->len + n + 1 > out->cap) {
    size_t cap = (out->len + n + 1) * 2;
    char *grown = realloc(out->data, cap);
    if (!grown) {
      return false;
    }
    out->data = grown;
    out->cap = cap;
  }
  memcpy(out->data + out->len, bytes, n);
  out->len += n;
  out->data[out->len] = '\0';
  return true;
}

// The shortest escape RFC 8259 allows for each byte that cannot appear raw:
// the quote, the backslash and U+0001..U+001F (U+0000 cannot occur in a
// NUL-terminated string). `/` and DEL need none.
static size_t escape_ascii(unsigned char byte, char *out) {
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
// costs one append for its text plus one per escaped byte.
bool json_quote_into(JsonBuf *out, const char *s) {
  if (!json_buf_append(out, "\"", 1)) {
    return false;
  }
  const char *run = s;
  while (*s) {
    char escaped[6];
    size_t width = json_utf8_scalar_length(s);
    size_t n = width == 1 ? escape_ascii((unsigned char)*s, escaped) : 0;
    if (width == 0 || (n > 0 && (!json_buf_append(out, run, (size_t)(s - run)) ||
                                 !json_buf_append(out, escaped, n)))) {
      return false;
    }
    s += width;
    run = n > 0 ? s : run;
  }
  return json_buf_append(out, run, (size_t)(s - run)) &&
         json_buf_append(out, "\"", 1);
}
