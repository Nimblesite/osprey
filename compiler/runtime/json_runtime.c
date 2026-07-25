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
  return 0;
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
    if (*c->p == '\\') {
      c->p++;
      switch (*c->p) {
      case '"': buf[0] = '"'; break;
      case '\\': buf[0] = '\\'; break;
      case '/': buf[0] = '/'; break;
      case 'b': buf[0] = '\b'; break;
      case 'f': buf[0] = '\f'; break;
      case 'n': buf[0] = '\n'; break;
      case 'r': buf[0] = '\r'; break;
      case 't': buf[0] = '\t'; break;
      case 'u': {
        unsigned long cp = 0;
        for (int i = 0; i < 4; i++) {
          c->p++;
          if (!*c->p) {
            free(out);
            c->ok = false;
            return NULL;
          }
          cp = (cp << 4) | hex_nibble(*c->p);
        }
        // High surrogate followed by low surrogate -> combine.
        if (cp >= 0xD800 && cp <= 0xDBFF && c->p[1] == '\\' && c->p[2] == 'u') {
          c->p += 2;
          unsigned long lo = 0;
          for (int i = 0; i < 4; i++) {
            c->p++;
            lo = (lo << 4) | hex_nibble(*c->p);
          }
          cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
        }
        n = utf8_encode(cp, buf);
        break;
      }
      default:
        buf[0] = *c->p;
        break;
      }
      c->p++;
    } else {
      buf[0] = *c->p;
      c->p++;
    }
    if (len + n + 1 > cap) {
      cap = (len + n + 1) * 2;
      char *nb = realloc(out, cap);
      if (!nb) {
        free(out);
        c->ok = false;
        return NULL;
      }
      out = nb;
    }
    memcpy(out + len, buf, n);
    len += n;
  }
  if (*c->p != '"') {
    free(out);
    c->ok = false;
    return NULL;
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

static JVal *parse_number(Cursor *c) {
  const char *start = c->p;
  if (*c->p == '-') {
    c->p++;
  }
  while ((*c->p >= '0' && *c->p <= '9') || *c->p == '.' || *c->p == 'e' ||
         *c->p == 'E' || *c->p == '+' || *c->p == '-') {
    c->p++;
  }
  size_t len = (size_t)(c->p - start);
  if (len == 0) {
    c->ok = false;
    return NULL;
  }
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
    char **nk = realloc(v->keys, (v->nmemb + 1) * sizeof(char *));
    JVal **nv = realloc(v->vals, (v->nmemb + 1) * sizeof(JVal *));
    if (!nk || !nv) {
      free(nk);
      free(nv);
      free(key);
      jval_free(val);
      jval_free(v);
      c->ok = false;
      return NULL;
    }
    v->keys = nk;
    v->vals = nv;
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
