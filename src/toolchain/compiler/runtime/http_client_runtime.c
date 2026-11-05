#include "http_client_internal.h"
#include "http_shared.h"
#include "io_error.h"
#include "socket_safety.h"
#include "string_runtime.h"

#include <errno.h>
#include <limits.h>
#include <stdbool.h>
#include <openssl/err.h>
#include <openssl/ssl.h>
#include <openssl/x509_vfy.h> // X509_VERIFY_PARAM

#ifdef _WIN32
#define osp_poll WSAPoll
#else
#include <poll.h>
#define osp_poll poll
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
  SSL_CTX *ctx;
  SSL *ssl;
  int sock;
} Transport;

// transport_close tears down the TLS session (if any) and closes the socket.
static void transport_close(const Transport *t) {
  if (t->ssl) {
    SSL_shutdown(t->ssl);
    SSL_free(t->ssl);
  }
  if (t->ctx) {
    SSL_CTX_free(t->ctx);
  }
  if (t->sock < 0) {
    return;
  }
#ifdef _WIN32
  closesocket(t->sock);
#else
  close(t->sock);
#endif
}

// transport_send / transport_recv route I/O over TLS when ssl is set, otherwise
// over the bare socket, so http_perform speaks both http:// and https://.
static ssize_t transport_send(SSL *ssl, int sock, const char *buf, int len) {
  if (ssl) {
    return SSL_write(ssl, buf, len);
  }
  return osp_socket_send(sock, buf, (size_t)len);
}

// Writes all of `buf`, however many short writes the transport makes of it.
static bool transport_send_all(SSL *ssl, int sock, const char *buf, size_t len) {
  size_t sent = 0;
  while (sent < len) {
    size_t left = len - sent;
    int block = left > (size_t)INT_MAX ? INT_MAX : (int)left;
    ssize_t n = transport_send(ssl, sock, buf + sent, block);
    if (n <= 0) {
      return false;
    }
    sent += (size_t)n;
  }
  return true;
}

static ssize_t transport_recv(SSL *ssl, int sock, char *buf, int cap) {
  if (ssl) {
    // SSL_get_error reads this queue, and OpenSSL requires it empty before SSL_read.
    // No test can make this line matter: SSL_connect clears the queue, and a failed
    // SSL_write returns before any read. It stays because the contract is OpenSSL's.
    ERR_clear_error();
    return SSL_read(ssl, buf, cap);
  }
  return recv(sock, buf, (size_t)cap, 0);
}

// Whether a read that returned n, and no bytes, was the peer closing in good order.
// Over TLS that takes a close_notify: a bare TCP close can be forged on the wire.
static bool transport_closed(SSL *ssl, ssize_t n) {
  return ssl ? SSL_get_error(ssl, (int)n) == SSL_ERROR_ZERO_RETURN : n == 0;
}

// What a read that returned n, and no bytes, is waiting for, or 0 if it has ended.
static short transport_wants(SSL *ssl, ssize_t n) {
  if (ssl) {
    int error = SSL_get_error(ssl, (int)n);
    return error == SSL_ERROR_WANT_READ ? POLLIN
         : error == SSL_ERROR_WANT_WRITE ? POLLOUT : 0;
  }
  bool retry = n < 0 && (errno == EINTR || osp_socket_timed_out(osp_socket_error()));
  return retry ? POLLIN : 0;
}

// Reads return at once from here on, so that the wait is this code's to bound: a
// blocking SSL_read stays inside a dripped TLS record for as long as it drips.
static bool transport_read_without_blocking(int sock) {
#ifdef _WIN32
  u_long enabled = 1;
  return ioctlsocket(sock, FIONBIO, &enabled) == 0;
#else
  int flags = fcntl(sock, F_GETFL, 0);
  return flags >= 0 && fcntl(sock, F_SETFL, flags | O_NONBLOCK) == 0;
#endif
}

// tls_client_ctx builds the verified client context [HTTP-STATUS-CLIENT]:
// TLS 1.2 floor, mandatory peer verification (SSL_VERIFY_PEER) and the
// platform trust store -- which honours SSL_CERT_FILE/SSL_CERT_DIR. A trust
// store that cannot be loaded is an error, never a silent downgrade to
// unauthenticated TLS. Returns NULL when any prerequisite fails.
static SSL_CTX *tls_client_ctx(void) {
  SSL_CTX *ctx = SSL_CTX_new(TLS_client_method());
  if (!ctx) {
    return NULL;
  }
  SSL_CTX_set_verify(ctx, SSL_VERIFY_PEER, NULL);
  SSL_CTX_set_verify_depth(ctx, 9);
  // The protocol floor is load-bearing: continuing without it would let a
  // peer negotiate TLS 1.0/1.1, so a failure must refuse the context.
  if (SSL_CTX_set_min_proto_version(ctx, TLS1_2_VERSION) != 1) {
    SSL_CTX_free(ctx);
    return NULL;
  }
  if (SSL_CTX_set_default_verify_paths(ctx) != 1) {
    SSL_CTX_free(ctx);
    return NULL;
  }
  return ctx;
}

// Binds the expected peer identity to the session: DNS names via host check,
// IPv4 literals via IP check -- so a trusted CA's certificate for the WRONG
// host is rejected too. Returns 1 on success.
static int tls_expect_peer(SSL *ssl, const char *host) {
  struct in_addr addr;
  X509_VERIFY_PARAM *param = SSL_get0_param(ssl);
  return inet_pton(AF_INET, host, &addr) == 1
             ? X509_VERIFY_PARAM_set1_ip(param, (unsigned char *)&addr, 4)
             : X509_VERIFY_PARAM_set1_host(param, host, 0);
}

