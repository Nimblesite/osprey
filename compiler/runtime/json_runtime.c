// JSON runtime - a compact, self-contained recursive-descent JSON parser with
// path-based accessors. Implements [BUILTIN-JSON].
//
// Surface (all handle-based; handles are 1-based ints):
//   json_parse(s)            -> handle (>=1) or negative error
//   json_get(h, path)        -> scalar value as a string, or NULL if not a scalar
//   json_length(h, path)     -> element count for arrays/objects, or -1
//   json_free(h)             -> 0 on success, -1 on invalid/double free
//
// Path syntax: "a.b[0].c". Keys containing '.' or '[' are not addressable in v1.

#include <pthread.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MAX_JSON_DOCS 1024

typedef enum { J_NULL, J_BOOL, J_NUM, J_STR, J_ARR, J_OBJ } JType;

typedef struct JVal {
  JType type;
  bool bval;          // J_BOOL
  char *str;          // J_STR (decoded value) / J_NUM (raw token text)
  struct JVal **items; // J_ARR
  size_t count;        // J_ARR length
  char **keys;         // J_OBJ keys
  struct JVal **vals;  // J_OBJ values
  size_t nmemb;        // J_OBJ pair count
} JVal;

static JVal *g_json_docs[MAX_JSON_DOCS];
static pthread_mutex_t g_json_mutex = PTHREAD_MUTEX_INITIALIZER;

// ---- value construction / teardown ----------------------------------------

static JVal *jval_new(JType type) {
  JVal *v = calloc(1, sizeof(JVal));
  if (v) {
    v->type = type;
  }
  return v;
}

static void jval_free(JVal *v) {
  if (!v) {
    return;
  }
  switch (v->type) {
  case J_STR:
  case J_NUM:
    free(v->str);
    break;
  case J_ARR:
    for (size_t i = 0; i < v->count; i++) {
      jval_free(v->items[i]);
    }
    free(v->items);
    break;
  case J_OBJ:
    for (size_t i = 0; i < v->nmemb; i++) {
      free(v->keys[i]);
      jval_free(v->vals[i]);
    }
    free(v->keys);
    free(v->vals);
    break;
  default:
    break;
  }
  free(v);
}

// ---- parser ----------------------------------------------------------------

typedef struct {
  const char *p;
  bool ok;
} Cursor;

static void skip_ws(Cursor *c) {
  while (*c->p == ' ' || *c->p == '\t' || *c->p == '\n' || *c->p == '\r') {
    c->p++;
  }
}

static JVal *parse_value(Cursor *c);

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

static bool copy_utf8_sequence(Cursor *c, char *buf, size_t *n) {
  unsigned char lead = (unsigned char)*c->p;
  size_t len = utf8_sequence_length(lead);
  if (len == 0) {
    return false;
  }
  unsigned long cp = lead & (0xFFu >> (len + 1));
  for (size_t i = 1; i < len; i++) {
    unsigned char next = (unsigned char)c->p[i];
    if ((next & UTF8_CONTINUATION_MASK) != UTF8_CONTINUATION_MARK) {
      return false; // truncated -- the terminating NUL stops here too
    }
    cp = (cp << 6) | (next & 0x3Fu);
  }
  if (cp < UTF8_MIN_FOR_LENGTH[len] || cp > UNICODE_MAX ||
      is_high_surrogate(cp) || is_low_surrogate(cp)) {
    return false;
  }
  memcpy(buf, c->p, len);
  *n = len;
  c->p += len;
  return true;
}

// RFC 8259 requires U+0000..U+001F to be escaped inside a string. Taking a raw
// one literally makes this parser disagree with every other reader about where
// the string ends [BUILTIN-JSON-STRING].
#define JSON_LAST_CONTROL_CHAR 0x1F

// Free the half-built buffer and mark the document malformed. Returns NULL so
// each rejection inside the decode loop is a single statement and none of them
// can drift out of step with the others.
static char *string_rejected(Cursor *c, char *out) {
  free(out);
  c->ok = false;
  return NULL;
}

// Decode the one character at the cursor into `buf`/`*n`, advancing past it.
// False when the input is not a well-formed JSON string character.
static bool decode_string_char(Cursor *c, char *buf, size_t *n) {
  if ((unsigned char)*c->p <= JSON_LAST_CONTROL_CHAR) {
    return false;
  }
  if ((unsigned char)*c->p >= UTF8_CONTINUATION_MARK) {
    return copy_utf8_sequence(c, buf, n);
  }
  if (*c->p != '\\') {
    buf[0] = *c->p;
    c->p++;
    return true;
  }
  c->p++;
  bool ok = *c->p == 'u' ? decode_unicode_escape(c, buf, n)
                         : simple_escape(*c->p, buf);
  c->p++;
  return ok;
}

