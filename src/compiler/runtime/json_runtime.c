// JSON runtime - the strict RFC 8259 recursive-descent parser every JSON
// entry point shares: the handle API (jsonParse) and the typed Json decoder
// both read documents through json_parse_doc, so they accept and reject
// exactly the same texts. String literals decode in json_text.c; handles,
// the decode accessors and the writer live in json_codec.c. Implements
// [BUILTIN-JSON], [BUILTIN-JSON-LIMITS].
//
// A document is built flat, in four growable arrays -- nodes, child slots,
// one text arena and a scratch stack of the open containers' children --
// so parsing costs a handful of allocations however large the document is,
// and freeing it costs four [BUILTIN-JSON-TYPED].

#include "io_error.h"
#include "json_internal.h"

#include <errno.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
  Cursor c;
  JDoc *doc;
  size_t *pending; // children of the containers still open, innermost last
  size_t npending;
  size_t pending_cap;
  const char **names; // scratch for checking a large object's names
  size_t names_cap;
  bool typed;
} Parser;

#define JSON_NO_NODE SIZE_MAX

void json_doc_free(JDoc *doc) {
  if (doc) {
    free(doc->nodes);
    free(doc->slots);
    free(doc->text.data);
    free(doc);
  }
}

const JNode *json_slot(const JDoc *doc, size_t slot) {
  return slot < doc->nslots ? &doc->nodes[doc->slots[slot]] : NULL;
}

// Room for one more element in a doubling array; false when it cannot grow.
static bool reserve(void **items, size_t *cap, size_t used, size_t size) {
  if (used < *cap) {
    return true;
  }
  size_t grown = *cap ? *cap * 2 : 64;
  void *moved = realloc(*items, grown * size);
  if (!moved) {
    return false;
  }
  *items = moved;
  *cap = grown;
  return true;
}

void json_reject(Cursor *c, const char *why) {
  if (c->ok) {
    c->ok = false;
    c->why = why;
    c->at = c->p;
  }
}

static size_t out_of_memory(Parser *ps) {
  json_reject(&ps->c, "out of memory");
  return JSON_NO_NODE;
}

// A fresh node of `type`; the caller fills the rest. Nodes are addressed by
// id, never by pointer, because the array moves as it grows.
static size_t new_node(Parser *ps, JType type) {
  JDoc *doc = ps->doc;
  if (!reserve((void **)&doc->nodes, &doc->cap, doc->count, sizeof(JNode))) {
    return out_of_memory(ps);
  }
  doc->nodes[doc->count] = (JNode){.type = (uint8_t)type, .key = JSON_NO_KEY};
  return doc->count++;
}

static void skip_ws(Cursor *c) {
  while (*c->p == ' ' || *c->p == '\t' || *c->p == '\n' || *c->p == '\r') {
    c->p++;
  }
}

static size_t parse_value(Parser *ps);