typedef enum { TURN_READY, TURN_TIMED_OUT, TURN_OVERDUE } TurnWait;

// Waits for the socket, up to the client's timeout (none when 0) and the whole read's
// deadline. An error from poll itself ends the read as a timeout does.
static TurnWait wait_turn(int sock, short events, int timeout_ms, int64_t deadline_ms) {
  for (;;) {
    int64_t left = deadline_ms - (int64_t)osp_monotonic_ms();
    if (left <= 0) {
      return TURN_OVERDUE;
    }
    bool bound_by_deadline = timeout_ms <= 0 || left <= timeout_ms;
    int wait_ms = !bound_by_deadline ? timeout_ms : left > INT_MAX ? INT_MAX : (int)left;
    struct pollfd watch = {.fd = sock, .events = events};
    int ready = osp_poll(&watch, 1, wait_ms);
    if (ready > 0) {
      return TURN_READY;
    }
    if (ready < 0 ? errno != EINTR : !bound_by_deadline) {
      return TURN_TIMED_OUT;
    }
  }
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
static int64_t read_turn(const Transport *t, ResponseRead *r, int timeout_ms, int64_t deadline_ms) {
  if (!response_room(r)) {
    return -9;
  }
  ssize_t n = transport_recv(t->ssl, t->sock, r->buf + r->len, CHUNK_SIZE);
  if (n > 0) {
    return response_took(r, (size_t)n);
  }
  short wanted = transport_wants(t->ssl, n);
  if (!wanted) {
    r->closed = transport_closed(t->ssl, n);
    r->done = true;
    return 0;
  }
  TurnWait waited = wait_turn(t->sock, wanted, timeout_ms, deadline_ms);
  if (waited == TURN_OVERDUE) {
    osp_io_error_text("http response", "read", "over the time limit");
    return -16;
  }
  r->done = waited == TURN_TIMED_OUT;
  return 0;
}

// Reads the response into a freshly allocated, NUL-terminated buffer, until its
// framing ends, the peer closes, a read times out, or the size limit or the whole
// read's time limit is passed. Implements [HTTP-RESPONSE-FRAMING].
static int64_t read_response(const Transport *t, int timeout_ms, char **out_raw,
                             size_t *out_len, bool *out_closed) {
  int64_t bound_ms = timeout_ms > 0 ? (int64_t)timeout_ms * HTTP_READ_BOUND_FACTOR
                                    : HTTP_READ_BOUND_UNTIMED_MS;
  int64_t deadline_ms = (int64_t)osp_monotonic_ms() + bound_ms;
  ResponseRead r = {.buf = malloc(MAX_HTTP_BUFFER), .cap = MAX_HTTP_BUFFER};
  if (!r.buf) {
    return -9;
  }
  int64_t rc = 0;
  while (rc == 0 && !r.done) {
    rc = read_turn(t, &r, timeout_ms, deadline_ms);
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

// Connects a socket to the client's host. Returns 0, or a negative code.
static int64_t transport_connect(const HttpClient *client, Transport *t) {
  t->sock = socket(AF_INET, SOCK_STREAM, 0);
  if (t->sock < 0) {
    return -3;
  }
  osp_socket_deadline(t->sock, client->timeout);
  (void)osp_socket_configure_no_sigpipe(t->sock); // TLS writes reach the socket too
  struct sockaddr_in server_addr;
  memset(&server_addr, 0, sizeof(server_addr));
  server_addr.sin_family = AF_INET;
  server_addr.sin_port = htons(client->port);
  if (!osp_socket_resolve_ipv4(client->host, &server_addr.sin_addr)) {
    return -4;
  }
  return connect(t->sock, (struct sockaddr *)&server_addr, sizeof(server_addr)) < 0 ? -5 : 0;
}

// Establishes the TLS session of an https:// client. SNI (the host name) is mandatory
// for virtual-hosted endpoints such as api.github.com, and every session verifies the
// peer chain AND that the certificate matches the host/IP we dialed
// [HTTP-STATUS-CLIENT]. Returns 0, or a negative code.
static int64_t transport_secure(const HttpClient *client, Transport *t) {
  t->ctx = tls_client_ctx();
  if (!t->ctx) {
    return -13; // trust store/verification prerequisites unavailable
  }
  t->ssl = SSL_new(t->ctx);
  if (!t->ssl) {
    return -11;
  }
  SSL_set_fd(t->ssl, t->sock);
  SSL_set_tlsext_host_name(t->ssl, client->host);
  if (tls_expect_peer(t->ssl, client->host) != 1) {
    return -11;
  }
  if (SSL_connect(t->ssl) != 1 || SSL_get_verify_result(t->ssl) != X509_V_OK) {
    return -12; // handshake or certificate verification failed
  }
  return 0;
}

// Sends the request. Only the head is formatted: the body is any size and goes out
// after it unchanged, so a large request can neither truncate nor read past the buffer.
// The Host header carries the bare host name (no :port) - servers like GitHub reject
// "Host: api.github.com:443" with 400. Returns 0, or -6.
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
  return transport_send_all(t->ssl, t->sock, head, (size_t)head_len) &&
                 transport_send_all(t->ssl, t->sock, body, body_len)
             ? 0
             : -6;
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
  Transport t = {.ctx = NULL, .ssl = NULL, .sock = -1};
  int64_t rc = transport_connect(client, &t);
  if (rc == 0 && client->is_https) {
    rc = transport_secure(client, &t);
  }
  if (rc == 0) {
    rc = send_request(client, &t, method, path, headers, body);
  }
  if (rc == 0) {
    rc = transport_read_without_blocking(t.sock)
             ? read_response(&t, client->timeout, out_raw, out_len, out_closed)
             : -3;
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
