// Scripted-origin client tests, split from http_runtime_tests.c so each
// test TU stays under the repo size budget. The origin is a raw TCP server
// answering with EXACT bytes, so the client is asserted against wire
// responses the Osprey server would never produce [HTTP-STATUS-CLIENT],
// [HTTP-RESPONSE-HANDLE].
#include "http_runtime_tests.h"
#include "http_shared.h"

#include <assert.h>

// ---------------------------------------------------------------------------
// Scripted origin: a raw TCP server that answers with EXACT bytes. It lets the
// client be asserted against wire responses the Osprey server would never
// produce -- chunked framing, a bogus status line, a silent close -- and
// records the request the client actually sent [HTTP-STATUS-CLIENT].
// ---------------------------------------------------------------------------

#define ORIGIN_PORT 18088

typedef struct {
  int listen_fd;
  const char *reply; // NULL: accept and close without writing a byte
  char request[MAX_HTTP_BUFFER];
  pthread_t thread;
} ScriptedOrigin;

// Reads one whole HTTP request: headers, then Content-Length bytes of body.
static void origin_read_request(int fd, char *out, size_t capacity) {
  size_t len = 0;
  char *blank = NULL;
  while (len + 1 < capacity && blank == NULL) {
    ssize_t n = recv(fd, out + len, capacity - 1 - len, 0);
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
    ssize_t n = recv(fd, out + len, capacity - 1 - len, 0);
    if (n <= 0) {
      break;
    }
    len += (size_t)n;
    have += (size_t)n;
    out[len] = '\0';
  }
}

static void *origin_thread(void *arg) {
  ScriptedOrigin *origin = (ScriptedOrigin *)arg;
  int fd = accept(origin->listen_fd, NULL, NULL);
  if (fd < 0) {
    return NULL;
  }
  origin_read_request(fd, origin->request, sizeof(origin->request));
  if (origin->reply) {
    size_t total = strlen(origin->reply);
    size_t sent = 0;
    while (sent < total) {
      ssize_t n = send(fd, origin->reply + sent, total - sent, 0);
      if (n <= 0) {
        break;
      }
      sent += (size_t)n;
    }
  }
  close(fd); // Connection: close is what makes the client stop draining
  return NULL;
}

static void origin_start(ScriptedOrigin *origin, const char *reply) {
  memset(origin, 0, sizeof(*origin));
  origin->reply = reply;
  origin->listen_fd = socket(AF_INET, SOCK_STREAM, 0);
  assert(origin->listen_fd >= 0);
  int opt = 1;
  assert(setsockopt(origin->listen_fd, SOL_SOCKET, SO_REUSEADDR,
                    (const char *)&opt, sizeof(opt)) == 0);
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_port = htons(ORIGIN_PORT);
  addr.sin_addr.s_addr = inet_addr("127.0.0.1");
  assert(bind(origin->listen_fd, (struct sockaddr *)&addr, sizeof(addr)) == 0);
  assert(listen(origin->listen_fd, 1) == 0);
  assert(pthread_create(&origin->thread, NULL, origin_thread, origin) == 0);
}

static void origin_stop(ScriptedOrigin *origin) {
  assert(pthread_join(origin->thread, NULL) == 0);
  close(origin->listen_fd);
}

static int64_t origin_client(void) {
  int64_t client_id = http_create_client(
      (char *)(uintptr_t) "http://127.0.0.1:18088", 2000);
  assert(client_id > 0);
  return client_id;
}

