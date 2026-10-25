#include "http_client_internal.h"
#include "http_shared.h"

#include <arpa/inet.h> // inet_pton
#include <openssl/err.h>
#include <openssl/ssl.h>
#include <openssl/x509_vfy.h> // X509_VERIFY_PARAM

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
  client->timeout = (int)timeout;
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

// transport_close tears down the TLS session (if any) and closes the socket.
static void transport_close(SSL *ssl, SSL_CTX *ctx, int sock) {
  if (ssl) {
    SSL_shutdown(ssl);
    SSL_free(ssl);
  }
  if (ctx) {
    SSL_CTX_free(ctx);
  }
#ifdef _WIN32
  closesocket(sock);
#else
  close(sock);
#endif
}

// transport_send / transport_recv route I/O over TLS when ssl is set, otherwise
// over the bare socket, so http_perform speaks both http:// and https://.
static ssize_t transport_send(SSL *ssl, int sock, const char *buf, int len) {
  if (ssl) {
    return SSL_write(ssl, buf, len);
  }
  return send(sock, buf, (size_t)len, 0);
}

static ssize_t transport_recv(SSL *ssl, int sock, char *buf, int cap) {
  if (ssl) {
    return SSL_read(ssl, buf, cap);
  }
  return recv(sock, buf, (size_t)cap, 0);
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

// http_perform connects, sends the request and reads the ENTIRE response into a
// freshly allocated, NUL-terminated buffer (the server uses Connection: close,
// so recv() draining to EOF captures the whole body). For https:// clients the
// transport is wrapped in TLS. Returns 0 on success with *out_raw / *out_len
// set (caller frees *out_raw), or a negative error code.
int64_t http_perform(int64_t client_id, int64_t method, char *path,
                      char *headers, char *body, char **out_raw,
                      size_t *out_len) {
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

  int sock = socket(AF_INET, SOCK_STREAM, 0);
  if (sock < 0) {
    return -3;
  }

  struct timeval tv;
  tv.tv_sec = client->timeout / 1000;
  tv.tv_usec = (client->timeout % 1000) * 1000;
  setsockopt(sock, SOL_SOCKET, SO_RCVTIMEO, (const char *)&tv, sizeof tv);

  struct sockaddr_in server_addr;
  memset(&server_addr, 0, sizeof(server_addr));
  server_addr.sin_family = AF_INET;
  server_addr.sin_port = htons(client->port);

  struct hostent *server = gethostbyname(client->host);
  if (!server) {
#ifdef _WIN32
    closesocket(sock);
#else
    close(sock);
#endif
    return -4;
  }

  memcpy(&server_addr.sin_addr.s_addr, server->h_addr, server->h_length);

  if (connect(sock, (struct sockaddr *)&server_addr, sizeof(server_addr)) < 0) {
#ifdef _WIN32
    closesocket(sock);
#else
    close(sock);
#endif
    return -5;
  }

  // Establish a TLS session for https:// clients. SNI (the host name) is
  // mandatory for virtual-hosted endpoints such as api.github.com, and every
  // session verifies the peer chain AND that the certificate matches the
  // host/IP we dialed [HTTP-STATUS-CLIENT].
  SSL_CTX *ctx = NULL;
  SSL *ssl = NULL;
  if (client->is_https) {
    ctx = tls_client_ctx();
    if (!ctx) {
      transport_close(NULL, NULL, sock);
      return -13; // trust store/verification prerequisites unavailable
    }
    ssl = SSL_new(ctx);
    if (!ssl) {
      transport_close(NULL, ctx, sock);
      return -11;
    }
    SSL_set_fd(ssl, sock);
    SSL_set_tlsext_host_name(ssl, client->host);
    if (tls_expect_peer(ssl, client->host) != 1) {
      transport_close(ssl, ctx, sock);
      return -11;
    }
    if (SSL_connect(ssl) != 1 || SSL_get_verify_result(ssl) != X509_V_OK) {
      transport_close(ssl, ctx, sock);
      return -12; // handshake or certificate verification failed
    }
  }

  char request[MAX_HTTP_BUFFER];
  const char *method_str = http_method_to_string((HttpMethod)method);

  // The Host header carries the bare host name (no :port) - servers like
  // GitHub reject "Host: api.github.com:443" with 400.
  int request_len = snprintf(request, sizeof(request),
                             "%s %s HTTP/1.1\r\n"
                             "Host: %s\r\n"
                             "Connection: close\r\n",
                             method_str, path, client->host);

  if (headers && strlen(headers) > 0) {
    request_len += snprintf(request + request_len, sizeof(request) - request_len,
                            "%s\r\n", headers);
  }

  if (body && strlen(body) > 0) {
    request_len += snprintf(request + request_len, sizeof(request) - request_len,
                            "Content-Length: %zu\r\n\r\n%s", strlen(body), body);
  } else {
    request_len += snprintf(request + request_len,
                            sizeof(request) - request_len, "\r\n");
  }

  if (transport_send(ssl, sock, request, request_len) < 0) {
    transport_close(ssl, ctx, sock);
    return -6;
  }

  size_t cap = MAX_HTTP_BUFFER;
  size_t len = 0;
  char *buf = malloc(cap);
  if (!buf) {
    transport_close(ssl, ctx, sock);
    return -9;
  }

  for (;;) {
    if (len + CHUNK_SIZE + 1 > cap) {
      cap *= 2;
      char *nb = realloc(buf, cap);
      if (!nb) {
        free(buf);
        transport_close(ssl, ctx, sock);
        return -9;
      }
      buf = nb;
    }
    ssize_t n = transport_recv(ssl, sock, buf + len, CHUNK_SIZE);
    if (n > 0) {
      len += (size_t)n;
    } else {
      break; // 0 = peer closed, <0 = timeout/error: stop draining
    }
  }

  transport_close(ssl, ctx, sock);

  if (len == 0) {
    free(buf);
    return -7;
  }

  buf[len] = '\0';
  *out_raw = buf;
  *out_len = len;
  return 0;
}

// Shared status-only request implementation.
static int64_t http_request(int64_t client_id, int64_t method, char *path,
                            char *headers, char *body) {
  char *raw = NULL;
  size_t len = 0;
  int64_t rc = http_perform(client_id, method, path, headers, body, &raw, &len);
  if (rc != 0) {
    return rc;
  }
  int64_t status = http_parse_status_line(raw);
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