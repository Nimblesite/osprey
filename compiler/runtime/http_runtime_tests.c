#include "http_server_internal.h"
#include "http_shared.h"
#include <assert.h>

// Include all runtime modules
extern int64_t http_create_client(char *base_url, int64_t timeout);
extern int64_t http_close_client(int64_t client_id);
extern int64_t http_get(int64_t client_id, char *path, char *headers);
extern int64_t http_post(int64_t client_id, char *path, char *body,
                         char *headers);
extern int64_t http_put(int64_t client_id, char *path, char *body,
                        char *headers);
extern int64_t http_delete(int64_t client_id, char *path, char *headers);

extern int64_t http_create_server(int64_t port, char *address);
extern int64_t http_listen(int64_t server_id, HttpRequestHandler handler);

// Defined with the live-server tests below; the lifecycle test needs a real
// callback now that http_listen is declared with its true signature.
static HttpResponse *server_test_handler(char *method, char *path,
                                         char *headers, char *body);
extern int64_t http_stop_server(int64_t server_id);


extern int64_t http_get_response(int64_t client_id, char *path, char *headers);
extern int64_t http_response_status(int64_t handle);
extern char *http_response_body(int64_t handle);
extern char *http_response_header(int64_t handle, char *name);
extern int64_t http_response_free(int64_t handle);

void test_http_create_client(void) {
  printf("Testing http_create_client...\n");

  // Test valid client creation
  int64_t client_id = http_create_client("http://example.com:8080", 5000);
  assert(client_id > 0);
  printf("✅ Created client with ID: %" PRId64 "\n", client_id);

  // Test invalid URL
  int64_t invalid_client = http_create_client(NULL, 5000);
  assert(invalid_client < 0);
  printf("✅ Correctly rejected NULL URL\n");

  // Test invalid timeout
  int64_t timeout_client = http_create_client("http://example.com", -1);
  assert(timeout_client < 0);
  printf("✅ Correctly rejected negative timeout\n");

  // Clean up
  http_close_client(client_id);
  printf("✅ http_create_client tests passed!\n\n");
}

void test_http_create_server(void) {
  printf("Testing http_create_server...\n");

  // Test valid server creation
  int64_t server_id = http_create_server(8080, "127.0.0.1");
  assert(server_id > 0);
  printf("✅ Created server with ID: %" PRId64 "\n", server_id);

  // Test invalid port
  int64_t invalid_server = http_create_server(0, "127.0.0.1");
  assert(invalid_server < 0);
  printf("✅ Correctly rejected invalid port\n");

  // Test invalid address
  int64_t addr_server = http_create_server(8081, NULL);
  assert(addr_server < 0);
  printf("✅ Correctly rejected NULL address\n");

  // Clean up
  http_stop_server(server_id);
  printf("✅ http_create_server tests passed!\n\n");
}

void test_http_server_lifecycle(void) {
  printf("Testing HTTP server lifecycle...\n");

  // Create server
  int64_t server_id = http_create_server(8082, "127.0.0.1");
  assert(server_id > 0);
  printf("✅ Server created: %" PRId64 "\n", server_id);

  // Start listening
  int64_t listen_result = http_listen(server_id, server_test_handler);
  assert(listen_result == 0);
  printf("✅ Server listening started\n");

  // Stop server
  int64_t stop_result = http_stop_server(server_id);
  assert(stop_result == 0);
  printf("✅ Server stopped successfully\n");

  printf("✅ HTTP server lifecycle tests passed!\n\n");
}

// Every RequestReadStatus maps to its exact rejection status and JSON body.
void test_rejection_mapping(void) {
  assert(http_rejection_status(REQUEST_TOO_LARGE) == 413);
  assert(http_rejection_status(REQUEST_TIMED_OUT) == 408);
  assert(http_rejection_status(REQUEST_MALFORMED) == 400);
  assert(http_rejection_status(REQUEST_INCOMPLETE) == 400);
  assert(http_rejection_status(REQUEST_READ_FAILED) == 400);
  assert(http_rejection_status(REQUEST_UNSUPPORTED_FRAMING) == 400);
  assert(strcmp(http_rejection_body(REQUEST_TOO_LARGE),
                "{\"error\":\"HTTP request too large\"}") == 0);
  assert(strcmp(http_rejection_body(REQUEST_TIMED_OUT),
                "{\"error\":\"HTTP request timed out\"}") == 0);
  assert(strcmp(http_rejection_body(REQUEST_UNSUPPORTED_FRAMING),
                "{\"error\":\"unsupported HTTP transfer encoding\"}") == 0);
  assert(strcmp(http_rejection_body(REQUEST_MALFORMED),
                "{\"error\":\"malformed HTTP request\"}") == 0);
  assert(strcmp(http_rejection_body(REQUEST_READ_FAILED),
                "{\"error\":\"malformed HTTP request\"}") == 0);
}

