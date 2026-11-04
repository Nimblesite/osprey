// The JSON runtime's internal surface, shared by its three units:
//   json_runtime.c  the strict RFC 8259 parser that builds a flat JDoc
//   json_text.c     JSON string text in both directions -- the literal decoder
//                   the parser uses and the escaper the writer uses, which
//                   must agree on UTF-8 and the escape alphabet byte for byte
//   json_codec.c    document handles, the typed-decode accessors and the
//                   single-buffer writer behind the built-in Json prelude
// Nothing here is visible to Osprey programs. Implements [BUILTIN-JSON].
#ifndef OSPREY_JSON_INTERNAL_H
#define OSPREY_JSON_INTERNAL_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef enum { J_NULL, J_BOOL, J_NUM, J_STR, J_ARR, J_OBJ } JType;

// A growable byte buffer, always NUL-terminated past `len`. Appends return
// false when the buffer cannot grow; `json_quote_into` also returns false
// when `s` is not valid UTF-8. Either way the bytes already appended stay,
// so callers abandon the buffer.
typedef struct {
  char *data;
  size_t len;
  size_t cap;
} JsonBuf;

bool json_buf_append(JsonBuf *out, const char *bytes, size_t n);
bool json_quote_into(JsonBuf *out, const char *s);

#define JSON_NO_KEY SIZE_MAX

// One value of a parsed document. Every piece of text -- a string's value, a
// member's name, a number's lexeme -- is a NUL-terminated run in the
// document's one text arena, and a container's children are a contiguous run
// of `slots`. A document is therefore four allocations however large it is,
// never one per value [BUILTIN-JSON-TYPED].
typedef struct {
  uint8_t type; // JType
  bool flag;    // J_BOOL: the value. J_NUM: the lexeme is an integer
  size_t key;   // arena offset of the member name, or JSON_NO_KEY
  size_t at;    // J_STR/J_NUM: arena offset of the text. J_ARR/J_OBJ: first slot
  size_t count; // J_ARR/J_OBJ: children
  union {
    int64_t i;
    double f;
  } number; // J_NUM read while parsing a typed document
} JNode;

// Slot 0 names the root; every container's children follow as one run.
typedef struct {
  JNode *nodes;
  size_t count;
  size_t cap;
  size_t *slots;
  size_t nslots;
  size_t slots_cap;
  JsonBuf text;
} JDoc;

// Containers nest at most this deep. The parser and the typed decoder both
// recurse once per level, so a hostile `[[[[...` must be a rejection, never a
// blown stack -- the iOS main thread has 1 MB of it [BUILTIN-JSON-LIMITS].
#define JSON_MAX_DEPTH 128

// The parse position. The FIRST rejection wins: `why` and `at` describe where
// the document stopped being JSON, and later unwinding never overwrites them.
typedef struct {
  const char *p;
  const char *base;
  const char *why;
  const char *at;
  size_t depth;
  bool ok;
} Cursor;

void json_reject(Cursor *c, const char *why);

// Decode the string literal at the cursor (on its opening quote) onto the
// end of `out`, NUL included. False, with the cursor rejected, when it is
// malformed or `out` cannot grow.
bool json_decode_string(Cursor *c, JsonBuf *out);

// The jsonParse status codes a rejection reports, all negative.
#define JSON_ERR_NULL ((int64_t)-1)
#define JSON_ERR_MALFORMED ((int64_t)-2)
#define JSON_ERR_TRAILING ((int64_t)-3)
#define JSON_ERR_TABLE_FULL ((int64_t)-4)

// Parse one complete JSON document. NULL on any rejection, with `*status`
// set to its code and the reason and byte offset recorded on the io-error
// channel under `op`. A `typed` parse also reads every number and rejects
// one the Json type cannot hold: an integer outside int64 or a float
// outside the finite range [BUILTIN-JSON-NUMBER].
JDoc *json_parse_doc(const char *text, const char *op, bool typed,
                     int64_t *status);
void json_doc_free(JDoc *doc);

// The node a slot names, or NULL for a slot outside the document.
const JNode *json_slot(const JDoc *doc, size_t slot);

// Bytes in the well-formed UTF-8 scalar starting at `p`, or 0 when `p` does
// not start one (overlong, surrogate, past U+10FFFF, truncated, stray
// continuation, or the terminating NUL). [BUILTIN-JSON-STRING]
size_t json_utf8_scalar_length(const char *p);

#endif // OSPREY_JSON_INTERNAL_H
