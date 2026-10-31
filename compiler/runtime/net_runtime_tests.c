// Assertion-driven tests for the extern-callable network units: a failed
// assert aborts the binary. This suite OWNS tls_client_runtime.c and
// file_server_runtime.c.
//
// Everything runs against real loopback sockets in this process: an OpenSSL
// echo server with a self-signed certificate for the TLS session, and a plain
// TCP client for the single-file HTTP server, so each unit is asserted end to
// end. Spec anchors:
//   [FFI-TLS-CLIENT]      dial, hex writes, exact-count hex reads, status, close
//   [FFI-TLS-UNVERIFIED]  a self-signed peer is accepted
//   [FFI-FILE-SERVER]     200/206/416/404/405 and byte-exact bodies
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
int64_t osprey_file_server_start(char *path, char *content_type, char *address);
int64_t osprey_file_server_stop(int64_t port);

enum { ECHO_PORT = 18092, ECHO_CLOSE_PORT = 18093, MEDIA_BYTES = 200000 };

static char LOOPBACK[] = "127.0.0.1";

// ---------------------------------------------------------------------------
// A self-signed certificate, generated in memory, so the suite needs no files.
// ---------------------------------------------------------------------------

static EVP_PKEY *test_key(void) {
  EVP_PKEY *key = EVP_RSA_gen(2048);
  assert(key != NULL);
  return key;
}

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

static void test_tls_echo_round_trip(void) {
  int64_t h = osprey_tls_connect(LOOPBACK, ECHO_PORT, 2000);
  assert(h >= 1);
  char hello[] = "48656c6c6f"; // "Hello"
  assert(osprey_tls_send_hex(h, hello) == 0);
  assert(osprey_tls_status(h) == 0);
  char *back = osprey_tls_recv_hex(h, 5, 2000);
  assert(strcmp(back, hello) == 0);
  assert(osprey_tls_status(h) == 0);
  // Upper-case hex is accepted on the way in and comes back lower-case.
  char upper[] = "00FF10";
  assert(osprey_tls_send_hex(h, upper) == 0);
  char *lower = osprey_tls_recv_hex(h, 3, 2000);
  assert(strcmp(lower, "00ff10") == 0);
  assert(osprey_tls_close(h) == 0);
  assert(osprey_tls_close(h) == -1);
  assert(osprey_tls_status(h) == -1);
}

static void test_tls_timeout_keeps_partial_bytes(void) {
  int64_t h = osprey_tls_connect(LOOPBACK, ECHO_PORT, 2000);
  assert(h >= 1);
  char three[] = "010203";
  assert(osprey_tls_send_hex(h, three) == 0);
  // Only three of five bytes can arrive: the read times out and keeps them.
  char *partial = osprey_tls_recv_hex(h, 5, 200);
  assert(strcmp(partial, "") == 0);
  assert(osprey_tls_status(h) == 1);
  char two[] = "0405";
  assert(osprey_tls_send_hex(h, two) == 0);
  char *whole = osprey_tls_recv_hex(h, 5, 2000);
  assert(strcmp(whole, "0102030405") == 0);
  assert(osprey_tls_status(h) == 0);
  // A nothing-pending timeout is still a timeout, not a failure.
  assert(strcmp(osprey_tls_recv_hex(h, 1, 100), "") == 0);
  assert(osprey_tls_status(h) == 1);
  assert(osprey_tls_close(h) == 0);
}

static void test_tls_argument_rejections(void) {
  int64_t h = osprey_tls_connect(LOOPBACK, ECHO_PORT, 2000);
  assert(h >= 1);
  char odd[] = "abc";
  char bad[] = "zz";
  char ok[] = "ab";
  assert(osprey_tls_send_hex(h, odd) == -2);
  assert(osprey_tls_send_hex(h, bad) == -2);
  assert(osprey_tls_send_hex(h, NULL) == -1);
  assert(osprey_tls_send_hex(999, ok) == -1);
  assert(osprey_tls_send_hex(0, ok) == -1);
  assert(strcmp(osprey_tls_recv_hex(999, 1, 100), "") == 0);
  assert(strcmp(osprey_tls_recv_hex(h, 0, 100), "") == 0);
  assert(osprey_tls_status(h) == 3);
  assert(strcmp(osprey_tls_recv_hex(h, 1 << 30, 100), "") == 0);
  assert(osprey_tls_close(h) == 0);
  assert(osprey_tls_connect(NULL, ECHO_PORT, 100) == -1);
  assert(osprey_tls_connect(LOOPBACK, 0, 100) == -1);
  char nowhere[] = "no-such-host.invalid";
  assert(osprey_tls_connect(nowhere, ECHO_PORT, 100) == -2);
  // Nothing listens on this port: the dial is refused.
  assert(osprey_tls_connect(LOOPBACK, 18099, 500) == -4);
}

