// A raw TLS client session for `extern fn` callers [FFI-TLS-CLIENT].
//
// Osprey strings are NUL-terminated, so a byte stream cannot cross the C
// boundary as a string. Every byte here crosses as lowercase hex text: two
// characters per byte, no separator, built and read by the Osprey side. The
// unit is deliberately protocol-agnostic: it dials, writes, reads an exact
// count with a deadline, reports the stream's state and closes. Framing,
// message formats and retries are the caller's.
//
// The peer is NOT verified [FFI-TLS-UNVERIFIED]: this surface exists for
// devices on the local network that present self-signed certificates (a
// Chromecast's port 8009 is the motivating case). It is not a web client;
// `httpCreateClient` keeps its mandatory verification for https://.
//
// A read that runs out of time keeps the bytes it already has, so the next
// read of the same count resumes rather than losing part of a frame.

#include "http_shared.h"
#include "memory_hooks.h"
#include "socket_safety.h"

#include <openssl/err.h>
#include <openssl/ssl.h>

enum { TLS_SESSIONS = 64, TLS_MAX_READ = 1 << 24 };
enum { TLS_OK = 0, TLS_TIMEOUT = 1, TLS_CLOSED = 2, TLS_FAILED = 3 };

typedef struct {
  SSL_CTX *ctx;
  SSL *ssl;
  int fd;
  int64_t status;
  unsigned char *pending;
  int64_t pending_len;
  int64_t pending_cap;
} TlsSession;

static TlsSession tls_sessions[TLS_SESSIONS];
static pthread_mutex_t tls_mutex = PTHREAD_MUTEX_INITIALIZER;

static TlsSession *tls_session(int64_t handle) {
  if (handle < 1 || handle > TLS_SESSIONS) {
    return NULL;
  }
  TlsSession *session = &tls_sessions[handle - 1];
  return session->ssl ? session : NULL;
}

static void close_fd(int fd) {
  if (fd >= 0) {
    close(fd);
  }
}

// A connected TCP socket to host:port, or a negative reason.
static int tcp_dial(const char *host, int64_t port, int64_t timeout_ms) {
  if (!host || port < 1 || port > 65535) {
    return -1;
  }
  osprey_wsa_init();
  struct hostent *server = gethostbyname(host);
  if (!server || server->h_addrtype != AF_INET) {
    return -2;
  }
  int fd = socket(AF_INET, SOCK_STREAM, 0);
  if (fd < 0) {
    return -3;
  }
  osp_socket_deadline(fd, timeout_ms > 0 ? timeout_ms : 10000);
  osp_socket_configure_no_sigpipe(fd);
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof addr);
  addr.sin_family = AF_INET;
  addr.sin_port = htons((uint16_t)port);
  memcpy(&addr.sin_addr.s_addr, server->h_addr_list[0], (size_t)server->h_length);
  if (connect(fd, (struct sockaddr *)&addr, sizeof addr) < 0) {
    close_fd(fd);
    return -4;
  }
  return fd;
}

// An unverified TLS 1.2+ client context [FFI-TLS-UNVERIFIED].
static SSL_CTX *tls_context(void) {
  SSL_CTX *ctx = SSL_CTX_new(TLS_client_method());
  if (!ctx) {
    return NULL;
  }
  SSL_CTX_set_verify(ctx, SSL_VERIFY_NONE, NULL);
  if (SSL_CTX_set_min_proto_version(ctx, TLS1_2_VERSION) != 1) {
    SSL_CTX_free(ctx);
    return NULL;
  }
  return ctx;
}

static int64_t tls_free_slot(void) {
  for (int64_t i = 0; i < TLS_SESSIONS; i++) {
    if (!tls_sessions[i].ssl) {
      return i;
    }
  }
  return -1;
}

static void tls_release(TlsSession *session) {
  if (session->ssl) {
    SSL_shutdown(session->ssl);
    SSL_free(session->ssl);
  }
  if (session->ctx) {
    SSL_CTX_free(session->ctx);
  }
  close_fd(session->fd);
  free(session->pending);
  memset(session, 0, sizeof *session);
  session->fd = -1;
}