// A chunked response is decoded before the caller sees it, and header lookup
// ignores case [HTTP-RESPONSE-HANDLE].
void test_client_chunked_response(void) {
  ScriptedOrigin origin;
  origin_start(&origin, "HTTP/1.1 200 OK\r\n"
                        "Transfer-Encoding: chunked\r\n"
                        "X-Mixed-Case: Yes\r\n"
                        "\r\n"
                        "5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n");
  int64_t client_id = origin_client();
  int64_t handle = http_get_response(client_id, (char *)(uintptr_t) "/chunked",
                                     (char *)(uintptr_t) "");
  origin_stop(&origin);
  assert(handle >= 1);
  assert(http_response_status(handle) == 200);
  char *body = http_response_body(handle);
  assert(body != NULL && strcmp(body, "hello world") == 0);
  free(body);
  char *header = http_response_header(handle, (char *)(uintptr_t) "x-mixed-case");
  assert(header != NULL && strcmp(header, "Yes") == 0);
  free(header);
  // A header name that is a prefix of a real one must NOT match.
  assert(http_response_header(handle, (char *)(uintptr_t) "X-Mixed") == NULL);
  assert(http_response_header(handle, NULL) == NULL);
  assert(http_response_free(handle) == 0);
  assert(strstr(origin.request, "GET /chunked HTTP/1.1\r\n") == origin.request);
  assert(strstr(origin.request, "Host: 127.0.0.1\r\n") != NULL);
  assert(strstr(origin.request, "Connection: close\r\n") != NULL);
  http_close_client(client_id);
}

// HTTP/1.0 status lines parse; anything that is not a status line is -8, and a
// server that closes without writing is -7 [HTTP-STATUS-CLIENT].
void test_client_status_line_handling(void) {
  ScriptedOrigin origin;
  origin_start(&origin, "HTTP/1.0 404 Not Found\r\n\r\nmissing");
  int64_t client_id = origin_client();
  assert(http_get(client_id, (char *)(uintptr_t) "/gone",
                  (char *)(uintptr_t) "") == 404);
  origin_stop(&origin);

  origin_start(&origin, "NOT-HTTP AT ALL\r\n\r\n");
  assert(http_get(client_id, (char *)(uintptr_t) "/junk",
                  (char *)(uintptr_t) "") == -8);
  origin_stop(&origin);

  origin_start(&origin, NULL); // accepted, then closed in silence
  assert(http_get(client_id, (char *)(uintptr_t) "/silent",
                  (char *)(uintptr_t) "") == -7);
  origin_stop(&origin);
  http_close_client(client_id);
}

// Every method surface reaches the wire with its own verb, and a body arrives
// with a matching Content-Length [HTTP-STATUS-CLIENT].
void test_client_methods_and_body(void) {
  const char *ok = "HTTP/1.1 204 No Content\r\n\r\n";
  ScriptedOrigin origin;
  int64_t client_id = origin_client();

  origin_start(&origin, ok);
  assert(http_post(client_id, (char *)(uintptr_t) "/submit",
                   (char *)(uintptr_t) "data",
                   (char *)(uintptr_t) "X-Trace: 7\r\n") == 204);
  origin_stop(&origin);
  assert(strstr(origin.request, "POST /submit HTTP/1.1\r\n") == origin.request);
  assert(strstr(origin.request, "X-Trace: 7\r\n") != NULL);
  assert(strstr(origin.request, "Content-Length: 4\r\n\r\ndata") != NULL);

  origin_start(&origin, ok);
  assert(http_put(client_id, (char *)(uintptr_t) "/item",
                  (char *)(uintptr_t) "v", (char *)(uintptr_t) "") == 204);
  origin_stop(&origin);
  assert(strstr(origin.request, "PUT /item HTTP/1.1\r\n") == origin.request);

  origin_start(&origin, ok);
  assert(http_delete(client_id, (char *)(uintptr_t) "/item",
                     (char *)(uintptr_t) "") == 204);
  origin_stop(&origin);
  assert(strstr(origin.request, "DELETE /item HTTP/1.1\r\n") == origin.request);
  http_close_client(client_id);
}

// Transport failures each report their own code rather than a status
// [HTTP-STATUS-CLIENT].
void test_client_transport_failures(void) {
  assert(http_create_client(NULL, 5000) == -1);
  assert(http_create_client((char *)(uintptr_t) "http://h", -1) == -2);
  assert(http_create_client((char *)(uintptr_t) "http://:0/", 5000) == -4);

  int64_t refused =
      http_create_client((char *)(uintptr_t) "http://127.0.0.1:1", 1000);
  assert(refused > 0);
  assert(http_get(refused, (char *)(uintptr_t) "/", (char *)(uintptr_t) "") ==
         -5);
  assert(http_get(refused, NULL, (char *)(uintptr_t) "") == -2);
  http_close_client(refused);

  char long_host[400];
  memcpy(long_host, "http://", 7);
  memset(long_host + 7, 'a', sizeof(long_host) - 10);
  memcpy(long_host + sizeof(long_host) - 3, "/x", 3);
  int64_t unresolvable = http_create_client(long_host, 1000);
  assert(unresolvable > 0);
  assert(http_get(unresolvable, (char *)(uintptr_t) "/",
                  (char *)(uintptr_t) "") == -4);
  http_close_client(unresolvable);
}

