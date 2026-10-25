// TLS origin tests, split from http_runtime_tests.c: a real OpenSSL server
// presenting test-generated certificate chains, so peer verification is
// asserted end to end [HTTP-STATUS-CLIENT].
#include "http_runtime_tests.h"
#include "http_shared.h"

#include <assert.h>
#include <openssl/err.h>
#include <openssl/pem.h>
#include <openssl/ssl.h>
#include <openssl/x509v3.h>


// ---------------------------------------------------------------------------
// TLS origin: a real OpenSSL server presenting a test-generated certificate
// chain, so peer verification is asserted end to end [HTTP-STATUS-CLIENT].
// ---------------------------------------------------------------------------

#define TLS_PORT 18090

// A self-signed root (basicConstraints CA:TRUE) when issuer is NULL, otherwise
// a leaf signed by that issuer. `san` is an X509V3 value such as
// "IP:127.0.0.1" or "DNS:wrong.example".
static X509 *tls_test_cert(EVP_PKEY *sign_key, X509 *issuer, EVP_PKEY *pub,
                           const char *cn, const char *san) {
  X509 *x = X509_new();
  if (!x) {
    return NULL;
  }
  ASN1_INTEGER_set(X509_get_serialNumber(x), (long)rand());
  X509_gmtime_adj(X509_getm_notBefore(x), -3600);
  X509_gmtime_adj(X509_getm_notAfter(x), 3600);
  X509_NAME *name = X509_NAME_dup(X509_get_subject_name(x));
  X509_NAME_add_entry_by_txt(name, "CN", MBSTRING_ASC,
                             (const unsigned char *)cn, -1, -1, 0);
  X509_set_subject_name(x, name);
  X509_set_issuer_name(x, issuer ? X509_get_subject_name(issuer) : name);
  X509_NAME_free(name);
  X509_set_pubkey(x, pub);
  const char *ext_name = issuer ? "subjectAltName" : "basicConstraints";
  const char *ext_value = issuer ? san : "critical,CA:TRUE";
  X509_EXTENSION *ext = X509V3_EXT_conf(NULL, NULL, ext_name, ext_value);
  if (!ext || X509_add_ext(x, ext, -1) != 1) {
    X509_EXTENSION_free(ext);
    X509_free(x);
    return NULL;
  }
  X509_EXTENSION_free(ext);
  if (X509_sign(x, sign_key, EVP_sha256()) == 0) {
    X509_free(x);
    return NULL;
  }
  return x;
}

typedef struct {
  int listen_fd;
  pthread_t thread;
  SSL_CTX *ctx;
  const char *reply;
  char request[MAX_HTTP_BUFFER];
} TlsOrigin;

// Reads one whole HTTP request through the TLS session: headers, then
// Content-Length bytes of body.
static void tls_origin_read_request(SSL *ssl, char *out, size_t capacity) {
  size_t len = 0;
  char *blank = NULL;
  while (len + 1 < capacity && blank == NULL) {
    int n = SSL_read(ssl, out + len, (int)(capacity - 1 - len));
    if (n <= 0) {
      break;
    }
    len += (size_t)n;
    out[len] = '\0';
    blank = strstr(out, "\r\n\r\n");
  }
  if (blank == NULL) {
    return;
  }
  const char *marker = strstr(out, "Content-Length: ");
  size_t want = marker ? (size_t)atoi(marker + 16) : 0;
  size_t have = len - (size_t)(blank + 4 - out);
  while (have < want && len + 1 < capacity) {
    int n = SSL_read(ssl, out + len, (int)(capacity - 1 - len));
    if (n <= 0) {
      break;
    }
    len += (size_t)n;
    have += (size_t)n;
    out[len] = '\0';
  }
}

static void *tls_origin_thread(void *arg) {
  TlsOrigin *origin = (TlsOrigin *)arg;
  int fd = accept(origin->listen_fd, NULL, NULL);
  if (fd < 0) {
    return NULL;
  }
  SSL *ssl = SSL_new(origin->ctx);
  if (ssl) {
    SSL_set_fd(ssl, fd);
    if (SSL_accept(ssl) == 1) {
      tls_origin_read_request(ssl, origin->request,
                              sizeof(origin->request));
      if (origin->reply) {
        SSL_write(ssl, origin->reply, (int)strlen(origin->reply));
      }
    }
    SSL_shutdown(ssl);
    SSL_free(ssl);
  }
  close(fd);
  return NULL;
}