// sanitize_log_token maps every byte outside '!'..'~' to '_', truncates at
// capacity-1, always terminates, and leaves a zero-capacity buffer untouched.
void test_sanitize_log_token(void) {
  char out[16];
  sanitize_log_token("GET", out, sizeof(out));
  assert(strcmp(out, "GET") == 0);
  sanitize_log_token("a b\tc\r\nd", out, sizeof(out));
  assert(strcmp(out, "a_b_c__d") == 0);
  char hostile[] = {'x', 0x1b, 0x7f, (char)0x80, '!', '~', ' ', '\0'};
  sanitize_log_token(hostile, out, sizeof(out));
  assert(strcmp(out, "x___!~_") == 0);
  sanitize_log_token("abcdefghij", out, 4); // truncates to capacity-1
  assert(strcmp(out, "abc") == 0);
  sanitize_log_token(NULL, out, sizeof(out));
  assert(out[0] == '\0');
  char untouched = 'Z';
  sanitize_log_token("anything", &untouched, 0);
  assert(untouched == 'Z');
}

// http_socket_interrupted reflects errno EXACTLY — EINTR and nothing else.
void test_socket_interrupted(void) {
  errno = EINTR;
  assert(http_socket_interrupted() == true);
  errno = 0;
  assert(http_socket_interrupted() == false);
  errno = EAGAIN;
  assert(http_socket_interrupted() == false);
}

// Response handles: invalid and freed handles are rejected with -1/NULL, and
// a slot frees exactly once.
void test_response_handle_rejection(void) {
  assert(http_response_status(-1) == -1);
  assert(http_response_status(999999) == -1);
  assert(http_response_body(-1) == NULL);
  assert(http_response_header(-1, (char *)(uintptr_t) "X") == NULL);
  assert(http_response_free(-1) == -1);
  assert(http_response_free(999999) == -1);
  assert(http_get_response(-1, (char *)(uintptr_t) "/", (char *)(uintptr_t) "") <
         0); // invalid client id
}

// In-process loopback exchange: a REAL server thread, a REAL client request,
// and byte-exact status/body/header assertions through the response handle
// accessors. Runs LAST: it exercises the full request path end to end.
static HttpResponse g_live_response;

static HttpResponse *live_handler(char *method, char *path, char *headers,
                                  char *body) {
  (void)headers;
  (void)body;
  assert(strcmp(method, "GET") == 0);
  assert(strcmp(path, "/ping") == 0);
  g_live_response.status = 200;
  g_live_response.headers = (char *)(uintptr_t) "X-Test: yes\r\n";
  g_live_response.contentType = (char *)(uintptr_t) "text/plain";
  g_live_response.streamFd = -1;
  g_live_response.isComplete = true;
  g_live_response.partialBody = (char *)(uintptr_t) "pong";
  return &g_live_response;
}

#define LIVE_PORT 18085
#define LIVE_TRIES 50

void test_live_loopback_exchange(void) {
  printf("Testing live loopback HTTP exchange...\n");
  int64_t server_id = http_create_server(LIVE_PORT, (char *)(uintptr_t) "127.0.0.1");
  assert(server_id > 0);
  assert(http_listen(server_id, live_handler) == 0);
  int64_t client_id =
      http_create_client((char *)(uintptr_t) "http://127.0.0.1:18085", 5000);
  assert(client_id > 0);
  int64_t handle = -1;
  for (int i = 0; i < LIVE_TRIES && handle < 0; i++) {
    handle = http_get_response(client_id, (char *)(uintptr_t) "/ping",
                               (char *)(uintptr_t) "");
    if (handle < 0) {
      usleep(100000); // listener thread may still be binding
    }
  }
  assert(handle >= 0);
  assert(http_response_status(handle) == 200);
  char *body = http_response_body(handle);
  assert(body != NULL && strcmp(body, "pong") == 0);
  free(body);
  char *header = http_response_header(handle, (char *)(uintptr_t) "X-Test");
  assert(header != NULL && strcmp(header, "yes") == 0);
  free(header);
  char *missing = http_response_header(handle, (char *)(uintptr_t) "X-Absent");
  assert(missing == NULL);
  assert(http_response_free(handle) == 0);
  assert(http_response_free(handle) == -1); // double free rejected
  assert(http_response_body(handle) == NULL);
  http_close_client(client_id);
  http_stop_server(server_id);
  printf("✅ Live loopback exchange passed!\n\n");
}

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
void test_client_handle_bounds(void) {
  assert(http_get(-1, (char *)(uintptr_t) "/x", (char *)(uintptr_t) "") == -1);
  assert(http_get(0, (char *)(uintptr_t) "/x", (char *)(uintptr_t) "") == -1);
  assert(http_get(MAX_CLIENTS, (char *)(uintptr_t) "/x",
                  (char *)(uintptr_t) "") == -1);
  assert(http_get(MAX_CLIENTS + 4096, (char *)(uintptr_t) "/x",
                  (char *)(uintptr_t) "") == -1);
  assert(http_close_client(-1) == -1);
  assert(http_close_client(MAX_CLIENTS + 4096) == -1);
  assert(http_get(MAX_CLIENTS - 1, (char *)(uintptr_t) "/x",
                  (char *)(uintptr_t) "") == -1); // in range, empty slot
  assert(http_close_client(MAX_CLIENTS - 1) == 0); // closing one is a no-op
}

