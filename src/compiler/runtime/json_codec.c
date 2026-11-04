// JSON document handles: the jsonParse/jsonGet surface, the accessors the
// built-in Json prelude decodes through, and the single-buffer writer it
// encodes through. Every handle is a slot in a table, so a stale, forged or
// double-freed handle is a refusal, never a dereference. Implements
// [BUILTIN-JSON], [BUILTIN-JSON-TYPED].
//
// Path syntax (jsonGet/jsonLength): "a.b[0].c". Keys containing '.' or '['
// are not addressable by path; the typed decoder walks slots instead.

#ifndef __wasm__
#include <pthread.h>
#endif
#include "io_error.h"
#include "json_internal.h"

#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

char *osp_float_to_string(double d);

#define MAX_JSON_HANDLES 1024

static bool valid_handle(int64_t handle, int64_t capacity) {
  return handle >= 1 && handle < capacity;
}

// Claim the first free slot of `table` for `item`; -1 when it is full.
static int64_t claim_slot(void **table, int64_t capacity, void *item) {
  for (int64_t i = 1; i < capacity; i++) {
    if (!table[i]) {
      table[i] = item;
      return i;
    }
  }
  return -1;
}

// ---- handle API ------------------------------------------------------------
//
// jsonParse documents are shared: any fiber may read or free any handle, so
// the table is lock-guarded. wasm32-wasip1 runs one thread -- the fiber
// runtime is not in its archive -- and the WASI SDK that CI pins ships no
// pthread mutex, so the lock compiles away there. [WASM-TARGET]

static JDoc *g_json_docs[MAX_JSON_HANDLES];

#ifdef __wasm__
static void json_table_lock(void) {}
static void json_table_unlock(void) {}
#else
static pthread_mutex_t g_json_mutex = PTHREAD_MUTEX_INITIALIZER;
static void json_table_lock(void) { pthread_mutex_lock(&g_json_mutex); }
static void json_table_unlock(void) { pthread_mutex_unlock(&g_json_mutex); }
#endif

static const char *text_at(const JDoc *doc, size_t offset) {
  return doc->text.data + offset;
}

static const JNode *child_at(const JDoc *doc, const JNode *n, size_t i) {
  return &doc->nodes[doc->slots[n->at + i]];
}

// The member of object `n` named by the `len` bytes at `name`, or NULL.
static const JNode *member(const JDoc *doc, const JNode *n, const char *name,
                           size_t len) {
  for (size_t i = 0; n->type == J_OBJ && i < n->count; i++) {
    const JNode *m = child_at(doc, n, i);
    const char *key = text_at(doc, m->key);
    if (strncmp(key, name, len) == 0 && key[len] == '\0') {
      return m;
    }
  }
  return NULL;
}

// `[digits]` at `*p`, consumed; the element it names, or NULL.
static const JNode *element(const JDoc *doc, const JNode *n, const char **p) {
  const char *s = *p + 1;
  size_t index = 0;
  const char *digits = s;
  while (*s >= '0' && *s <= '9' && index <= SIZE_MAX / 10 - 9) {
    index = index * 10 + (size_t)(*s - '0');
    s++;
  }
  *p = s + 1;
  bool well_formed = s != digits && *s == ']';
  return well_formed && n->type == J_ARR && index < n->count
             ? child_at(doc, n, index)
             : NULL;
}

static const JNode *navigate(const JDoc *doc, const char *path) {
  const JNode *cur = json_slot(doc, 0);
  const char *p = path;
  while (cur && *p) {
    if (*p == '.') {
      p++;
    } else if (*p == '[') {
      cur = element(doc, cur, &p);
    } else {
      const char *start = p;
      p += strcspn(p, ".[");
      cur = member(doc, cur, start, (size_t)(p - start));
    }
  }
  return cur;
}

// The document and node at `path`, or NULL. Callers hold the lock.
static const JNode *lookup(int64_t handle, const char *path, JDoc **doc) {
  *doc = valid_handle(handle, MAX_JSON_HANDLES) ? g_json_docs[handle] : NULL;
  return *doc ? navigate(*doc, path ? path : "") : NULL;
}

int64_t json_parse(char *s) {
  int64_t status = JSON_ERR_MALFORMED;
  JDoc *doc = json_parse_doc(s, "jsonParse", false, &status);
  if (!doc) {
    return status;
  }
  json_table_lock();
  int64_t handle = claim_slot((void **)g_json_docs, MAX_JSON_HANDLES, doc);
  json_table_unlock();
  if (handle < 0) {
    json_doc_free(doc);
    osp_io_error_text("jsonParse", "document", "too many open documents");
    return JSON_ERR_TABLE_FULL;
  }
  return handle;
}

static const char *scalar_text(const JDoc *doc, const JNode *n) {
  switch (n->type) {
  case J_STR:
  case J_NUM: return text_at(doc, n->at);
  case J_BOOL: return n->flag ? "true" : "false";
  case J_NULL: return "null";
  default: return NULL; // arrays/objects are not scalars
  }
}

