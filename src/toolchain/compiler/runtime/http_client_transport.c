// One request's connection, over http:// or https://, and the deadline the whole
// request keeps: connecting, the TLS handshake, sending and reading
// [HTTP-RESPONSE-FRAMING]. From the connect on, nothing blocks: every wait is
// a poll bounded by the client's timeout and by that deadline, so a peer that
// drips its handshake or reads a request a little at a time cannot hold the
// client past it.
#include "http_client_internal.h"
#include "http_shared.h"
#include "io_error.h"
#include "socket_safety.h"

#include <errno.h>
#include <limits.h>
#include <openssl/err.h>
#include <openssl/x509_vfy.h> // X509_VERIFY_PARAM

#ifdef _WIN32
#define osp_poll WSAPoll
#else
#include <fcntl.h>
#include <poll.h>
#define osp_poll poll
#endif

Transport transport_open(int timeout_ms) {
  int64_t bound_ms = timeout_ms > 0 ? (int64_t)timeout_ms * HTTP_READ_BOUND_FACTOR
                                    : HTTP_READ_BOUND_UNTIMED_MS;
  return (Transport){.ctx = NULL, .ssl = NULL, .sock = -1, .timeout_ms = timeout_ms,
                     .deadline_ms = (int64_t)osp_monotonic_ms() + bound_ms};
}