static void test_tls_peer_close_is_reported(void) {
  int64_t h = osprey_tls_connect(LOOPBACK, ECHO_CLOSE_PORT, 2000);
  assert(h >= 1);
  char *none = osprey_tls_recv_hex(h, 4, 2000);
  assert(strcmp(none, "") == 0);
  assert(osprey_tls_status(h) == 2);
  assert(osprey_tls_close(h) == 0);
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
  char nowhere[] = "no-such-host.invalid";
  assert(strcmp(osprey_net_local_address(nowhere, 9), "") == 0);
  assert(strcmp(osprey_net_local_address(NULL, 9), "") == 0);
  assert(strcmp(osprey_net_local_address(LOOPBACK, 0), "") == 0);
}

// ---------------------------------------------------------------------------
// File server tests [FFI-FILE-SERVER]
// ---------------------------------------------------------------------------

static char media_path[256];

static unsigned char media_byte(int64_t i) { return (unsigned char)((i * 7 + i / 251) & 255); }

static void write_media(void) {
  snprintf(media_path, sizeof media_path, "/tmp/osprey-net-tests-%d.bin", (int)getpid());
  FILE *file = fopen(media_path, "wb");
  assert(file != NULL);
  for (int64_t i = 0; i < MEDIA_BYTES; i++) {
    unsigned char b = media_byte(i);
    assert(fwrite(&b, 1, 1, file) == 1);
  }
  fclose(file);
}

typedef struct {
  int status;
  char headers[4096];
  unsigned char *body;
  size_t body_len;
} Reply;

// Sends one raw request and reads the whole reply until the server closes.
static void fetch(int port, const char *request, Reply *reply) {
  int fd = socket(AF_INET, SOCK_STREAM, 0);
  assert(fd >= 0);
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof addr);
  addr.sin_family = AF_INET;
  addr.sin_port = htons((uint16_t)port);
  addr.sin_addr.s_addr = inet_addr(LOOPBACK);
  assert(connect(fd, (struct sockaddr *)&addr, sizeof addr) == 0);
  assert(send(fd, request, strlen(request), 0) == (ssize_t)strlen(request));
  size_t cap = MEDIA_BYTES + 8192;
  unsigned char *all = malloc(cap);
  size_t len = 0;
  for (;;) {
    ssize_t n = recv(fd, all + len, cap - len, 0);
    if (n <= 0) {
      break;
    }
    len += (size_t)n;
  }
  close(fd);
  all[len] = '\0';
  char *blank = strstr((char *)all, "\r\n\r\n");
  assert(blank != NULL);
  size_t header_len = (size_t)(blank + 4 - (char *)all);
  memcpy(reply->headers, all, header_len);
  reply->headers[header_len] = '\0';
  reply->status = atoi((char *)all + 9);
  reply->body_len = len - header_len;
  reply->body = malloc(reply->body_len + 1);
  memcpy(reply->body, all + header_len, reply->body_len);
  free(all);
}

static void assert_body_is(const Reply *reply, int64_t start, int64_t count) {
  assert(reply->body_len == (size_t)count);
  for (int64_t i = 0; i < count; i++) {
    assert(reply->body[i] == media_byte(start + i));
  }
}

static void test_file_server_whole_and_ranges(void) {
  char type[] = "audio/mpeg";
  int64_t port = osprey_file_server_start(media_path, type, LOOPBACK);
  assert(port > 0);
  Reply reply;
  fetch((int)port, "GET /media HTTP/1.1\r\nHost: x\r\n\r\n", &reply);
  assert(reply.status == 200);
  assert(strstr(reply.headers, "Content-Type: audio/mpeg\r\n") != NULL);
  assert(strstr(reply.headers, "Accept-Ranges: bytes\r\n") != NULL);
  assert(strstr(reply.headers, "Content-Length: 200000\r\n") != NULL);
  assert_body_is(&reply, 0, MEDIA_BYTES);
  free(reply.body);
  fetch((int)port, "GET /media HTTP/1.1\r\nHost: x\r\nRange: bytes=100-199\r\n\r\n", &reply);
  assert(reply.status == 206);
  assert(strstr(reply.headers, "Content-Range: bytes 100-199/200000\r\n") != NULL);
  assert(strstr(reply.headers, "Content-Length: 100\r\n") != NULL);
  assert_body_is(&reply, 100, 100);
  free(reply.body);
  // An open range runs to the end; an end past the file is clamped.
  fetch((int)port, "GET /media HTTP/1.1\r\nRange: bytes=199990-\r\n\r\n", &reply);
  assert(reply.status == 206);
  assert(strstr(reply.headers, "Content-Range: bytes 199990-199999/200000\r\n") != NULL);
  assert_body_is(&reply, 199990, 10);
  free(reply.body);
  fetch((int)port, "GET /media HTTP/1.1\r\nRange: bytes=199000-999999\r\n\r\n", &reply);
  assert(reply.status == 206);
  assert_body_is(&reply, 199000, 1000);
  free(reply.body);
  assert(osprey_file_server_stop(port) == 0);
}

