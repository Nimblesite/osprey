#include "http_client_internal.h"
#include "http_shared.h"
#include "io_error.h"

#include <strings.h> // strncasecmp

#include <pthread.h>

// Generic response-handle implementation, split from the client transport
// (http_client_runtime.c) so each TU stays under the repo size budget.
// Implements [HTTP-RESPONSE-HANDLE].

// http_header_is_chunked reports whether the response header block names chunked
// as the final coding of its Transfer-Encoding header (case-insensitive, per
// RFC 9112 6.1). Any other header that merely mentions either word does not count.
bool http_header_is_chunked(const char *headers) {
  static const size_t name_len = 18;
  static const size_t coding_len = 7;
  const char *line = headers;
  while (line && *line) {
    const char *eol = strstr(line, "\r\n");
    if (strncasecmp(line, "Transfer-Encoding:", name_len) == 0) {
      const char *value = line + name_len;
      const char *end = eol ? eol : value + strlen(value);
      while (end > value && (end[-1] == ' ' || end[-1] == '\t')) {
        end--;
      }
      if ((size_t)(end - value) < coding_len ||
          strncasecmp(end - coding_len, "chunked", coding_len) != 0) {
        return false;
      }
      char before = end[-(ptrdiff_t)coding_len - 1];
      return before == ':' || before == ' ' || before == '\t' || before == ',';
    }
    line = eol ? eol + 2 : NULL;
  }
  return false;
}

static const char CHUNK_TOO_BIG[] = "chunk size exceeds the remaining body";
static const char CHUNK_BAD_SIZE[] = "invalid chunk size";
static const char CHUNK_NO_CRLF[] = "chunk data not followed by CRLF";
static const char CHUNK_CUT_SHORT[] = "truncated";

static const char *find_crlf(const char *p, const char *end) {
  for (; end - p >= 2; p++) {
    if (p[0] == '\r' && p[1] == '\n') {
      return p;
    }
  }
  return NULL;
}

static int hex_digit(char c) {
  if (c >= '0' && c <= '9') {
    return c - '0';
  }
  if (c >= 'a' && c <= 'f') {
    return c - 'a' + 10;
  }
  return c >= 'A' && c <= 'F' ? c - 'A' + 10 : -1;
}

// Parses "<hex-size>[ws][;ext]\r\n" at *cursor and moves past it. Returns NULL,
// or why the line is malformed. A size that overflows is malformed, so it can
// never wrap round to a small value that passes the caller's bound.
static const char *parse_chunk_size(const char **cursor, const char *end,
                                    size_t *size) {
  const char *p = *cursor;
  const char *digits = p;
  *size = 0;
  for (; p < end && hex_digit(*p) >= 0; p++) {
    if (*size > SIZE_MAX / 16) {
      return CHUNK_BAD_SIZE;
    }
    *size = *size * 16 + (size_t)hex_digit(*p);
  }
  if (p == end) {
    return CHUNK_CUT_SHORT;
  }
  if (p == digits) {
    return CHUNK_BAD_SIZE;
  }
  while (p < end && (*p == ' ' || *p == '\t')) {
    p++;
  }
  if (p == end) {
    return CHUNK_CUT_SHORT;
  }
  if (*p != ';' && *p != '\r') {
    return CHUNK_BAD_SIZE;
  }
  const char *line_end = find_crlf(p, end);
  if (!line_end) {
    return *p == ';' || end - p == 1 ? CHUNK_CUT_SHORT : CHUNK_BAD_SIZE;
  }
  if (*p == '\r' && line_end != p) {
    return CHUNK_BAD_SIZE;
  }
  *cursor = line_end + 2;
  return NULL;
}