static size_t parse_string(Parser *ps) {
  size_t at = ps->doc->text.len;
  if (!json_decode_string(&ps->c, &ps->doc->text)) {
    return JSON_NO_NODE;
  }
  size_t id = new_node(ps, J_STR);
  if (id != JSON_NO_NODE) {
    ps->doc->nodes[id].at = at;
  }
  return id;
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
static bool scan_number(Cursor *c, bool *integer) {
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

// A typed document holds every number as the value the Json type gives it:
// an integer lexeme must fit int64 and any other must be a finite double, so
// nothing is rounded or clamped behind the program's back.
static const char *read_number(JNode *n, const char *lexeme, size_t len) {
  if (n->flag) {
    return read_integer(lexeme, len, &n->number.i);
  }
  n->number.f = strtod(lexeme, NULL);
  return isfinite(n->number.f) ? NULL : "number outside the float range";
}

static size_t number_node(Parser *ps, const char *start, bool integer) {
  size_t len = (size_t)(ps->c.p - start);
  size_t at = ps->doc->text.len;
  if (!json_buf_append(&ps->doc->text, start, len) ||
      !json_buf_append(&ps->doc->text, "", 1)) {
    return out_of_memory(ps);
  }
  size_t id = new_node(ps, J_NUM);
  if (id == JSON_NO_NODE) {
    return id;
  }
  JNode *n = &ps->doc->nodes[id];
  n->at = at;
  n->flag = integer;
  const char *bad = ps->typed ? read_number(n, ps->doc->text.data + at, len)
                              : NULL;
  if (bad) {
    ps->c.p = start; // name the number, not the byte after it
    json_reject(&ps->c, bad);
    return JSON_NO_NODE;
  }
  return id;
}

static size_t parse_number(Parser *ps) {
  const char *start = ps->c.p;
  bool integer = true;
  if (!scan_number(&ps->c, &integer)) {
    ps->c.p = start; // name the number, not the byte the scan stopped on
    json_reject(&ps->c, "invalid number");
    return JSON_NO_NODE;
  }
  return number_node(ps, start, integer);
}

static bool push_pending(Parser *ps, size_t id) {
  if (!reserve((void **)&ps->pending, &ps->pending_cap, ps->npending,
               sizeof(size_t))) {
    out_of_memory(ps);
    return false;
  }
  ps->pending[ps->npending++] = id;
  return true;
}

// Move the children pushed since `base` into the document's slots as the
// container's one contiguous run.
static size_t close_container(Parser *ps, size_t id, size_t base) {
  JDoc *doc = ps->doc;
  size_t count = ps->npending - base;
  while (doc->slots_cap < doc->nslots + count) {
    if (!reserve((void **)&doc->slots, &doc->slots_cap, doc->slots_cap,
                 sizeof(size_t))) {
      return out_of_memory(ps);
    }
  }
  // An empty container has no children to move, and `slots` may still be
  // NULL: forming NULL + 0 is undefined, so copy only when there is something.
  if (count != 0) {
    memcpy(doc->slots + doc->nslots, ps->pending + base,
           count * sizeof(size_t));
  }
  doc->nodes[id].at = doc->nslots;
  doc->nodes[id].count = count;
  doc->nslots += count;
  ps->npending = base;
  return id;
}

// After an element: true to read another, false once the closing bracket is
// consumed or the container is malformed (then `c->ok` is false).
static bool another(Cursor *c, char close, const char *why) {
  skip_ws(c);
  if (*c->p == ',') {
    c->p++;
    return true;
  }
  if (*c->p == close) {
    c->p++;
  } else {
    json_reject(c, why);
  }
  return false;
}

static size_t parse_array(Parser *ps) {
  ps->c.p++; // consume '['
  size_t id = new_node(ps, J_ARR);
  size_t base = ps->npending;
  skip_ws(&ps->c);
  bool more = id != JSON_NO_NODE && *ps->c.p != ']';
  if (!more && id != JSON_NO_NODE) {
    ps->c.p++;
  }
  while (more) {
    size_t item = parse_value(ps);
    more = item != JSON_NO_NODE && push_pending(ps, item) &&
           another(&ps->c, ']', "expected ',' or ']' in array");
  }
  return ps->c.ok ? close_container(ps, id, base) : JSON_NO_NODE;
}

static int compare_keys(const void *left, const void *right) {
  return strcmp(*(char *const *)left, *(char *const *)right);
}

static const char *member_name(const Parser *ps, size_t member) {
  return ps->doc->text.data + ps->doc->nodes[ps->pending[member]].key;
}

// Objects this small compare every pair: cheaper than sorting, and no
// allocation at all.
#define JSON_PAIRWISE_MEMBERS 8

static bool repeats_pairwise(const Parser *ps, size_t base) {
  for (size_t i = base; i < ps->npending; i++) {
    for (size_t j = i + 1; j < ps->npending; j++) {
      if (strcmp(member_name(ps, i), member_name(ps, j)) == 0) {
        return true;
      }
    }
  }
  return false;
}

// Whether two of the members pushed since `base` share a name. A large
// object sorts its names in a scratch array the parse reuses, so a huge
// object cannot turn the check into a denial of service.
static bool repeats_a_name(Parser *ps, size_t base, bool *failed) {
  size_t n = ps->npending - base;
  if (n <= JSON_PAIRWISE_MEMBERS) {
    return repeats_pairwise(ps, base);
  }
  while (ps->names_cap < n) {
    if (!reserve((void **)&ps->names, &ps->names_cap, ps->names_cap,
                 sizeof(char *))) {
      *failed = true;
      return false;
    }
  }
  for (size_t i = 0; i < n; i++) {
    ps->names[i] = member_name(ps, base + i);
  }
  qsort(ps->names, n, sizeof(char *), compare_keys);
  for (size_t i = 1; i < n; i++) {
    if (strcmp(ps->names[i - 1], ps->names[i]) == 0) {
      return true;
    }
  }
  return false;
}

// Two members with one name make every lookup a guess about which one the
// author meant -- first-wins here, last-wins in most other readers -- and
// hostile input exploits exactly that disagreement. A document that repeats a
// name is rejected whole, at the object's opening brace [BUILTIN-JSON-LIMITS].
static size_t close_object(Parser *ps, size_t id, size_t base,
                           const char *open) {
  bool oom = false;
  if (ps->npending - base > 1 && repeats_a_name(ps, base, &oom)) {
    ps->c.p = open;
    json_reject(&ps->c, "duplicate member name");
    return JSON_NO_NODE;
  }
  return oom ? out_of_memory(ps) : close_container(ps, id, base);
}

// One `"name": value` member, its name recorded on the value's node.
static size_t parse_member(Parser *ps) {
  skip_ws(&ps->c);
  size_t key = ps->doc->text.len;
  if (!json_decode_string(&ps->c, &ps->doc->text)) {
    json_reject(&ps->c, "expected a member name");
    return JSON_NO_NODE;
  }
  skip_ws(&ps->c);
  if (*ps->c.p != ':') {
    json_reject(&ps->c, "expected ':' after a member name");
    return JSON_NO_NODE;
  }
  ps->c.p++;
  size_t value = parse_value(ps);
  if (value != JSON_NO_NODE) {
    ps->doc->nodes[value].key = key;
  }
  return value;
}

static size_t parse_object(Parser *ps) {
  const char *open = ps->c.p;
  ps->c.p++; // consume '{'
  size_t id = new_node(ps, J_OBJ);
  size_t base = ps->npending;
  skip_ws(&ps->c);
  bool more = id != JSON_NO_NODE && *ps->c.p != '}';
  if (!more && id != JSON_NO_NODE) {
    ps->c.p++;
  }
  while (more) {
    size_t member = parse_member(ps);
    more = member != JSON_NO_NODE && push_pending(ps, member) &&
           another(&ps->c, '}', "expected ',' or '}' in object");
  }
  return ps->c.ok ? close_object(ps, id, base, open) : JSON_NO_NODE;
}

#define JSON_STRINGIFY(x) #x
#define JSON_DEPTH_TEXT(x) "nesting deeper than " JSON_STRINGIFY(x) " levels"

// Every container costs one parser frame, and the typed decoder another, so
// depth is bounded before the frame is pushed [BUILTIN-JSON-LIMITS].
static size_t parse_nested(Parser *ps, size_t (*container)(Parser *)) {
  if (ps->c.depth >= JSON_MAX_DEPTH) {
    json_reject(&ps->c, JSON_DEPTH_TEXT(JSON_MAX_DEPTH));
    return JSON_NO_NODE;
  }
  ps->c.depth++;
  size_t id = container(ps);
  ps->c.depth--;
  return id;
}

// `true`, `false` or `null` at the cursor.
static size_t parse_literal(Parser *ps, const char *word, JType type,
                            bool flag) {
  size_t len = strlen(word);
  if (strncmp(ps->c.p, word, len) != 0) {
    json_reject(&ps->c, "unexpected character");
    return JSON_NO_NODE;
  }
  ps->c.p += len;
  size_t id = new_node(ps, type);
  if (id != JSON_NO_NODE) {
    ps->doc->nodes[id].flag = flag;
  }
  return id;
}

static size_t parse_value(Parser *ps) {
  skip_ws(&ps->c);
  char lead = *ps->c.p;
  switch (lead) {
  case '"': return parse_string(ps);
  case '{': return parse_nested(ps, parse_object);
  case '[': return parse_nested(ps, parse_array);
  case 't': return parse_literal(ps, "true", J_BOOL, true);
  case 'f': return parse_literal(ps, "false", J_BOOL, false);
  case 'n': return parse_literal(ps, "null", J_NULL, false);
  default: break;
  }
  if (lead == '-' || (lead >= '0' && lead <= '9')) {
    return parse_number(ps);
  }
  json_reject(&ps->c, lead ? "unexpected character" : "unexpected end of text");
  return JSON_NO_NODE;
}

// Slot 0 names the root, so it is reserved before any container claims one.
static bool parse_root(Parser *ps) {
  JDoc *doc = ps->doc;
  if (!reserve((void **)&doc->slots, &doc->slots_cap, 0, sizeof(size_t)) ||
      !json_buf_append(&doc->text, "", 0)) {
    out_of_memory(ps);
    return false;
  }
  doc->nslots = 1;
  size_t root = parse_value(ps);
  doc->slots[0] = root;
  skip_ws(&ps->c);
  return root != JSON_NO_NODE;
}

static void record_rejection(const Cursor *c, const char *op) {
  char where[32];
  snprintf(where, sizeof(where), "byte %zu", (size_t)(c->at - c->base));
  osp_io_error_text(op, where, c->why);
}

// One complete value and nothing after it but whitespace. The reason names
// the byte where the text stopped being JSON, so `Error { message }` says why
// -- "jsonParse: byte 7: expected ',' or ']' in array" -- instead of the
// placeholder word "Error" [BUILTIN-JSON-STRING].
JDoc *json_parse_doc(const char *text, const char *op, bool typed,
                     int64_t *status) {
  if (!text) {
    *status = JSON_ERR_NULL;
    osp_io_error_text(op, "document", "no text");
    return NULL;
  }
  Parser ps = {.c = {.p = text, .base = text, .ok = true}, .typed = typed};
  ps.doc = calloc(1, sizeof(JDoc));
  bool parsed = ps.doc ? parse_root(&ps) : (out_of_memory(&ps), false);
  bool trailing = parsed && *ps.c.p != '\0';
  if (trailing) {
    json_reject(&ps.c, "trailing text after the value");
  }
  free(ps.pending);
  free(ps.names);
  if (ps.c.ok) {
    return ps.doc;
  }
  json_doc_free(ps.doc);
  *status = trailing ? JSON_ERR_TRAILING : JSON_ERR_MALFORMED;
  record_rejection(&ps.c, op);
  return NULL;
}
