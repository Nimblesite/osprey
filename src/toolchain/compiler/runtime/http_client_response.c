#include "http_client_internal.h"
#include "http_shared.h"
#include "string_runtime.h"

#include <pthread.h>

// Generic response-handle implementation, split from the client transport
// (http_client_runtime.c) so each TU stays under the repo size budget.
// Implements [HTTP-RESPONSE-HANDLE].

// ---------------------------------------------------------------------------
// Response-handle API. Implements [HTTP-RESPONSE-HANDLE].
// ---------------------------------------------------------------------------

#define MAX_HTTP_RESPONSES 1024

typedef struct {
  bool in_use;
  int64_t status;
  HttpBody body;    // framed bytes in libc storage until the handle closes
  char *raw_headers; // header block between status line and blank line (heap)
} HttpClientResponse;

static HttpClientResponse g_http_responses[MAX_HTTP_RESPONSES];
static pthread_mutex_t g_http_resp_mutex = PTHREAD_MUTEX_INITIALIZER;

static bool valid_response_handle(int64_t handle) {
  return handle >= 1 && handle < MAX_HTTP_RESPONSES;
}

// http_request_capture performs the request, retains status/body/headers in a
// heap slot, and returns a 1-based handle (>=1) or a negative error code.
// Implements [HTTP-RESPONSE-FRAMING].
static int64_t http_request_capture(int64_t client_id, int64_t method,
                                    char *path, char *headers, char *body) {
  char *raw = NULL;
  size_t len = 0;
  bool closed = false;
  int64_t rc = http_perform(client_id, method, path, headers, body, &raw, &len, &closed);
  if (rc != 0) {
    return rc;
  }
  const char *sep = NULL;
  int64_t status = 0;
  const char *head = http_checked_head(raw, len, &sep, &status, &rc);
  if (!head) {
    free(raw);
    return rc;
  }
  char *resp_headers = NULL;
  if (!http_copy_response_headers(head, sep, &resp_headers)) {
    free(raw);
    return -9; // without its headers, a chunked body would pass for a plain one
  }

  // Decode a chunked body so callers see the real payload, not chunk framing.
  const char *body_start = sep + 4;
  HttpBody resp_body;
  rc = http_response_body_of(status, resp_headers, body_start,
                             len - (size_t)(body_start - raw), closed, &resp_body);
  free(raw);
  if (rc != 0) {
    free(resp_headers);
    return rc;
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
    free(resp_body.data);
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
  if (g_http_responses[handle].in_use) {
    HttpBody body = g_http_responses[handle].body;
    copy = osp_string_dup_internal(body.data, body.length);
  }
  pthread_mutex_unlock(&g_http_resp_mutex);
  return copy;
}

// The value of the first field named `name` in a head, a fresh copy: the names
// read in any case and the value trimmed, as RFC 9110 5.1 and 5.5 read them
// [HTTP-FIELDS]. NULL when there is none or the copy cannot be had.
static char *find_header_value(const char *headers, const char *name) {
  if (!headers || !name) {
    return NULL;
  }
  size_t cursor = 0;
  HttpField field;
  while (http_next_field(headers, strlen(headers), &cursor, &field)) {
    if (http_text_is(field.name, field.name_len, name)) {
      char *out = malloc(field.value_len + 1);
      if (out) {
        memcpy(out, field.value, field.value_len);
        out[field.value_len] = '\0';
      }
      return out;
    }
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
    free(g_http_responses[handle].body.data);
    free(g_http_responses[handle].raw_headers);
    g_http_responses[handle].body = (HttpBody){0};
    g_http_responses[handle].raw_headers = NULL;
    g_http_responses[handle].in_use = false;
    rc = 0;
  }
  pthread_mutex_unlock(&g_http_resp_mutex);
  return rc;
}
