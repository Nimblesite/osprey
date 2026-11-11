// Assertion-driven tests for the extern-callable network units: a failed
// assert aborts the binary. This suite OWNS tls_client_runtime.c and, through
// file_server_tests.c, file_server_runtime.c.
//
// Everything runs against real loopback sockets in this process: an OpenSSL
// echo server with a self-signed certificate for the TLS session, so the unit is
// asserted end to end. Spec anchors:
//   [FFI-TLS-CLIENT]      dial, hex writes, exact-count hex reads, status, close
//   [FFI-TLS-UNVERIFIED]  a self-signed peer is accepted
#include "http_shared.h"

#include <assert.h>
#include <openssl/err.h>
#include <openssl/pem.h>
#include <openssl/ssl.h>
#include <openssl/x509v3.h>
#include <signal.h>

int64_t osprey_tls_connect(char *host, int64_t port, int64_t timeout_ms);
int64_t osprey_tls_send_hex(int64_t handle, char *hex);
char *osprey_tls_recv_hex(int64_t handle, int64_t count, int64_t timeout_ms);
int64_t osprey_tls_status(int64_t handle);
int64_t osprey_tls_close(int64_t handle);
char *osprey_net_local_address(char *host, int64_t port);
void run_file_server_tests(void); // file_server_tests.c

enum { ECHO_PORT = 18092, ECHO_CLOSE_PORT = 18093 };

static char LOOPBACK[] = "127.0.0.1";
static char NOWHERE[] = "no-such-host.invalid";

// ---------------------------------------------------------------------------
// A self-signed certificate, generated in memory, so the suite needs no files.
// ---------------------------------------------------------------------------

static X509 *self_signed(EVP_PKEY *key) {
  X509 *x = X509_new();
  assert(x != NULL);
  ASN1_INTEGER_set(X509_get_serialNumber(x), 1);
  X509_gmtime_adj(X509_getm_notBefore(x), -3600);
  X509_gmtime_adj(X509_getm_notAfter(x), 3600);
  X509_NAME *name = X509_NAME_new();
  X509_NAME_add_entry_by_txt(name, "CN", MBSTRING_ASC, (const unsigned char *)"device", -1, -1, 0);
  X509_set_subject_name(x, name);
  X509_set_issuer_name(x, name);
  X509_NAME_free(name);
  X509_set_pubkey(x, key);
  assert(X509_sign(x, key, EVP_sha256()) != 0);
  return x;
}

// ---------------------------------------------------------------------------
// A TLS echo server: every byte read is written straight back. On
// ECHO_CLOSE_PORT the server closes after the handshake without reading.
// ---------------------------------------------------------------------------

typedef struct {
  int listen_fd;
  int port;
  bool close_immediately;
  pthread_t thread;
  SSL_CTX *ctx;
} EchoServer;

static void echo_session(SSL *ssl, bool close_immediately) {
  unsigned char buf[4096];
  while (!close_immediately) {
    int n = SSL_read(ssl, buf, sizeof buf);
    if (n <= 0) {
      return;
    }
    assert(SSL_write(ssl, buf, n) == n);
  }
}

typedef struct {
  SSL_CTX *ctx;
  int fd;
  bool close_immediately;
} EchoConnection;

static void *echo_connection_thread(void *arg) {
  EchoConnection *connection = arg;
  SSL *ssl = SSL_new(connection->ctx);
  SSL_set_fd(ssl, connection->fd);
  if (SSL_accept(ssl) == 1) {
    echo_session(ssl, connection->close_immediately);
    SSL_shutdown(ssl);
  }
  SSL_free(ssl);
  close(connection->fd);
  free(connection);
  return NULL;
}