// osprey_tls_connect dials host:port and completes the handshake. Returns a
// handle >= 1, or a negative reason: -1 bad argument, -2 unresolvable host,
// -3 no socket, -4 connect refused or timed out, -5 no session slot,
// -6 TLS context unavailable, -7 handshake failed.
int64_t osprey_tls_connect(char *host, int64_t port, int64_t timeout_ms) {
  int fd = tcp_dial(host, port, timeout_ms);
  if (fd < 0) {
    return fd;
  }
  pthread_mutex_lock(&tls_mutex);
  int64_t slot = tls_free_slot();
  if (slot < 0) {
    pthread_mutex_unlock(&tls_mutex);
    close_fd(fd);
    return -5;
  }
  TlsSession *session = &tls_sessions[slot];
  memset(session, 0, sizeof *session);
  session->fd = fd;
  session->ctx = tls_context();
  session->ssl = session->ctx ? SSL_new(session->ctx) : NULL;
  if (!session->ssl) {
    tls_release(session);
    pthread_mutex_unlock(&tls_mutex);
    return -6;
  }
  SSL_set_fd(session->ssl, fd);
  if (SSL_connect(session->ssl) != 1) {
    tls_release(session);
    pthread_mutex_unlock(&tls_mutex);
    return -7;
  }
  pthread_mutex_unlock(&tls_mutex);
  return slot + 1;
}

static int hex_nibble(char c) {
  if (c >= '0' && c <= '9') {
    return c - '0';
  }
  if (c >= 'a' && c <= 'f') {
    return c - 'a' + 10;
  }
  if (c >= 'A' && c <= 'F') {
    return c - 'A' + 10;
  }
  return -1;
}

// The bytes a hex text spells, or NULL for odd length or a non-hex character.
static unsigned char *hex_bytes(const char *hex, size_t *length) {
  size_t chars = strlen(hex);
  if (chars % 2 != 0) {
    return NULL;
  }
  unsigned char *bytes = malloc(chars / 2 + 1);
  if (!bytes) {
    return NULL;
  }
  for (size_t i = 0; i < chars; i += 2) {
    int high = hex_nibble(hex[i]);
    int low = hex_nibble(hex[i + 1]);
    if (high < 0 || low < 0) {
      free(bytes);
      return NULL;
    }
    bytes[i / 2] = (unsigned char)(high * 16 + low);
  }
  *length = chars / 2;
  return bytes;
}

// Hex text for `length` bytes, in the Osprey allocator so the string is an
// ordinary runtime value.
static char *bytes_hex(const unsigned char *bytes, size_t length) {
  static const char digits[] = "0123456789abcdef";
  char *hex = osp_alloc_tagged((int64_t)(length * 2 + 1), 0);
  if (!hex) {
    return NULL;
  }
  for (size_t i = 0; i < length; i++) {
    hex[i * 2] = digits[bytes[i] >> 4];
    hex[i * 2 + 1] = digits[bytes[i] & 15];
  }
  hex[length * 2] = '\0';
  return hex;
}

static int64_t ssl_write_all(SSL *ssl, const unsigned char *bytes, size_t length) {
  size_t done = 0;
  while (done < length) {
    int n = SSL_write(ssl, bytes + done, (int)(length - done));
    if (n <= 0) {
      return -3;
    }
    done += (size_t)n;
  }
  return 0;
}

// osprey_tls_send_hex writes the bytes `hex` spells. Returns 0, or -1 for an
// unknown handle, -2 for text that is not hex, -3 when the write failed.
int64_t osprey_tls_send_hex(int64_t handle, char *hex) {
  pthread_mutex_lock(&tls_mutex);
  TlsSession *session = tls_session(handle);
  if (!session || !hex) {
    pthread_mutex_unlock(&tls_mutex);
    return -1;
  }
  size_t length = 0;
  unsigned char *bytes = hex_bytes(hex, &length);
  if (!bytes) {
    pthread_mutex_unlock(&tls_mutex);
    return -2;
  }
  int64_t result = ssl_write_all(session->ssl, bytes, length);
  session->status = result == 0 ? TLS_OK : TLS_FAILED;
  free(bytes);
  pthread_mutex_unlock(&tls_mutex);
  return result;
}

// Makes room for `count` pending bytes; a new count discards an old partial.
static bool pending_prepare(TlsSession *session, int64_t count) {
  if (session->pending_cap == count) {
    return true;
  }
  free(session->pending);
  session->pending = malloc((size_t)count);
  session->pending_len = 0;
  session->pending_cap = session->pending ? count : 0;
  return session->pending != NULL;
}

