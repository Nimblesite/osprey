#include "http_client_internal.h"
#include "http_shared.h"
#include "io_error.h"
#include "string_runtime.h"

#include <limits.h>
#include <stdbool.h>

#ifndef _WIN32
#include <poll.h>
#endif

// HTTP client runtime.
//
// Two request surfaces share one transport helper (http_perform):
//   * Status-only API  (http_get/post/put/delete, http_request) returns the
//     numeric status code [HTTP-STATUS-CLIENT].
//   * Response-handle API (http_*_response + http_response_*) retains the full
//     response body and headers in a heap slot and returns an opaque handle so
//     Osprey can read the body. Implements [HTTP-RESPONSE-HANDLE].

// A client handle indexes clients[] directly, so EVERY entry point must prove
// the handle is inside the table first: get_next_id() is a process-global
// counter shared with servers and sockets, and a caller-supplied handle is
// arbitrary. Handle 0 is never issued [HTTP-STATUS-CLIENT].
static bool valid_client_id(int64_t client_id) {
  return client_id > 0 && client_id < MAX_CLIENTS;
}

// Create HTTP client - returns client_id or negative error
int64_t http_create_client(char *base_url, int64_t timeout) {
  if (!base_url) {
    return -1;
  }

  if (timeout < 0) {
    return -2;
  }

  int64_t id = get_next_id();
  if (!valid_client_id(id)) {
    return -5; // handle table exhausted: refuse rather than write past its end
  }
  HttpClient *client = malloc(sizeof(HttpClient));
  if (!client) {
    return -3;
  }

  client->id = id;
  client->base_url = strdup(base_url);
  client->timeout = timeout > INT_MAX ? INT_MAX : (int)timeout;
  client->is_persistent = false;
  client->is_https = (strncmp(base_url, "https://", 8) == 0) ||
                     (strncmp(base_url, "wss://", 6) == 0);

  // Parse base URL
  char *path;
  if (parse_url(base_url, &client->host, &client->port, &path) != 0) {
    free(client->base_url);
    free(client);
    return -4;
  }
  free(path); // We only need host and port for client

  pthread_mutex_lock(&runtime_mutex);
  clients[id] = client;
  pthread_mutex_unlock(&runtime_mutex);

  return id;
}

typedef struct {
  char *buf;
  size_t cap;
  size_t len;
  bool closed; // the peer closed in good order
  bool done;
  HttpFrameScan scan;
} ResponseRead;

static bool response_room(ResponseRead *r) {
  if (r->len + CHUNK_SIZE + 1 <= r->cap) {
    return true;
  }
  char *grown = realloc(r->buf, r->cap * 2);
  if (grown) {
    r->buf = grown;
    r->cap *= 2;
  }
  return grown != NULL;
}

// Takes in n bytes just read. Returns 0, or -15 when they pass the size limit.
static int64_t response_took(ResponseRead *r, size_t n) {
  r->len += n;
  if (r->len > HTTP_MAX_RESPONSE_BYTES) {
    osp_io_error_text("http response", "size", "over the limit");
    return -15;
  }
  r->buf[r->len] = '\0';
  r->done = http_frame_complete(&r->scan, r->buf, r->len, &r->len);
  r->buf[r->len] = '\0'; // what follows the framing isn't part of the response
  return 0;
}

// One read and, if it has to, one wait. Returns 0, or a negative code.
static int64_t read_turn(const Transport *t, ResponseRead *r) {
  if (!response_room(r)) {
    return -9;
  }
  ssize_t n = transport_recv(t, r->buf + r->len, CHUNK_SIZE);
  if (n > 0) {
    return response_took(r, (size_t)n);
  }
  short wanted = transport_wants(t, n, POLLIN);
  if (!wanted) {
    r->closed = transport_closed(t, n);
    r->done = true;
    return 0;
  }
  TurnWait waited = transport_wait(t, wanted);
  if (waited == TURN_OVERDUE) {
    osp_io_error_text("http response", "read", "over the time limit");
    return -16;
  }
  r->done = waited == TURN_TIMED_OUT;
  return 0;
}

// Reads the response into a freshly allocated, NUL-terminated buffer, until its
// framing ends, the peer closes, a read times out, or the size limit or the whole
// request's time limit is passed. Implements [HTTP-RESPONSE-FRAMING].
static int64_t read_response(const Transport *t, char **out_raw, size_t *out_len, bool *out_closed) {
  ResponseRead r = {.buf = malloc(MAX_HTTP_BUFFER), .cap = MAX_HTTP_BUFFER};
  if (!r.buf) {
    return -9;
  }
  int64_t rc = 0;
  while (rc == 0 && !r.done) {
    rc = read_turn(t, &r);
  }
  if (rc == 0 && r.len == 0) {
    rc = -7;
  }
  if (rc != 0) {
    free(r.buf);
    return rc;
  }
  *out_raw = r.buf;
  *out_len = r.len;
  *out_closed = r.closed;
  return 0;
}