// Every accepted connection gets its own thread, so many sessions can be
// open at once (the slot-bound test holds 64).
static void *echo_thread(void *arg) {
  EchoServer *server = arg;
  for (;;) {
    int fd = accept(server->listen_fd, NULL, NULL);
    if (fd < 0) {
      return NULL;
    }
    EchoConnection *connection = malloc(sizeof *connection);
    assert(connection != NULL);
    connection->ctx = server->ctx;
    connection->fd = fd;
    connection->close_immediately = server->close_immediately;
    pthread_t thread;
    assert(pthread_create(&thread, NULL, echo_connection_thread, connection) == 0);
    pthread_detach(thread);
  }
}

static int listen_loopback(int port) {
  int fd = socket(AF_INET, SOCK_STREAM, 0);
  assert(fd >= 0);
  int opt = 1;
  assert(setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, (const char *)&opt, sizeof opt) == 0);
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof addr);
  addr.sin_family = AF_INET;
  addr.sin_port = htons((uint16_t)port);
  addr.sin_addr.s_addr = inet_addr(LOOPBACK);
  assert(bind(fd, (struct sockaddr *)&addr, sizeof addr) == 0);
  assert(listen(fd, 64) == 0);
  return fd;
}

static void echo_start(EchoServer *server, int port, bool close_immediately, X509 *cert,
                       EVP_PKEY *key) {
  memset(server, 0, sizeof *server);
  server->port = port;
  server->close_immediately = close_immediately;
  server->ctx = SSL_CTX_new(TLS_server_method());
  assert(server->ctx != NULL);
  assert(SSL_CTX_use_certificate(server->ctx, cert) == 1);
  assert(SSL_CTX_use_PrivateKey(server->ctx, key) == 1);
  server->listen_fd = listen_loopback(port);
  assert(pthread_create(&server->thread, NULL, echo_thread, server) == 0);
}

static void echo_stop(EchoServer *server) {
  // Wake the accept loop with one plain connection, then let it find EBADF.
  int fd = socket(AF_INET, SOCK_STREAM, 0);
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof addr);
  addr.sin_family = AF_INET;
  addr.sin_port = htons((uint16_t)server->port);
  addr.sin_addr.s_addr = inet_addr(LOOPBACK);
  shutdown(server->listen_fd, SHUT_RDWR);
  close(server->listen_fd);
  connect(fd, (struct sockaddr *)&addr, sizeof addr);
  close(fd);
  pthread_join(server->thread, NULL);
  SSL_CTX_free(server->ctx);
}

// ---------------------------------------------------------------------------
// TLS session tests [FFI-TLS-CLIENT]
// ---------------------------------------------------------------------------

static int64_t session(int port) {
  int64_t h = osprey_tls_connect(LOOPBACK, port, 2000);
  assert(h >= 1);
  return h;
}

static void close_session(int64_t h) { assert(osprey_tls_close(h) == 0); }

// One hex write (skipped when `sent` is NULL) and one exact-count hex read,
// both asserted; returns the status the read left behind.
static int64_t exchange(int64_t h, char *sent, int64_t count, int64_t timeout_ms, char *want) {
  if (sent != NULL) {
    assert(osprey_tls_send_hex(h, sent) == 0);
  }
  char *got = osprey_tls_recv_hex(h, count, timeout_ms);
  assert(strcmp(got, want) == 0);
  return osprey_tls_status(h);
}

static void test_tls_echo_round_trip(void) {
  int64_t h = session(ECHO_PORT);
  assert(osprey_tls_send_hex(h, "48656c6c6f") == 0); // "Hello"
  assert(osprey_tls_status(h) == 0);
  assert(exchange(h, NULL, 5, 2000, "48656c6c6f") == 0);
  // Upper-case hex is accepted on the way in and comes back lower-case.
  exchange(h, "00FF10", 3, 2000, "00ff10");
  close_session(h);
  assert(osprey_tls_close(h) == -1);
  assert(osprey_tls_status(h) == -1);
}

static void test_tls_timeout_keeps_partial_bytes(void) {
  int64_t h = session(ECHO_PORT);
  // Only three of five bytes can arrive: the read times out and keeps them.
  assert(exchange(h, "010203", 5, 200, "") == 1);
  assert(exchange(h, "0405", 5, 2000, "0102030405") == 0);
  // A nothing-pending timeout is still a timeout, not a failure.
  assert(exchange(h, NULL, 1, 100, "") == 1);
  close_session(h);
}