// Decodes every chunk of [p, end) into out, which holds end - p + 1 bytes: the
// decoded bytes can't outnumber the framed ones. The body ends at the blank line
// after the zero chunk's trailers, and bytes after it are ignored, as the
// connection closes there. Returns NULL, or why the framing is malformed.
static const char *decode_chunks(const char *p, const char *end, char *out) {
  size_t out_len = 0;
  for (;;) {
    size_t size = 0;
    const char *malformed = parse_chunk_size(&p, end, &size);
    if (malformed) {
      return malformed;
    }
    if (size == 0) {
      break;
    }
    if (size > (size_t)(end - p)) {
      return CHUNK_TOO_BIG;
    }
    memcpy(out + out_len, p, size);
    out_len += size;
    p += size;
    if (end - p < 2) {
      return CHUNK_CUT_SHORT;
    }
    if (p[0] != '\r' || p[1] != '\n') {
      return CHUNK_NO_CRLF;
    }
    p += 2;
  }
  out[out_len] = '\0';
  for (;;) {
    const char *line_end = find_crlf(p, end);
    if (!line_end) {
      return CHUNK_CUT_SHORT;
    }
    if (line_end == p) {
      return NULL;
    }
    p = line_end + 2;
  }
}

// http_decode_chunked decodes the len bytes of an HTTP/1.1 chunked body. Returns
// a freshly allocated, NUL-terminated body (caller frees), or NULL with *malformed
// saying what is wrong with the framing, or NULL with *malformed NULL when out of
// memory. Nothing partial comes back. Implements [HTTP-CHUNKED-DECODE].
char *http_decode_chunked(const char *body, size_t len, const char **malformed) {
  *malformed = NULL;
  char *out = malloc(len + 1);
  if (!out) {
    return NULL;
  }
  *malformed = decode_chunks(body, body + len, out);
  if (*malformed) {
    free(out);
    return NULL;
  }
  return out;
}

// http_parse_status_line returns the numeric status from an HTTP status line, or -8.
int64_t http_parse_status_line(const char *raw) {
  if (strncmp(raw, "HTTP/1.1 ", 9) == 0 || strncmp(raw, "HTTP/1.0 ", 9) == 0) {
    return atoi(raw + 9);
  }
  return -8;
}

// ---------------------------------------------------------------------------
// Response-handle API. Implements [HTTP-RESPONSE-HANDLE].
// ---------------------------------------------------------------------------

#define MAX_HTTP_RESPONSES 1024

typedef struct {
  bool in_use;
  int64_t status;
  char *body;        // response body (NUL-terminated, heap)
  char *raw_headers; // header block between status line and blank line (heap)
} HttpClientResponse;

static HttpClientResponse g_http_responses[MAX_HTTP_RESPONSES];
static pthread_mutex_t g_http_resp_mutex = PTHREAD_MUTEX_INITIALIZER;

static bool valid_response_handle(int64_t handle) {
  return handle >= 1 && handle < MAX_HTTP_RESPONSES;
}

// http_request_capture performs the request, retains status/body/headers in a
// heap slot, and returns a 1-based handle (>=1) or a negative error code.
static int64_t http_request_capture(int64_t client_id, int64_t method,
                                    char *path, char *headers, char *body) {
  char *raw = NULL;
  size_t len = 0;
  int64_t rc = http_perform(client_id, method, path, headers, body, &raw, &len);
  if (rc != 0) {
    return rc;
  }

  int64_t status = http_parse_status_line(raw);

  char *sep = strstr(raw, "\r\n\r\n");
  char *body_start = sep ? sep + 4 : raw;
  char *first_eol = strstr(raw, "\r\n");

  char *resp_headers = NULL;
  if (first_eol && sep && sep >= first_eol + 2) {
    size_t hlen = (size_t)(sep - (first_eol + 2));
    resp_headers = malloc(hlen + 1);
    if (resp_headers) {
      memcpy(resp_headers, first_eol + 2, hlen);
      resp_headers[hlen] = '\0';
    }
  }

  // Decode a chunked body so callers see the real payload, not chunk framing.
  size_t body_len = len - (size_t)(body_start - raw);
  const char *malformed = NULL;
  char *resp_body = http_header_is_chunked(resp_headers)
                        ? http_decode_chunked(body_start, body_len, &malformed)
                        : strdup(body_start);
  free(raw);

  if (malformed) {
    free(resp_headers);
    osp_io_error_text("http response", "chunked body", malformed);
    return -14; // broken framing is never shown to the caller as a body
  }
  if (!resp_body) {
    free(resp_headers);
    return -9;
  }

  pthread_mutex_lock(&g_http_resp_mutex);
  int64_t handle = -1;
  for (int64_t i = 1; i < MAX_HTTP_RESPONSES; i++) {
    if (!g_http_responses[i].in_use) {
      g_http_responses[i].in_use = true;
      g_http_responses[i].status = status;
      g_http_responses[i].body = resp_body;
      g_http_responses[i].raw_headers = resp_headers;
      handle = i;
      break;
    }
  }
  pthread_mutex_unlock(&g_http_resp_mutex);

  if (handle < 0) {
    free(resp_body);
    free(resp_headers);
    return -10; // response table full
  }
  return handle;
}

