// JSON scalar text. The parser decodes string literals here and the writer
// escapes them here, so what one rejects the other can never produce: the same
// UTF-8 validator and the same escape alphabet serve both. Numbers are scanned
// and read here too. Implements [BUILTIN-JSON-STRING] and [BUILTIN-JSON-NUMBER].
#include "json_escape.h"
#include "json_internal.h"
#include "utf8_encode.h"

#include <errno.h>
#include <math.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

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


// Decode one `\uXXXX` escape with the cursor on its `u`, leaving the cursor on
// the last consumed character. False for every malformed spelling.
static bool decode_unicode_escape(Cursor *c, char *buf, size_t *n) {
  unsigned long cp = 0;
  if (!parse_hex4(c, &cp)) {
    return false;
  }
  if (cp == 0 && !c->python) {
    return false; // Strict JSON keeps its existing NUL policy.
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
                       ? utf8_scalar_length(c->p, SIZE_MAX)
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
      bool embedded = c->end && c->p < c->end;
      return string_rejected(c, embedded ? "unescaped NUL in string" : "unterminated string");
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

static bool json_buf_sink(void *sink, const char *bytes, size_t n) {
  return json_buf_append((JsonBuf *)sink, bytes, n);
}

bool json_quote_into(JsonBuf *out, const char *s, size_t length) {
  return json_quote_to(json_buf_sink, out, s, length);
}



// One or more digits; false when there were none.
static bool scan_digits(Cursor *c) {
  const char *start = c->p;
  while (*c->p >= '0' && *c->p <= '9') {
    c->p++;
  }
  return c->p != start;
}
// RFC 8259's number grammar exactly: an optional minus, an integer part whose
// leading zero stands alone, an optional fraction of at least one digit, and
// an optional exponent of at least one digit. `*integer` says whether the
// fraction and exponent were both absent.
//
// The scanner this replaces took any run of [0-9.eE+-], so `-`, `1.`, `1e`,
// `1+2`, `--1` and `1e+` all parsed -- and since a number is STORED as its
// source text, every one of them came straight back out of jsonGet as a
// "number" no other reader would ever produce [BUILTIN-JSON-NUMBER].
bool json_scan_number(Cursor *c, bool *integer) {
  if (*c->p == '-') {
    c->p++;
  }
  if (*c->p == '0') {
    c->p++; // a leading zero is the entire integer part
  } else if (!scan_digits(c)) {
    return false;
  }
  *integer = *c->p != '.' && *c->p != 'e' && *c->p != 'E';
  if (*c->p == '.') {
    c->p++;
    if (!scan_digits(c)) {
      return false;
    }
  }
  if (*c->p != 'e' && *c->p != 'E') {
    return true;
  }
  c->p++;
  if (*c->p == '+' || *c->p == '-') {
    c->p++;
  }
  return scan_digits(c);
}
// Up to 18 digits cannot overflow int64, so the common integer is read in
// one pass; a longer one goes through strtoll for its exact range check.
static const char *read_integer(const char *lexeme, size_t len, int64_t *out) {
  bool negative = lexeme[0] == '-';
  size_t digits = len - (negative ? 1 : 0);
  if (digits > 18) {
    errno = 0;
    *out = strtoll(lexeme, NULL, 10);
    return errno == ERANGE ? "integer outside the 64-bit range" : NULL;
  }
  int64_t value = 0;
  for (const char *d = lexeme + (negative ? 1 : 0); d < lexeme + len; d++) {
    value = value * 10 + (*d - '0');
  }
  *out = negative ? -value : value;
  return NULL;
}
// Typed numbers fit int64 or double; the default policy rejects non-finite
// doubles, while the explicit Python policy retains NaN and infinities.
const char *json_read_number(JNode *n, const char *lexeme, size_t len, bool python) {
  if (n->flag) {
    return read_integer(lexeme, len, &n->number.i);
  }
  n->number.f = strtod(lexeme, NULL);
  return python || isfinite(n->number.f) ? NULL : "number outside the float range";
}