// Classifies a failed SSL_read: a socket deadline is a timeout, a clean or
// abrupt close is closed, anything else failed. `saved` is the socket error
// as the read left it, taken before SSL_get_error can disturb it: a blocking
// socket whose deadline passed wants more input (SSL_ERROR_WANT_READ) or
// reports a timeout, and neither means the peer went away.
static int64_t read_outcome(SSL *ssl, int n, int saved) {
  int reason = SSL_get_error(ssl, n);
  if (reason == SSL_ERROR_ZERO_RETURN) {
    return TLS_CLOSED;
  }
  if (reason == SSL_ERROR_WANT_READ || osp_socket_timed_out(saved)) {
    return TLS_TIMEOUT;
  }
  if (reason == SSL_ERROR_SYSCALL) {
    return n == 0 || saved == 0 || osp_socket_reset(saved) ? TLS_CLOSED : TLS_FAILED;
  }
  return TLS_FAILED;
}

// Reads until the pending buffer holds `count` bytes or the deadline passes.
static int64_t fill_pending(TlsSession *session, int64_t count) {
  while (session->pending_len < count) {
    int want = (int)(count - session->pending_len);
    osp_socket_error_clear();
    int n = SSL_read(session->ssl, session->pending + session->pending_len, want);
    int saved = osp_socket_error();
    if (n <= 0) {
      return read_outcome(session->ssl, n, saved);
    }
    session->pending_len += n;
  }
  return TLS_OK;
}

// osprey_tls_recv_hex returns exactly `count` bytes as hex, or "" when they
// did not all arrive within `timeout_ms`; `osprey_tls_status` then says why.
// Bytes read before a timeout are kept for the next call with the same count.
char *osprey_tls_recv_hex(int64_t handle, int64_t count, int64_t timeout_ms) {
  pthread_mutex_lock(&tls_mutex);
  TlsSession *session = tls_session(handle);
  if (!session || count < 1 || count > TLS_MAX_READ || !pending_prepare(session, count)) {
    if (session) {
      session->status = TLS_FAILED;
    }
    pthread_mutex_unlock(&tls_mutex);
    return bytes_hex((const unsigned char *)"", 0);
  }
  osp_socket_deadline(session->fd, timeout_ms > 0 ? timeout_ms : 10000);
  session->status = fill_pending(session, count);
  bool whole = session->status == TLS_OK;
  char *hex = bytes_hex(session->pending, whole ? (size_t)count : 0);
  if (whole) {
    session->pending_len = 0;
  }
  pthread_mutex_unlock(&tls_mutex);
  return hex;
}

// osprey_tls_status is the outcome of the last read or write on the session:
// 0 ok, 1 timed out, 2 the peer closed, 3 failed; -1 for an unknown handle.
int64_t osprey_tls_status(int64_t handle) {
  pthread_mutex_lock(&tls_mutex);
  TlsSession *session = tls_session(handle);
  int64_t status = session ? session->status : -1;
  pthread_mutex_unlock(&tls_mutex);
  return status;
}

// osprey_tls_close ends the session and frees its slot. 0, or -1 if unknown.
int64_t osprey_tls_close(int64_t handle) {
  pthread_mutex_lock(&tls_mutex);
  TlsSession *session = tls_session(handle);
  if (!session) {
    pthread_mutex_unlock(&tls_mutex);
    return -1;
  }
  tls_release(session);
  pthread_mutex_unlock(&tls_mutex);
  return 0;
}

// osprey_net_local_address is the address of the local interface that routes
// to host:port (a UDP socket is connected, never sent on), as dotted text, or
// "" when the host does not resolve. A server that must be reachable from a
// particular device binds here.
char *osprey_net_local_address(char *host, int64_t port) {
  osprey_wsa_init();
  struct hostent *server = host ? gethostbyname(host) : NULL;
  if (!server || server->h_addrtype != AF_INET || port < 1 || port > 65535) {
    return bytes_hex((const unsigned char *)"", 0);
  }
  int fd = socket(AF_INET, SOCK_DGRAM, 0);
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof addr);
  addr.sin_family = AF_INET;
  addr.sin_port = htons((uint16_t)port);
  memcpy(&addr.sin_addr.s_addr, server->h_addr_list[0], (size_t)server->h_length);
  struct sockaddr_in local;
  socklen_t local_len = sizeof local;
  char text[INET_ADDRSTRLEN] = "";
  if (fd >= 0 && connect(fd, (struct sockaddr *)&addr, sizeof addr) == 0 &&
      getsockname(fd, (struct sockaddr *)&local, &local_len) == 0) {
    inet_ntop(AF_INET, &local.sin_addr, text, sizeof text);
  }
  close_fd(fd);
  char *copy = osp_alloc_tagged((int64_t)strlen(text) + 1, 0);
  if (copy) {
    memcpy(copy, text, strlen(text) + 1);
  }
  return copy;
}