int64_t http_get_response(int64_t client_id, char *path, char *headers) {
  return http_request_capture(client_id, HTTP_GET, path, headers, NULL);
}

// POST twin of http_get_response: the request body is sent and the response
// body/headers are retained behind the returned handle [HTTP-RESPONSE-HANDLE].
int64_t http_post_response(int64_t client_id, char *path, char *body,
                           char *headers) {
  return http_request_capture(client_id, HTTP_POST, path, headers, body);
}

int64_t http_response_status(int64_t handle) {
  if (!valid_response_handle(handle)) {
    return -1;
  }
  pthread_mutex_lock(&g_http_resp_mutex);
  int64_t status = g_http_responses[handle].in_use
                       ? g_http_responses[handle].status
                       : -1;
  pthread_mutex_unlock(&g_http_resp_mutex);
  return status;
}

// Returns a fresh copy of the body so it survives http_response_free.
char *http_response_body(int64_t handle) {
  if (!valid_response_handle(handle)) {
    return NULL;
  }
  pthread_mutex_lock(&g_http_resp_mutex);
  char *copy = NULL;
  if (g_http_responses[handle].in_use && g_http_responses[handle].body) {
    copy = strdup(g_http_responses[handle].body);
  }
  pthread_mutex_unlock(&g_http_resp_mutex);
  return copy;
}

// Case-insensitive prefix match of "name:" at the start of a header line.
static char *find_header_value(const char *headers, const char *name) {
  if (!headers || !name) {
    return NULL;
  }
  size_t name_len = strlen(name);
  const char *line = headers;
  while (line && *line) {
    bool match = true;
    for (size_t i = 0; i < name_len; i++) {
      char a = line[i];
      char b = name[i];
      if (a >= 'A' && a <= 'Z') {
        a = (char)(a - 'A' + 'a');
      }
      if (b >= 'A' && b <= 'Z') {
        b = (char)(b - 'A' + 'a');
      }
      if (a != b || a == '\0') {
        match = false;
        break;
      }
    }
    if (match && line[name_len] == ':') {
      const char *value = line + name_len + 1;
      while (*value == ' ' || *value == '\t') {
        value++;
      }
      const char *end = strstr(value, "\r\n");
      size_t vlen = end ? (size_t)(end - value) : strlen(value);
      char *out = malloc(vlen + 1);
      if (!out) {
        return NULL;
      }
      memcpy(out, value, vlen);
      out[vlen] = '\0';
      return out;
    }
    const char *next = strstr(line, "\r\n");
    line = next ? next + 2 : NULL;
  }
  return NULL;
}

char *http_response_header(int64_t handle, char *name) {
  if (!valid_response_handle(handle)) {
    return NULL;
  }
  pthread_mutex_lock(&g_http_resp_mutex);
  char *value = NULL;
  if (g_http_responses[handle].in_use) {
    value = find_header_value(g_http_responses[handle].raw_headers, name);
  }
  pthread_mutex_unlock(&g_http_resp_mutex);
  return value;
}

// Frees a response slot. Returns 0 on success, -1 on invalid/double free.
int64_t http_response_free(int64_t handle) {
  if (!valid_response_handle(handle)) {
    return -1;
  }
  pthread_mutex_lock(&g_http_resp_mutex);
  int64_t rc = -1;
  if (g_http_responses[handle].in_use) {
    free(g_http_responses[handle].body);
    free(g_http_responses[handle].raw_headers);
    g_http_responses[handle].body = NULL;
    g_http_responses[handle].raw_headers = NULL;
    g_http_responses[handle].in_use = false;
    rc = 0;
  }
  pthread_mutex_unlock(&g_http_resp_mutex);
  return rc;
}