static void tls_origin_start(TlsOrigin *origin, X509 *leaf, EVP_PKEY *leaf_key,
                             const char *reply) {
  memset(origin, 0, sizeof(*origin));
  origin->reply = reply;
  origin->ctx = SSL_CTX_new(TLS_server_method());
  assert(origin->ctx != NULL);
  assert(SSL_CTX_use_certificate(origin->ctx, leaf) == 1);
  assert(SSL_CTX_use_PrivateKey(origin->ctx, leaf_key) == 1);
  origin->listen_fd = socket(AF_INET, SOCK_STREAM, 0);
  assert(origin->listen_fd >= 0);
  int opt = 1;
  assert(setsockopt(origin->listen_fd, SOL_SOCKET, SO_REUSEADDR,
                    (const char *)&opt, sizeof(opt)) == 0);
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_port = htons(TLS_PORT);
  addr.sin_addr.s_addr = inet_addr("127.0.0.1");
  assert(bind(origin->listen_fd, (struct sockaddr *)&addr, sizeof(addr)) == 0);
  assert(listen(origin->listen_fd, 1) == 0);
  assert(pthread_create(&origin->thread, NULL, tls_origin_thread, origin) == 0);
}

static void tls_origin_stop(TlsOrigin *origin) {
  assert(pthread_join(origin->thread, NULL) == 0);
  close(origin->listen_fd);
  SSL_CTX_free(origin->ctx);
}

// One https:// client aimed at the TLS origin on TLS_PORT for `host`.
static int64_t tls_client(const char *host) {
  char url[128];
  snprintf(url, sizeof(url), "https://%s:%d", host, TLS_PORT);
  int64_t client_id = http_create_client(url, 3000);
  assert(client_id > 0);
  return client_id;
}

// Publishes the test CA through SSL_CERT_FILE -- the trust channel a user of
// the client runtime already has. Returns the heap path of the CA bundle.
static char *tls_ca_file_publish(X509 *ca) {
  const char *dir = getenv("TMPDIR");
  char path[512];
  snprintf(path, sizeof(path), "%s/osprey-tls-ca-%d.pem",
           (dir && *dir) ? dir : "/tmp", (int)getpid());
  BIO *bio = BIO_new_file(path, "w");
  assert(bio != NULL);
  assert(PEM_write_bio_X509(bio, ca) == 1);
  BIO_free(bio);
  assert(setenv("SSL_CERT_FILE", path, 1) == 0);
  return strdup(path);
}

// Restores SSL_CERT_FILE to the ORIGINAL value captured before any mutation
// (`prev`, NULL meaning it was unset) and removes the published bundle. No
// temporary environment variable: the original travels by argument, so it
// cannot be clobbered by the mutation it survives.
static void tls_ca_file_restore(char *path, const char *prev) {
  if (prev && *prev) {
    assert(setenv("SSL_CERT_FILE", prev, 1) == 0);
  } else {
    assert(unsetenv("SSL_CERT_FILE") == 0);
  }
  unlink(path);
  free(path);
}

// Serves `reply` behind a leaf cert for `san`, then asserts a GET is refused
// with the TLS error: a trusted CA's certificate for the WRONG identity (or
// an untrusted issuer, when ca is NULL) must never be served.
static void tls_expect_rejected(EVP_PKEY *ca_key, X509 *ca, const char *cn,
                                const char *san, const char *host) {
  EVP_PKEY *key = EVP_RSA_gen(2048);
  X509 *leaf = tls_test_cert(ca_key, ca, key, cn, san);
  assert(leaf != NULL);
  TlsOrigin origin;
  tls_origin_start(&origin, leaf, key,
                   "HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nbad");
  int64_t client_id = tls_client(host);
  assert(http_get(client_id, (char *)(uintptr_t) "/x",
                  (char *)(uintptr_t) "") == -12);
  tls_origin_stop(&origin);
  X509_free(leaf);
  EVP_PKEY_free(key);
  http_close_client(client_id);
}