// Sends the request. Only the head is formatted: the body is any size and goes out
// after it unchanged, so a large request can neither truncate nor read past the buffer.
// The Host header carries the bare host name (no :port) - servers like GitHub reject
// "Host: api.github.com:443" with 400. Returns 0, -6, or -16 past the deadline.
static int64_t send_request(const HttpClient *client, const Transport *t, int64_t method,
                            const char *path, const char *headers, const char *body) {
  char head[MAX_HTTP_BUFFER];
  const char *method_str = http_method_to_string((HttpMethod)method);
  size_t body_len = (size_t)osp_strlen(body);
  const char *extra = headers ? headers : "";
  size_t extra_len = strlen(extra);
  bool ended = extra_len == 0 || (extra_len >= 2 && strcmp(extra + extra_len - 2, "\r\n") == 0);
  const char *extra_end = ended ? "" : "\r\n";
  int head_len = body_len > 0
      ? snprintf(head, sizeof(head),
                 "%s %s HTTP/1.1\r\nHost: %s\r\nConnection: close\r\n%s%sContent-Length: %zu\r\n\r\n",
                 method_str, path, client->host, extra, extra_end, body_len)
      : snprintf(head, sizeof(head),
                 "%s %s HTTP/1.1\r\nHost: %s\r\nConnection: close\r\n%s%s\r\n",
                 method_str, path, client->host, extra, extra_end);
  if (head_len < 0 || (size_t)head_len >= sizeof(head)) {
    return -6; // a head that does not fit is refused, never sent truncated
  }
  int64_t rc = transport_send_all(t, head, (size_t)head_len);
  return rc == 0 ? transport_send_all(t, body, body_len) : rc;
}

// http_perform connects, sends the request and reads the response. For https:// clients
// the transport is wrapped in TLS. Returns 0 on success with *out_raw / *out_len set
// (caller frees *out_raw), or a negative error code.
int64_t http_perform(int64_t client_id, int64_t method, char *path,
                     char *headers, char *body, char **out_raw,
                     size_t *out_len, bool *out_closed) {
  if (!valid_client_id(client_id)) {
    return -1;
  }
  pthread_mutex_lock(&runtime_mutex);
  HttpClient *client = clients[client_id];
  pthread_mutex_unlock(&runtime_mutex);
  if (!client) {
    return -1;
  }
  if (!path) {
    return -2;
  }
  Transport t = transport_open(client->timeout);
  int64_t rc = transport_connect(&t, client->host, client->port);
  if (rc == 0 && client->is_https) {
    rc = transport_secure(&t, client->host);
  }
  if (rc == 0) {
    rc = send_request(client, &t, method, path, headers, body);
  }
  if (rc == 0) {
    rc = read_response(&t, out_raw, out_len, out_closed);
  }
  transport_close(&t);
  return rc;
}

// Shared status-only request implementation.
static int64_t http_request(int64_t client_id, int64_t method, char *path,
                            char *headers, char *body) {
  char *raw = NULL;
  size_t len = 0;
  bool closed = false;
  int64_t rc = http_perform(client_id, method, path, headers, body, &raw, &len, &closed);
  if (rc != 0) {
    return rc;
  }
  int64_t status = http_parse_status_line(http_final_response(raw));
  free(raw);
  return status;
}

// Close HTTP client - returns 0 on success
int64_t http_close_client(int64_t client_id) {
  if (!valid_client_id(client_id)) {
    return -1;
  }
  pthread_mutex_lock(&runtime_mutex);
  HttpClient *client = clients[client_id];
  if (client) {
    clients[client_id] = NULL;
    free(client->base_url);
    free(client->host);
    free(client);
  }
  pthread_mutex_unlock(&runtime_mutex);

  return 0;
}

// Convenience functions for specific HTTP methods (status-only API)
int64_t http_get(int64_t client_id, char *path, char *headers) {
  return http_request(client_id, HTTP_GET, path, headers, NULL);
}

int64_t http_post(int64_t client_id, char *path, char *body, char *headers) {
  return http_request(client_id, HTTP_POST, path, headers, body);
}

int64_t http_put(int64_t client_id, char *path, char *body, char *headers) {
  return http_request(client_id, HTTP_PUT, path, headers, body);
}

int64_t http_delete(int64_t client_id, char *path, char *headers) {
  return http_request(client_id, HTTP_DELETE, path, headers, NULL);
}