// Parses a JSON string literal (cursor positioned at the opening quote) into a
// freshly allocated, decoded, NUL-terminated C string.
static char *parse_string_raw(Cursor *c) {
  if (*c->p != '"') {
    c->ok = false;
    return NULL;
  }
  c->p++;
  size_t cap = 16;
  size_t len = 0;
  char *out = malloc(cap);
  if (!out) {
    c->ok = false;
    return NULL;
  }
  while (*c->p && *c->p != '"') {
    char buf[4];
    size_t n = 1;
    if (!decode_string_char(c, buf, &n)) {
      return string_rejected(c, out);
    }
    if (len + n + 1 > cap) {
      cap = (len + n + 1) * 2;
      char *nb = realloc(out, cap);
      if (!nb) {
        return string_rejected(c, out);
      }
      out = nb;
    }
    memcpy(out + len, buf, n);
    len += n;
  }
  if (*c->p != '"') {
    return string_rejected(c, out);
  }
  c->p++;
  out[len] = '\0';
  return out;
}

static JVal *parse_string(Cursor *c) {
  char *s = parse_string_raw(c);
  if (!s) {
    return NULL;
  }
  JVal *v = jval_new(J_STR);
  if (!v) {
    free(s);
    c->ok = false;
    return NULL;
  }
  v->str = s;
  return v;
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
// an optional exponent of at least one digit.
//
// The scanner this replaces took any run of [0-9.eE+-], so `-`, `1.`, `1e`,
// `1+2`, `--1` and `1e+` all parsed -- and since a number is STORED as its
// source text, every one of them came straight back out of jsonGet as a
// "number" no other reader would ever produce [BUILTIN-JSON-NUMBER].
static bool scan_number(Cursor *c) {
  if (*c->p == '-') {
    c->p++;
  }
  if (*c->p == '0') {
    c->p++; // a leading zero is the entire integer part
  } else if (!scan_digits(c)) {
    return false;
  }
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

static JVal *parse_number(Cursor *c) {
  const char *start = c->p;
  if (!scan_number(c)) {
    c->ok = false;
    return NULL;
  }
  size_t len = (size_t)(c->p - start);
  JVal *v = jval_new(J_NUM);
  if (!v) {
    c->ok = false;
    return NULL;
  }
  v->str = malloc(len + 1);
  if (!v->str) {
    jval_free(v);
    c->ok = false;
    return NULL;
  }
  memcpy(v->str, start, len);
  v->str[len] = '\0';
  return v;
}

static JVal *parse_array(Cursor *c) {
  c->p++; // consume '['
  JVal *v = jval_new(J_ARR);
  if (!v) {
    c->ok = false;
    return NULL;
  }
  skip_ws(c);
  if (*c->p == ']') {
    c->p++;
    return v;
  }
  for (;;) {
    JVal *item = parse_value(c);
    if (!c->ok) {
      jval_free(v);
      return NULL;
    }
    JVal **ni = realloc(v->items, (v->count + 1) * sizeof(JVal *));
    if (!ni) {
      jval_free(item);
      jval_free(v);
      c->ok = false;
      return NULL;
    }
    v->items = ni;
    v->items[v->count++] = item;
    skip_ws(c);
    if (*c->p == ',') {
      c->p++;
      skip_ws(c);
      continue;
    }
    if (*c->p == ']') {
      c->p++;
      return v;
    }
    jval_free(v);
    c->ok = false;
    return NULL;
  }
}

static JVal *parse_object(Cursor *c) {
  c->p++; // consume '{'
  JVal *v = jval_new(J_OBJ);
  if (!v) {
    c->ok = false;
    return NULL;
  }
  skip_ws(c);
  if (*c->p == '}') {
    c->p++;
    return v;
  }
  for (;;) {
    skip_ws(c);
    char *key = parse_string_raw(c);
    if (!key) {
      jval_free(v);
      c->ok = false;
      return NULL;
    }
    skip_ws(c);
    if (*c->p != ':') {
      free(key);
      jval_free(v);
      c->ok = false;
      return NULL;
    }
    c->p++;
    JVal *val = parse_value(c);
    if (!c->ok) {
      free(key);
      jval_free(v);
      return NULL;
    }
    // Each block is published into `v` the instant it exists. realloc has
    // already freed the old one, so holding a new block in a local while the
    // NEXT allocation runs leaves `v` pointing at freed memory — and the
    // cleanup below then walks it. That was a use-after-free reached by
    // nothing worse than a second realloc saying no.
    char **nk = realloc(v->keys, (v->nmemb + 1) * sizeof(char *));
    if (nk != NULL) {
      v->keys = nk;
    }
    JVal **nv = realloc(v->vals, (v->nmemb + 1) * sizeof(JVal *));
    if (nv != NULL) {
      v->vals = nv;
    }
    if (nk == NULL || nv == NULL) {
      free(key);
      jval_free(val);
      jval_free(v);
      c->ok = false;
      return NULL;
    }
    v->keys[v->nmemb] = key;
    v->vals[v->nmemb] = val;
    v->nmemb++;
    skip_ws(c);
    if (*c->p == ',') {
      c->p++;
      continue;
    }
    if (*c->p == '}') {
      c->p++;
      return v;
    }
    jval_free(v);
    c->ok = false;
    return NULL;
  }
}

static JVal *parse_value(Cursor *c) {
  skip_ws(c);
  switch (*c->p) {
  case '"':
    return parse_string(c);
  case '{':
    return parse_object(c);
  case '[':
    return parse_array(c);
  case 't':
    if (strncmp(c->p, "true", 4) == 0) {
      c->p += 4;
      JVal *v = jval_new(J_BOOL);
      if (v) {
        v->bval = true;
      } else {
        c->ok = false;
      }
      return v;
    }
    break;
  case 'f':
    if (strncmp(c->p, "false", 5) == 0) {
      c->p += 5;
      JVal *v = jval_new(J_BOOL);
      if (v) {
        v->bval = false;
      } else {
        c->ok = false;
      }
      return v;
    }
    break;
  case 'n':
    if (strncmp(c->p, "null", 4) == 0) {
      c->p += 4;
      JVal *v = jval_new(J_NULL);
      if (!v) {
        c->ok = false;
      }
      return v;
    }
    break;
  default:
    if (*c->p == '-' || (*c->p >= '0' && *c->p <= '9')) {
      return parse_number(c);
    }
    break;
  }
  c->ok = false;
  return NULL;
}

// ---- path navigation -------------------------------------------------------

static const JVal *navigate(const JVal *cur, const char *path) {
  const char *p = path;
  while (cur && *p) {
    if (*p == '.') {
      p++;
      continue;
    }
    if (*p == '[') {
      p++;
      long idx = 0;
      bool any = false;
      while (*p >= '0' && *p <= '9') {
        idx = idx * 10 + (*p - '0');
        p++;
        any = true;
      }
      if (!any || *p != ']') {
        return NULL;
      }
      p++;
      if (cur->type != J_ARR || idx < 0 || (size_t)idx >= cur->count) {
        return NULL;
      }
      cur = cur->items[idx];
    } else {
      const char *start = p;
      while (*p && *p != '.' && *p != '[') {
        p++;
      }
      size_t klen = (size_t)(p - start);
      if (cur->type != J_OBJ) {
        return NULL;
      }
      const JVal *next = NULL;
      for (size_t i = 0; i < cur->nmemb; i++) {
        if (strlen(cur->keys[i]) == klen &&
            strncmp(cur->keys[i], start, klen) == 0) {
          next = cur->vals[i];
          break;
        }
      }
      if (!next) {
        return NULL;
      }
      cur = next;
    }
  }
  return cur;
}

static bool valid_doc_handle(int64_t handle) {
  return handle >= 1 && handle < MAX_JSON_DOCS;
}

static const JVal *lookup(int64_t handle, const char *path) {
  if (!valid_doc_handle(handle) || !g_json_docs[handle]) {
    return NULL;
  }
  return navigate(g_json_docs[handle], path);
}

// ---- public API ------------------------------------------------------------

int64_t json_parse(char *s) {
  if (!s) {
    return -1;
  }
  Cursor c = {.p = s, .ok = true};
  JVal *root = parse_value(&c);
  if (!c.ok || !root) {
    jval_free(root);
    return -2;
  }
  skip_ws(&c);
  if (*c.p != '\0') {
    jval_free(root);
    return -3; // trailing garbage
  }

  pthread_mutex_lock(&g_json_mutex);
  int64_t handle = -1;
  for (int64_t i = 1; i < MAX_JSON_DOCS; i++) {
    if (!g_json_docs[i]) {
      g_json_docs[i] = root;
      handle = i;
      break;
    }
  }
  pthread_mutex_unlock(&g_json_mutex);

  if (handle < 0) {
    jval_free(root);
    return -4; // document table full
  }
  return handle;
}

char *json_get(int64_t handle, char *path) {
  pthread_mutex_lock(&g_json_mutex);
  const JVal *v = lookup(handle, path ? path : "");
  char *out = NULL;
  if (v) {
    switch (v->type) {
    case J_STR:
    case J_NUM:
      out = strdup(v->str);
      break;
    case J_BOOL:
      out = strdup(v->bval ? "true" : "false");
      break;
    case J_NULL:
      out = strdup("null");
      break;
    default:
      out = NULL; // arrays/objects are not scalars
      break;
    }
  }
  pthread_mutex_unlock(&g_json_mutex);
  return out;
}

int64_t json_length(int64_t handle, char *path) {
  pthread_mutex_lock(&g_json_mutex);
  const JVal *v = lookup(handle, path ? path : "");
  int64_t len = -1;
  if (v) {
    if (v->type == J_ARR) {
      len = (int64_t)v->count;
    } else if (v->type == J_OBJ) {
      len = (int64_t)v->nmemb;
    }
  }
  pthread_mutex_unlock(&g_json_mutex);
  return len;
}

int64_t json_free(int64_t handle) {
  if (!valid_doc_handle(handle)) {
    return -1;
  }
  pthread_mutex_lock(&g_json_mutex);
  int64_t rc = -1;
  if (g_json_docs[handle]) {
    jval_free(g_json_docs[handle]);
    g_json_docs[handle] = NULL;
    rc = 0;
  }
  pthread_mutex_unlock(&g_json_mutex);
  return rc;
}