static void test_tls_argument_rejections(void) {
  int64_t h = session(ECHO_PORT);
  assert(osprey_tls_send_hex(h, "abc") == -2);  // odd length
  assert(osprey_tls_send_hex(h, "zz") == -2);   // not hex
  assert(osprey_tls_send_hex(h, NULL) == -1);
  assert(osprey_tls_send_hex(999, "ab") == -1); // no such session
  assert(osprey_tls_send_hex(0, "ab") == -1);
  exchange(999, NULL, 1, 100, "");
  exchange(h, NULL, 0, 100, "");
  assert(osprey_tls_status(h) == 3);
  exchange(h, NULL, 1 << 30, 100, "");
  close_session(h);
  assert(osprey_tls_connect(NULL, ECHO_PORT, 100) == -1);
  assert(osprey_tls_connect("", ECHO_PORT, 100) == -1);
  assert(osprey_tls_connect(LOOPBACK, 65536, 100) == -1);
  assert(osprey_tls_connect(LOOPBACK, ECHO_PORT, 0) == -1);
  assert(osprey_tls_connect(LOOPBACK, ECHO_PORT, -1) == -1);
  assert(osprey_tls_connect(LOOPBACK, ECHO_PORT, 60001) == -1);
  assert(osprey_tls_connect(LOOPBACK, 0, 100) == -1);
  assert(osprey_tls_connect(NOWHERE, ECHO_PORT, 100) == -2);
  // Nothing listens on this port: the dial is refused.
  assert(osprey_tls_connect(LOOPBACK, 18099, 500) == -4);
}

static void test_tls_peer_close_is_reported(void) {
  int64_t h = session(ECHO_CLOSE_PORT);
  assert(exchange(h, NULL, 4, 2000, "") == 2);
  close_session(h);
}

static void test_tls_plain_listener_fails_handshake(void) {
  int plain = listen_loopback(18094);
  assert(osprey_tls_connect(LOOPBACK, 18094, 500) == -7);
  close(plain);
}

static void test_tls_slots_are_bounded(void) {
  int64_t handles[64];
  for (int i = 0; i < 64; i++) {
    handles[i] = osprey_tls_connect(LOOPBACK, ECHO_PORT, 2000);
    assert(handles[i] == i + 1);
  }
  assert(osprey_tls_connect(LOOPBACK, ECHO_PORT, 2000) == -5);
  for (int i = 0; i < 64; i++) {
    assert(osprey_tls_close(handles[i]) == 0);
  }
}

static void test_local_address(void) {
  char *address = osprey_net_local_address(LOOPBACK, 9);
  assert(strcmp(address, LOOPBACK) == 0);
  assert(strcmp(osprey_net_local_address(NOWHERE, 9), "") == 0);
  assert(strcmp(osprey_net_local_address(NULL, 9), "") == 0);
  assert(strcmp(osprey_net_local_address(LOOPBACK, 0), "") == 0);
}

int main(void) {
  signal(SIGPIPE, SIG_IGN);
  EVP_PKEY *key = EVP_RSA_gen(2048);
  assert(key != NULL);
  X509 *cert = self_signed(key);
  EchoServer echo;
  EchoServer closer;
  echo_start(&echo, ECHO_PORT, false, cert, key);
  echo_start(&closer, ECHO_CLOSE_PORT, true, cert, key);
  test_tls_echo_round_trip();
  test_tls_timeout_keeps_partial_bytes();
  test_tls_argument_rejections();
  test_tls_peer_close_is_reported();
  test_tls_plain_listener_fails_handshake();
  test_tls_slots_are_bounded();
  test_local_address();
  echo_stop(&echo);
  echo_stop(&closer);
  X509_free(cert);
  EVP_PKEY_free(key);
  run_file_server_tests();
  printf("✅ network runtime tests passed\n");
  return 0;
}