static void test_file_server_refusals(void) {
  char type[] = "audio/mpeg";
  int64_t port = osprey_file_server_start(media_path, type, LOOPBACK);
  assert(port > 0);
  Reply reply;
  fetch((int)port, "GET /media HTTP/1.1\r\nRange: bytes=300000-\r\n\r\n", &reply);
  assert(reply.status == 416);
  assert(strstr(reply.headers, "Content-Range: bytes */200000\r\n") != NULL);
  assert(reply.body_len == 0);
  free(reply.body);
  fetch((int)port, "GET /media HTTP/1.1\r\nRange: bytes=50-10\r\n\r\n", &reply);
  assert(reply.status == 416);
  free(reply.body);
  // A Range that is not `bytes=start-` is ignored: the whole file is sent.
  fetch((int)port, "GET /media HTTP/1.1\r\nRange: bytes=x-\r\n\r\n", &reply);
  assert(reply.status == 200);
  assert_body_is(&reply, 0, MEDIA_BYTES);
  free(reply.body);
  fetch((int)port, "HEAD /media HTTP/1.1\r\n\r\n", &reply);
  assert(reply.status == 200);
  assert(strstr(reply.headers, "Content-Length: 200000\r\n") != NULL);
  assert(reply.body_len == 0);
  free(reply.body);
  fetch((int)port, "GET /other HTTP/1.1\r\n\r\n", &reply);
  assert(reply.status == 404);
  free(reply.body);
  fetch((int)port, "POST /media HTTP/1.1\r\n\r\n", &reply);
  assert(reply.status == 405);
  assert(strstr(reply.headers, "Allow: GET, HEAD\r\n") != NULL);
  free(reply.body);
  fetch((int)port, "garbage\r\n\r\n", &reply);
  assert(reply.status == 400);
  free(reply.body);
  assert(osprey_file_server_stop(port) == 0);
  assert(osprey_file_server_stop(port) == -1);
}

static void test_file_server_start_failures(void) {
  char type[] = "audio/mpeg";
  char missing[] = "/tmp/osprey-net-tests-missing.bin";
  char directory[] = "/tmp";
  char bad_address[] = "not-an-address";
  assert(osprey_file_server_start(NULL, type, LOOPBACK) == -1);
  assert(osprey_file_server_start(media_path, NULL, LOOPBACK) == -1);
  assert(osprey_file_server_start(media_path, type, NULL) == -1);
  assert(osprey_file_server_start(missing, type, LOOPBACK) == -2);
  assert(osprey_file_server_start(directory, type, LOOPBACK) == -2);
  assert(osprey_file_server_start(media_path, type, bad_address) == -4);
  int64_t ports[16];
  for (int i = 0; i < 16; i++) {
    ports[i] = osprey_file_server_start(media_path, type, LOOPBACK);
    assert(ports[i] > 0);
  }
  assert(osprey_file_server_start(media_path, type, LOOPBACK) == -3);
  for (int i = 0; i < 16; i++) {
    assert(osprey_file_server_stop(ports[i]) == 0);
  }
}

static void test_file_server_closes_with_the_player(void) {
  // A player that drops the connection mid-body must not take the server down.
  char type[] = "audio/mpeg";
  int64_t port = osprey_file_server_start(media_path, type, LOOPBACK);
  assert(port > 0);
  int fd = socket(AF_INET, SOCK_STREAM, 0);
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof addr);
  addr.sin_family = AF_INET;
  addr.sin_port = htons((uint16_t)port);
  addr.sin_addr.s_addr = inet_addr(LOOPBACK);
  assert(connect(fd, (struct sockaddr *)&addr, sizeof addr) == 0);
  const char *request = "GET /media HTTP/1.1\r\n\r\n";
  assert(send(fd, request, strlen(request), 0) > 0);
  char first[16];
  assert(recv(fd, first, sizeof first, 0) > 0);
  close(fd);
  Reply reply;
  fetch((int)port, "GET /media HTTP/1.1\r\nRange: bytes=0-0\r\n\r\n", &reply);
  assert(reply.status == 206);
  assert_body_is(&reply, 0, 1);
  free(reply.body);
  assert(osprey_file_server_stop(port) == 0);
}

int main(void) {
  signal(SIGPIPE, SIG_IGN);
  EVP_PKEY *key = test_key();
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
  write_media();
  test_file_server_whole_and_ranges();
  test_file_server_refusals();
  test_file_server_start_failures();
  test_file_server_closes_with_the_player();
  unlink(media_path);
  printf("✅ network runtime tests passed\n");
  return 0;
}