char *json_get(int64_t handle, char *path) {
  json_table_lock();
  JDoc *doc = NULL;
  const JNode *n = lookup(handle, path, &doc);
  const char *text = n ? scalar_text(doc, n) : NULL;
  char *out = text ? strdup(text) : NULL;
  json_table_unlock();
  return out;
}

int64_t json_length(int64_t handle, char *path) {
  json_table_lock();
  JDoc *doc = NULL;
  const JNode *n = lookup(handle, path, &doc);
  bool counted = n && (n->type == J_ARR || n->type == J_OBJ);
  int64_t len = counted ? (int64_t)n->count : -1;
  json_table_unlock();
  return len;
}

int64_t json_free(int64_t handle) {
  if (!valid_handle(handle, MAX_JSON_HANDLES)) {
    return -1;
  }
  json_table_lock();
  JDoc *doc = g_json_docs[handle];
  g_json_docs[handle] = NULL;
  json_table_unlock();
  if (!doc) {
    return -1;
  }
  json_doc_free(doc);
  return 0;
}

// ---- typed decode ----------------------------------------------------------
//
// jsonDecode opens a document, walks it once and closes it, and the walk
// calls no program code and performs no effect -- so it never leaves the
// thread that opened the document, and no other thread can name it. The
// table is therefore thread-local and every accessor is lock-free: the walk
// asks one or two questions per value, and a mutex on each would cost more
// than the answer [BUILTIN-JSON-TYPED].

#define MAX_DECODE_HANDLES 64

static _Thread_local JDoc *t_decodes[MAX_DECODE_HANDLES];

int64_t json_decode_open(char *text) {
  int64_t status = JSON_ERR_MALFORMED;
  JDoc *doc = json_parse_doc(text, "jsonDecode", true, &status);
  if (!doc) {
    return status;
  }
  int64_t handle = claim_slot((void **)t_decodes, MAX_DECODE_HANDLES, doc);
  if (handle < 0) {
    json_doc_free(doc);
    osp_io_error_text("jsonDecode", "document", "too many open documents");
    return JSON_ERR_TABLE_FULL;
  }
  return handle;
}

int64_t json_decode_close(int64_t handle) {
  JDoc *doc = valid_handle(handle, MAX_DECODE_HANDLES) ? t_decodes[handle]
                                                       : NULL;
  if (!doc) {
    osp_io_error_text("jsonDecode", "document", "invalid document handle");
    return -1;
  }
  t_decodes[handle] = NULL;
  json_doc_free(doc);
  return 0;
}

// The node at `slot` of an open decode, or NULL.
static const JNode *decoded(int64_t handle, int64_t slot, const JDoc **doc) {
  *doc = valid_handle(handle, MAX_DECODE_HANDLES) ? t_decodes[handle] : NULL;
  return *doc && slot >= 0 ? json_slot(*doc, (size_t)slot) : NULL;
}

// The kind codes the prelude matches on. Booleans carry their value in the
// kind, and numbers their int/float classification, so one call per value
// decides its variant.
enum { K_NULL, K_FALSE, K_TRUE, K_INT, K_FLOAT, K_STRING, K_ARRAY, K_OBJECT };

static int64_t kind_of(const JNode *n) {
  switch (n->type) {
  case J_NULL: return K_NULL;
  case J_BOOL: return n->flag ? K_TRUE : K_FALSE;
  case J_NUM: return n->flag ? K_INT : K_FLOAT;
  case J_STR: return K_STRING;
  case J_ARR: return K_ARRAY;
  default: return K_OBJECT;
  }
}

int64_t json_node_kind(int64_t handle, int64_t slot) {
  const JDoc *doc = NULL;
  const JNode *n = decoded(handle, slot, &doc);
  return n ? kind_of(n) : -1;
}

static bool is_container(const JNode *n) {
  return n && (n->type == J_ARR || n->type == J_OBJ);
}

// A container's children are the slots [first, end), so the prelude ranges
// over them directly; anything else has the empty range.
int64_t json_node_first(int64_t handle, int64_t slot) {
  const JDoc *doc = NULL;
  const JNode *n = decoded(handle, slot, &doc);
  return is_container(n) ? (int64_t)n->at : 0;
}

int64_t json_node_end(int64_t handle, int64_t slot) {
  const JDoc *doc = NULL;
  const JNode *n = decoded(handle, slot, &doc);
  return is_container(n) ? (int64_t)(n->at + n->count) : 0;
}

// A fresh copy the caller owns; "" when the question has no answer.
static char *owned(const char *text) { return strdup(text ? text : ""); }

// The member name of an object's child; "" for any other slot.
char *json_node_key(int64_t handle, int64_t slot) {
  const JDoc *doc = NULL;
  const JNode *n = decoded(handle, slot, &doc);
  return owned(n && n->key != JSON_NO_KEY ? text_at(doc, n->key) : NULL);
}

// A string's value; "" for any other slot.
char *json_node_text(int64_t handle, int64_t slot) {
  const JDoc *doc = NULL;
  const JNode *n = decoded(handle, slot, &doc);
  return owned(n && n->type == J_STR ? text_at(doc, n->at) : NULL);
}