// get_next_id() is a process-global counter shared with servers and sockets.
// Once it passes MAX_CLIENTS the table cannot hold the handle, and creation
// must SAY SO instead of writing past the end of clients[]. Runs LAST: it
// burns the shared id space that every other handle type draws from.
void test_client_handle_exhaustion(void) {
  int64_t last = 0;
  for (int i = 0; i < MAX_CLIENTS + 8; i++) {
    last = http_create_client((char *)(uintptr_t) "http://127.0.0.1:1", 1);
    if (last < 0) {
      break;
    }
    assert(http_close_client(last) == 0);
  }
  assert(last == -5); // handle table exhausted, nothing written out of bounds
  assert(http_create_client((char *)(uintptr_t) "http://127.0.0.1:1", 1) == -5);
}

// A body larger than the initial receive buffer forces the drain loop to grow
// its buffer; the payload must survive intact [HTTP-RESPONSE-HANDLE].
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

// ---------------------------------------------------------------------------
// Live server: raw clients against a REAL listener, so the reject path, the
// self-stop path and the listen/stop state machine are asserted end to end
// [HTTP-SERVER].
// ---------------------------------------------------------------------------

#define SERVER_TEST_PORT 18089

static int64_t g_self_stop_server = -1;

// Connects to the live listener, writes `request` verbatim, and returns the
// whole reply. The server sets Connection: close, so read-to-EOF is the reply.
static void server_exchange(const char *request, char *reply, size_t capacity) {
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_port = htons(SERVER_TEST_PORT);
  addr.sin_addr.s_addr = inet_addr("127.0.0.1");
  int fd = -1;
  for (int i = 0; i < 100 && fd < 0; i++) {
    fd = socket(AF_INET, SOCK_STREAM, 0);
    assert(fd >= 0);
    if (connect(fd, (struct sockaddr *)&addr, sizeof(addr)) != 0) {
      close(fd);
      fd = -1;
      usleep(20000); // the listener fiber may still be binding
    }
  }
  assert(fd >= 0);
  struct timeval tv = {10, 0};
  assert(setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv)) == 0);
  size_t total = strlen(request);
  size_t sent = 0;
  while (sent < total) {
    ssize_t n = send(fd, request + sent, total - sent, 0);
    assert(n > 0);
    sent += (size_t)n;
  }
  size_t len = 0;
  while (len + 1 < capacity) {
    ssize_t n = recv(fd, reply + len, capacity - 1 - len, 0);
    if (n <= 0) {
      break;
    }
    len += (size_t)n;
  }
  reply[len] = '\0';
  close(fd);
}

static HttpResponse g_server_test_response;

static HttpResponse *server_test_handler(char *method, char *path,
                                         char *headers, char *body) {
  (void)headers;
  (void)body;
  g_server_test_response.status = 200;
  g_server_test_response.headers = (char *)(uintptr_t) "Content-Type: text/plain\r\n";
  g_server_test_response.contentType = (char *)(uintptr_t) "text/plain";
  g_server_test_response.streamFd = -1;
  g_server_test_response.isComplete = true;
  g_server_test_response.partialBody =
      strcmp(method, "GET") == 0 && strcmp(path, "/stop") == 0
          ? (char *)(uintptr_t) "stopping"
          : (char *)(uintptr_t) "served";
  return &g_server_test_response;
}