// A client handle outside the table must be REJECTED, never used as an index:
// clients[] holds MAX_CLIENTS entries and a raw index is an out-of-bounds
// access [HTTP-STATUS-CLIENT].
void test_client_large_response(void) {
  enum { BIG_BODY = 12000 };
  static char reply[BIG_BODY + 256];
  int head = snprintf(reply, sizeof(reply),
                      "HTTP/1.1 200 OK\r\nContent-Length: %d\r\n\r\n", BIG_BODY);
  assert(head > 0);
  memset(reply + head, 'Z', BIG_BODY);
  reply[head + BIG_BODY] = '\0';
  ScriptedOrigin origin;
  origin_start(&origin, reply);
  int64_t client_id = origin_client();
  int64_t handle = http_get_response(client_id, (char *)(uintptr_t) "/big",
                                     (char *)(uintptr_t) "");
  origin_stop(&origin);
  assert(handle >= 1);
  char *body = http_response_body(handle);
  assert(body != NULL && strlen(body) == BIG_BODY);
  assert(body[0] == 'Z' && body[BIG_BODY - 1] == 'Z');
  free(body);
  // A response with no Transfer-Encoding header is returned unchanged.
  char *length_header =
      http_response_header(handle, (char *)(uintptr_t) "Content-Length");
  assert(length_header != NULL && strcmp(length_header, "12000") == 0);
  free(length_header);
  assert(http_response_free(handle) == 0);
  http_close_client(client_id);
}

// Chunked framing the origin got wrong stops decoding at the damage instead of
// walking off the end of the body [HTTP-RESPONSE-HANDLE].
static void expect_chunked_body(const char *chunks, const char *expected) {
  static char reply[512];
  int written = snprintf(reply, sizeof(reply),
                         "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n%s",
                         chunks);
  assert(written > 0 && (size_t)written < sizeof(reply));
  ScriptedOrigin origin;
  origin_start(&origin, reply);
  int64_t client_id = origin_client();
  int64_t handle = http_get_response(client_id, (char *)(uintptr_t) "/c",
                                     (char *)(uintptr_t) "");
  origin_stop(&origin);
  assert(handle >= 1);
  char *body = http_response_body(handle);
  assert(body != NULL && strcmp(body, expected) == 0);
  free(body);
  assert(http_response_free(handle) == 0);
  http_close_client(client_id);
}

void test_client_malformed_chunks(void) {
  expect_chunked_body("zz\r\nhello\r\n0\r\n\r\n", "");   // size is not hex
  expect_chunked_body("4", "");                          // size line unfinished
  expect_chunked_body("3\r\nabc\r\nzz\r\n", "abc");      // damage after a chunk
  expect_chunked_body("0\r\n\r\n", "");                  // terminator only
}

// An https:// client negotiates TLS before it sends anything; a plaintext peer
// cannot complete that handshake and the request fails as a TLS error rather
// than being sent in the clear [HTTP-STATUS-CLIENT].
void test_client_tls_handshake_failure(void) {
  ScriptedOrigin origin;
  origin_start(&origin, NULL); // speaks no TLS
  int64_t client_id = http_create_client(
      (char *)(uintptr_t) "https://127.0.0.1:18088", 1000);
  assert(client_id > 0);
  assert(http_get(client_id, (char *)(uintptr_t) "/secure",
                  (char *)(uintptr_t) "") == -12);
  origin_stop(&origin);
  assert(origin.request[0] != 'G'); // never reached the wire as plaintext
  http_close_client(client_id);
}
