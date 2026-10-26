#include "http_client_internal.h"
#include "http_shared.h"

#include <strings.h> // strncasecmp

#include <pthread.h>

// Generic response-handle implementation, split from the client transport
// (http_client_runtime.c) so each TU stays under the repo size budget.
// Implements [HTTP-RESPONSE-HANDLE].

// http_header_is_chunked reports whether the response header block declares
// Transfer-Encoding: chunked (case-insensitive header name).
bool http_header_is_chunked(const char *headers) {
  if (!headers) {
    return false;
  }
  for (const char *p = headers; *p; p++) {
    if ((p[0] == 'T' || p[0] == 't') &&
        strncasecmp(p, "Transfer-Encoding:", 18) == 0) {
      const char *v = p + 18;
      return strstr(v, "chunked") != NULL || strstr(v, "Chunked") != NULL;
    }
  }
  return false;
}

// http_decode_chunked decodes an HTTP/1.1 chunked body in place: each chunk is
// "<hex-length>\r\n<data>\r\n", terminated by a zero-length chunk. Returns a
// freshly allocated, NUL-terminated decoded body (caller frees), or NULL.
char *http_decode_chunked(const char *body) {
  size_t cap = strlen(body) + 1;
  char *out = malloc(cap);
  if (!out) {
    return NULL;
  }
  size_t out_len = 0;
  const char *p = body;
  for (;;) {
    char *end = NULL;
    long chunk = strtol(p, &end, 16); // chunk size in hex
    if (end == p) {
      break; // no valid size: stop
    }
    const char *line_end = strstr(end, "\r\n");
    if (!line_end) {
      break;
    }
    const char *data = line_end + 2;
    if (chunk <= 0) {
      break; // terminating zero chunk
    }
    memcpy(out + out_len, data, (size_t)chunk);
    out_len += (size_t)chunk;
    p = data + chunk;
    if (strncmp(p, "\r\n", 2) == 0) {
      p += 2; // skip trailing CRLF after the chunk data
    }
  }
  out[out_len] = '\0';
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
  char *resp_body = NULL;
  if (http_header_is_chunked(resp_headers)) {
    resp_body = http_decode_chunked(body_start);
  }
  if (!resp_body) {
    resp_body = strdup(body_start);
  }
  free(raw);

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