// An integer's value; 0 for any other slot.
int64_t json_node_int(int64_t handle, int64_t slot) {
  const JDoc *doc = NULL;
  const JNode *n = decoded(handle, slot, &doc);
  return n && kind_of(n) == K_INT ? n->number.i : 0;
}

// A float's value; 0.0 for any other slot.
double json_node_float(int64_t handle, int64_t slot) {
  const JDoc *doc = NULL;
  const JNode *n = decoded(handle, slot, &doc);
  return n && kind_of(n) == K_FLOAT ? n->number.f : 0.0;
}

// ---- single-buffer writer --------------------------------------------------
//
// The prelude walks a Json value and appends each token here; the document is
// built in one growing C buffer and handed to Osprey as ONE string when the
// writer closes. Separators are the writer's job: a value that follows a
// value gets its ',', and `[`, `{` and `:` reset that, so the walk is a plain
// traversal. Failure is sticky: the first refusal is kept, later writes are
// no-ops, and close reports it -- so the walk needs no per-token checks.
//
// Like a decode, an encode walk never leaves its thread, so writers live in
// a lock-free thread-local table.

typedef struct {
  JsonBuf buf;
  const char *why;
  bool after_value; // the next value needs a ',' in front of it
} JWriter;

#define MAX_WRITER_HANDLES 64

static _Thread_local JWriter *t_writers[MAX_WRITER_HANDLES];

int64_t json_writer_open(void) {
  JWriter *w = calloc(1, sizeof(JWriter));
  int64_t handle =
      w ? claim_slot((void **)t_writers, MAX_WRITER_HANDLES, w) : -1;
  if (handle < 0) {
    free(w);
    osp_io_error_text("jsonEncode", "value",
                      w ? "too many open writers" : "out of memory");
  }
  return handle;
}

// The live writer behind `handle`, or NULL.
static JWriter *writer_at(int64_t handle) {
  JWriter *w = valid_handle(handle, MAX_WRITER_HANDLES) ? t_writers[handle]
                                                        : NULL;
  return w && !w->why ? w : NULL;
}

static int64_t writer_status(JWriter *w, bool ok, const char *why) {
  if (w && !ok && !w->why) {
    w->why = why;
  }
  return w && !w->why ? 0 : -1;
}

// Start a value: its separator, then whether it leaves a value behind (a
// scalar does; an opening bracket does not until it closes).
static bool begin_value(JWriter *w, bool complete) {
  bool ok = !w->after_value || json_buf_append(&w->buf, ",", 1);
  w->after_value = complete;
  return ok;
}

// A structural token or a literal (`null`, `true`, `false`).
int64_t json_write_raw(int64_t handle, char *token) {
  JWriter *w = writer_at(handle);
  if (!w || !token) {
    return writer_status(w, false, "out of memory");
  }
  char lead = token[0];
  bool closes = lead == ']' || lead == '}';
  bool ok = closes || lead == ':' || begin_value(w, lead != '[' && lead != '{');
  w->after_value = closes || (w->after_value && lead != ':');
  ok = ok && json_buf_append(&w->buf, token, strlen(token));
  return writer_status(w, ok, "out of memory");
}

int64_t json_write_string(int64_t handle, char *s) {
  JWriter *w = writer_at(handle);
  bool ok = w && s && begin_value(w, true) && json_quote_into(&w->buf, s);
  return writer_status(w, ok, "string is not valid UTF-8");
}

int64_t json_write_int(int64_t handle, int64_t n) {
  char digits[24];
  int len = snprintf(digits, sizeof(digits), "%lld", (long long)n);
  JWriter *w = writer_at(handle);
  bool ok = w && begin_value(w, true) &&
            json_buf_append(&w->buf, digits, (size_t)len);
  return writer_status(w, ok, "out of memory");
}

// The float spelling toString prints, which keeps a `.0` or exponent so the
// text decodes back to a float. NaN and infinities have no JSON spelling.
int64_t json_write_float(int64_t handle, double f) {
  JWriter *w = writer_at(handle);
  if (!isfinite(f)) {
    return writer_status(w, false, "float is not finite");
  }
  char *text = w ? osp_float_to_string(f) : NULL;
  bool ok = text && begin_value(w, true) &&
            json_buf_append(&w->buf, text, strlen(text));
  free(text);
  return writer_status(w, ok, "out of memory");
}

char *json_writer_close(int64_t handle) {
  JWriter *w = valid_handle(handle, MAX_WRITER_HANDLES) ? t_writers[handle]
                                                        : NULL;
  if (w) {
    t_writers[handle] = NULL;
  }
  const char *why = !w ? "invalid writer handle"
                    : w->why ? w->why
                    : !w->buf.data && !json_buf_append(&w->buf, "", 0)
                        ? "out of memory"
                        : NULL;
  char *out = w && !why ? w->buf.data : NULL;
  if (w && why) {
    free(w->buf.data);
  }
  free(w);
  if (why) {
    osp_io_error_text("jsonEncode", "value", why);
  }
  return out;
}