// A request the reader refuses never reaches the handler: it is answered with
// the rejection status and its JSON body, and the exchange is still logged
// with a labelled method and path [HTTP-SERVER].
void test_server_rejects_bad_requests(void) {
  int64_t server_id =
      http_create_server(SERVER_TEST_PORT, (char *)(uintptr_t) "127.0.0.1");
  assert(server_id > 0);
  assert(http_listen(server_id, server_test_handler) == 0);
  // Listening twice on one server is refused, and so is a second bind of the
  // same port by another server.
  assert(http_listen(server_id, server_test_handler) == -7);
  int64_t rival =
      http_create_server(SERVER_TEST_PORT, (char *)(uintptr_t) "127.0.0.1");
  assert(rival > 0);
  assert(http_listen(rival, server_test_handler) == -4); // port already bound
  assert(http_stop_server(rival) == 0);

  char reply[2048];
  server_exchange("GET /ok HTTP/1.1\r\nHost: x\r\n\r\n", reply, sizeof(reply));
  assert(strstr(reply, "HTTP/1.1 200") == reply);
  assert(strstr(reply, "served") != NULL);

  server_exchange("GET /chunk HTTP/1.1\r\nHost: x\r\n"
                  "Transfer-Encoding: chunked\r\n\r\n",
                  reply, sizeof(reply));
  assert(strstr(reply, "HTTP/1.1 400") == reply);
  assert(strstr(reply, "unsupported HTTP transfer encoding") != NULL);

  server_exchange("GET /bad HTTP/1.1\r\nHost: x\r\n"
                  "Content-Length: not-a-number\r\n\r\n",
                  reply, sizeof(reply));
  assert(strstr(reply, "HTTP/1.1 400") == reply);
  assert(strstr(reply, "malformed HTTP request") != NULL);

  // A declared body far past the cap is refused before a byte of it is read.
  server_exchange("POST /huge HTTP/1.1\r\nHost: x\r\n"
                  "Content-Length: 99999999\r\n\r\n",
                  reply, sizeof(reply));
  assert(strstr(reply, "HTTP/1.1 413") == reply);
  assert(strstr(reply, "HTTP request too large") != NULL);

  assert(http_stop_server(server_id) == 0);
}

// A handler may stop its own server. The listener fiber then owns the teardown
// and the stop call must not wait on itself [HTTP-SERVER].
static HttpResponse *self_stopping_handler(char *method, char *path,
                                           char *headers, char *body) {
  (void)method;
  (void)path;
  (void)headers;
  (void)body;
  assert(http_stop_server(g_self_stop_server) == 0);
  g_server_test_response.status = 200;
  g_server_test_response.headers = (char *)(uintptr_t) "Content-Type: text/plain\r\n";
  g_server_test_response.contentType = (char *)(uintptr_t) "text/plain";
  g_server_test_response.streamFd = -1;
  g_server_test_response.isComplete = true;
  g_server_test_response.partialBody = (char *)(uintptr_t) "self-stopped";
  return &g_server_test_response;
}

void test_server_self_stop_from_handler(void) {
  g_self_stop_server =
      http_create_server(SERVER_TEST_PORT, (char *)(uintptr_t) "127.0.0.1");
  assert(g_self_stop_server > 0);
  assert(http_listen(g_self_stop_server, self_stopping_handler) == 0);
  char reply[1024];
  server_exchange("GET /stop HTTP/1.1\r\nHost: x\r\n\r\n", reply,
                  sizeof(reply));
  assert(strstr(reply, "HTTP/1.1 200") == reply);
  assert(strstr(reply, "self-stopped") != NULL);
  // The handle is already gone: stopping it again is a no-op, not a double
  // free, and the port comes back.
  assert(http_stop_server(g_self_stop_server) == 0);
  int64_t reborn =
      http_create_server(SERVER_TEST_PORT, (char *)(uintptr_t) "127.0.0.1");
  assert(reborn > 0);
  assert(http_listen(reborn, server_test_handler) == 0);
  assert(http_stop_server(reborn) == 0);
}

// Server handles outside the table are rejected; one inside it that names no
// server is a no-op [HTTP-SERVER].
void test_server_handle_bounds(void) {
  assert(http_listen(-1, server_test_handler) == -1);
  assert(http_listen(0, server_test_handler) == -1);
  assert(http_listen(MAX_SERVERS, server_test_handler) == -1);
  assert(http_listen(MAX_SERVERS - 1, server_test_handler) == -1);
  assert(http_stop_server(-1) == -1);
  assert(http_stop_server(0) == -1);
  assert(http_stop_server(MAX_SERVERS) == -1);
  assert(http_stop_server(MAX_SERVERS - 1) == 0); // in range, no server
}

void run_all_http_tests(void) {
  printf("🧪 Starting HTTP Runtime Test Suite\n");
  printf("=====================================\n\n");

  test_http_create_client();
  test_http_create_server();
  test_http_server_lifecycle();
  test_rejection_mapping();
  test_sanitize_log_token();
  test_socket_interrupted();
  test_response_handle_rejection();
  test_client_chunked_response();
  test_client_status_line_handling();
  test_client_methods_and_body();
  test_client_transport_failures();
  test_client_large_response();
  test_client_malformed_chunks();
  test_client_tls_handshake_failure();
  test_client_handle_bounds();
  test_server_rejects_bad_requests();
  test_server_self_stop_from_handler();
  test_server_handle_bounds();
  test_live_loopback_exchange();    // full end-to-end request path
  test_client_handle_exhaustion();  // LAST: burns the shared id space

  printf("🎉 All HTTP runtime tests passed!\n");
  printf("=====================================\n");
}

int main(void) {
  run_all_http_tests();
  return 0;
}