// Serves `reply` behind a leaf cert VALID for `host`, and asserts the full
// response-handle round trip: status, body, and the request the origin saw.
static void tls_expect_served(EVP_PKEY *ca_key, X509 *ca, const char *cn,
                              const char *san, const char *host,
                              const char *expect_body) {
  EVP_PKEY *key = EVP_RSA_gen(2048);
  X509 *leaf = tls_test_cert(ca_key, ca, key, cn, san);
  assert(leaf != NULL);
  char reply[128];
  snprintf(reply, sizeof(reply), "HTTP/1.1 200 OK\r\nContent-Length: %zu\r\n\r\n%s",
           strlen(expect_body), expect_body);
  TlsOrigin origin;
  tls_origin_start(&origin, leaf, key, reply);
  int64_t client_id = tls_client(host);
  int64_t handle = http_get_response(client_id, (char *)(uintptr_t) "/x",
                                     (char *)(uintptr_t) "");
  assert(handle >= 1);
  assert(http_response_status(handle) == 200);
  char *body = http_response_body(handle);
  assert(body != NULL && strcmp(body, expect_body) == 0);
  free(body);
  assert(http_response_free(handle) == 0);
  tls_origin_stop(&origin);
  assert(strncmp(origin.request, "GET /x HTTP/1.1", 15) == 0);
  X509_free(leaf);
  EVP_PKEY_free(key);
  http_close_client(client_id);
}

// A https:// request must accept ONLY a certificate that is (a) issued by a
// trusted CA and (b) valid for the identity dialed -- an IP literal OR a DNS
// name such as localhost. An untrusted issuer, a wrong host or a wrong DNS
// name must be rejected as a TLS error, never silently served
// [HTTP-STATUS-CLIENT].
void test_client_tls_peer_verification(void) {
  printf("Testing TLS peer verification...\n");

  // 0. Capture the ORIGINAL trust environment BEFORE the untrusted case
  // clears it -- otherwise every later "restore" restores the mutation.
  const char *saved_env = getenv("SSL_CERT_FILE");
  char *saved = saved_env ? strdup(saved_env) : NULL;

  // 1. Untrusted issuer: self-signed leaf, no CA published anywhere.
  EVP_PKEY *rogue_key = EVP_RSA_gen(2048);
  X509 *rogue = tls_test_cert(rogue_key, NULL, rogue_key, "rogue",
                              "IP:127.0.0.1");
  assert(rogue != NULL);
  TlsOrigin origin;
  tls_origin_start(&origin, rogue, rogue_key,
                   "HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nbad");
  int64_t client_id = tls_client("127.0.0.1");
  assert(http_get(client_id, (char *)(uintptr_t) "/x",
                  (char *)(uintptr_t) "") == -12);
  tls_origin_stop(&origin);
  X509_free(rogue);
  EVP_PKEY_free(rogue_key);
  http_close_client(client_id);

  // 2. Trusted CA, so every leaf below chains to a published root.
  EVP_PKEY *ca_key = EVP_RSA_gen(2048);
  X509 *ca = tls_test_cert(ca_key, NULL, ca_key, "Osprey Test Root", NULL);
  assert(ca != NULL);
  char *ca_file = tls_ca_file_publish(ca);

  // 2a. Wrong host: trusted issuer, but the SAN is a DNS name, not the dialed
  // IP. 2b. Wrong DNS name: dialed localhost, SAN says something else.
  tls_expect_rejected(ca_key, ca, "wrong", "DNS:wrong.example", "127.0.0.1");
  tls_expect_rejected(ca_key, ca, "not-localhost", "DNS:other.example",
                      "localhost");

  // 2c/2d. The two accepted paths: SAN matches the dialed IP literal, and SAN
  // matches the dialed DNS name (localhost).
  tls_expect_served(ca_key, ca, "loopback", "IP:127.0.0.1", "127.0.0.1",
                    "trust");
  tls_expect_served(ca_key, ca, "local", "DNS:localhost", "localhost",
                    "dns-ok");

  X509_free(ca);
  EVP_PKEY_free(ca_key);
  tls_ca_file_restore(ca_file, saved);
  free(saved);
  printf("✅ TLS peer verification passed!\n\n");
}