// Tears down the TLS session (if any) and closes the socket.
void transport_close(const Transport *t) {
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

// The request ran past its deadline during `stage`: code -16.
static int64_t transport_overdue(const char *stage) {
  osp_io_error_text("http request", stage, "over the time limit");
  return -16;
}

// Waits for the socket, up to the client's timeout (none when 0) and the whole
// request's deadline. An error from poll itself ends the wait as a timeout does.
TurnWait transport_wait(const Transport *t, short events) {
  for (;;) {
    int64_t left = t->deadline_ms - (int64_t)osp_monotonic_ms();
    if (left <= 0) {
      return TURN_OVERDUE;
    }
    bool bound_by_deadline = t->timeout_ms <= 0 || left <= t->timeout_ms;
    int wait_ms = !bound_by_deadline ? t->timeout_ms : left > INT_MAX ? INT_MAX : (int)left;
    struct pollfd watch = {.fd = t->sock, .events = events};
    int ready = osp_poll(&watch, 1, wait_ms);
    if (ready > 0) {
      return TURN_READY;
    }
    if (ready < 0 ? errno != EINTR : !bound_by_deadline) {
      return TURN_TIMED_OUT;
    }
  }
}

// What an I/O call that returned n, and no bytes, is waiting for, or 0 if it has
// ended. A bare socket that would block waits for `direction`.
short transport_wants(const Transport *t, ssize_t n, short direction) {
  if (t->ssl) {
    int error = SSL_get_error(t->ssl, (int)n);
    return error == SSL_ERROR_WANT_READ ? POLLIN : error == SSL_ERROR_WANT_WRITE ? POLLOUT : 0;
  }
  bool retry = n < 0 && (errno == EINTR || osp_socket_timed_out(osp_socket_error()));
  return retry ? direction : 0;
}

// I/O returns at once from the connect on, so that every wait is this code's to
// bound: a blocking SSL_read or SSL_connect stays inside a dripped TLS record for
// as long as it drips, and a blocking send for as long as the peer keeps reading.
static bool transport_without_blocking(int sock) {
#ifdef _WIN32
  u_long enabled = 1;
  return ioctlsocket(sock, FIONBIO, &enabled) == 0;
#else
  int flags = fcntl(sock, F_GETFL, 0);
  return flags >= 0 && fcntl(sock, F_SETFL, flags | O_NONBLOCK) == 0;
#endif
}

// Connects a socket to `host`. Returns 0, or a negative code.
int64_t transport_connect(Transport *t, const char *host, int port) {
  t->sock = socket(AF_INET, SOCK_STREAM, 0);
  if (t->sock < 0) {
    return -3;
  }
  osp_socket_deadline(t->sock, t->timeout_ms);
  (void)osp_socket_configure_no_sigpipe(t->sock); // TLS writes reach the socket too
  struct sockaddr_in server_addr;
  memset(&server_addr, 0, sizeof(server_addr));
  server_addr.sin_family = AF_INET;
  server_addr.sin_port = htons((uint16_t)port);
  if (!osp_socket_resolve_ipv4(host, &server_addr.sin_addr)) {
    return -4;
  }
  if (connect(t->sock, (struct sockaddr *)&server_addr, sizeof(server_addr)) < 0) {
    return -5;
  }
  return transport_without_blocking(t->sock) ? 0 : -3;
}

// The verified client context [HTTP-STATUS-CLIENT]: TLS 1.2 floor, mandatory
// peer verification (SSL_VERIFY_PEER) and the platform trust store, which
// honours SSL_CERT_FILE/SSL_CERT_DIR. A trust store that cannot be loaded is an
// error, never a silent downgrade to unauthenticated TLS. NULL when any fails.
static SSL_CTX *tls_client_ctx(void) {
  SSL_CTX *ctx = SSL_CTX_new(TLS_client_method());
  if (!ctx) {
    return NULL;
  }
  SSL_CTX_set_verify(ctx, SSL_VERIFY_PEER, NULL);
  SSL_CTX_set_verify_depth(ctx, 9);
  // The protocol floor is load-bearing: continuing without it would let a
  // peer negotiate TLS 1.0/1.1, so a failure must refuse the context.
  if (SSL_CTX_set_min_proto_version(ctx, TLS1_2_VERSION) != 1 ||
      SSL_CTX_set_default_verify_paths(ctx) != 1) {
    SSL_CTX_free(ctx);
    return NULL;
  }
  return ctx;
}

// Binds the expected peer identity to the session: DNS names via host check,
// IPv4 literals via IP check, so a trusted CA's certificate for the WRONG host
// is rejected too. Returns 1 on success.
static int tls_expect_peer(SSL *ssl, const char *host) {
  struct in_addr addr;
  X509_VERIFY_PARAM *param = SSL_get0_param(ssl);
  return inet_pton(AF_INET, host, &addr) == 1
             ? X509_VERIFY_PARAM_set1_ip(param, (unsigned char *)&addr, 4)
             : X509_VERIFY_PARAM_set1_host(param, host, 0);
}

// The handshake, one step per wait, so a peer that drips it ends at the deadline.
static int64_t transport_handshake(const Transport *t) {
  for (;;) {
    ERR_clear_error();
    int done = SSL_connect(t->ssl);
    if (done == 1) {
      return SSL_get_verify_result(t->ssl) == X509_V_OK ? 0 : -12;
    }
    short wanted = transport_wants(t, done, POLLIN);
    TurnWait waited = wanted ? transport_wait(t, wanted) : TURN_TIMED_OUT;
    if (waited != TURN_READY) {
      return waited == TURN_OVERDUE ? transport_overdue("handshake") : -12;
    }
  }
}

// Establishes the TLS session of an https:// client. SNI (the host name) is
// mandatory for virtual-hosted endpoints such as api.github.com, and every
// session verifies the peer chain AND that the certificate matches the host/IP
// dialed [HTTP-STATUS-CLIENT]. `host` is not const because OpenSSL's SNI macro
// takes it as void *. Returns 0, or a negative code.
int64_t transport_secure(Transport *t, char *host) {
  t->ctx = tls_client_ctx();
  if (!t->ctx) {
    return -13; // trust store/verification prerequisites unavailable
  }
  t->ssl = SSL_new(t->ctx);
  if (!t->ssl) {
    return -11;
  }
  SSL_set_fd(t->ssl, t->sock);
  SSL_set_tlsext_host_name(t->ssl, host);
  return tls_expect_peer(t->ssl, host) == 1 ? transport_handshake(t) : -11;
}

static ssize_t transport_send(const Transport *t, const char *buf, int len) {
  return t->ssl ? SSL_write(t->ssl, buf, len) : osp_socket_send(t->sock, buf, (size_t)len);
}

// Writes all of `buf`, however many short writes and waits it takes. Returns 0,
// -6 when the peer fails or stalls past the timeout, or -16 past the deadline.
int64_t transport_send_all(const Transport *t, const char *buf, size_t len) {
  for (size_t sent = 0; sent < len;) {
    size_t left = len - sent;
    ssize_t n = transport_send(t, buf + sent, left > (size_t)INT_MAX ? INT_MAX : (int)left);
    if (n > 0) {
      sent += (size_t)n;
      continue;
    }
    short wanted = transport_wants(t, n, POLLOUT);
    TurnWait waited = wanted ? transport_wait(t, wanted) : TURN_TIMED_OUT;
    if (waited != TURN_READY) {
      return waited == TURN_OVERDUE ? transport_overdue("send") : -6;
    }
  }
  return 0;
}

ssize_t transport_recv(const Transport *t, char *buf, int cap) {
  if (t->ssl) {
    // SSL_get_error reads this queue, and OpenSSL requires it empty before SSL_read.
    ERR_clear_error();
    return SSL_read(t->ssl, buf, cap);
  }
  return recv(t->sock, buf, (size_t)cap, 0);
}

// Whether a read that returned n, and no bytes, was the peer closing in good order.
// Over TLS that takes a close_notify: a bare TCP close can be forged on the wire.
bool transport_closed(const Transport *t, ssize_t n) {
  return t->ssl ? SSL_get_error(t->ssl, (int)n) == SSL_ERROR_ZERO_RETURN : n == 0;
}
