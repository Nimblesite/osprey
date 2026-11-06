// Closure-bridge and connected-client teardown tests, split from
// http_server_live_tests.c: a raw loopback client against a listener installed
// through http_listen_closure, and a stop from another thread while a client is
// still connected [HTTP-SERVER].
#include "http_runtime_tests.h"
#include "http_shared.h"

#include <assert.h>

// ---------------------------------------------------------------------------
// Closure bridge: an Osprey function value reaches the C listener as a closure
// record, and the bridge reads its handler from the record's FIRST word
// [HTTP-SERVER]. This test closure is laid out the same way and carries the
// request the handler saw, so every exchange proves the whole record — not a
// bare function pointer — crossed.
// ---------------------------------------------------------------------------

#define CLOSURE_PORT 18100
#define CLOSURE_HELD_PORT 18101
#define CLOSURE_HEADERS "Content-Type: application/json\r\nX-Closure: yes\r\n"

extern int64_t http_listen_closure(int64_t server_id, void *closure);

typedef HttpResponse *(*ClosureHandlerFn)(void *, char *, char *, char *,
                                          char *);

typedef struct {
  ClosureHandlerFn handler; // first word: what call_handler invokes
  int calls;
  char method[16];
  char path[256];
  char headers[256];
  char body[64];
} ClosureRecord;

static HttpResponse g_closure_response;
static char g_closure_body[128];
static ClosureRecord g_closure;

static void record_closure_call(ClosureRecord *record, char *method,
                                char *path, char *headers, char *body) {
  record->calls++;
  snprintf(record->method, sizeof(record->method), "%s", method);
  snprintf(record->path, sizeof(record->path), "%s", path);
  snprintf(record->headers, sizeof(record->headers), "%s", headers);
  snprintf(record->body, sizeof(record->body), "%s", body);
}

static HttpResponse *closure_handler(void *raw, char *method, char *path,
                                     char *headers, char *body) {
  ClosureRecord *record = raw;
  assert(record == &g_closure && "the handler sees its own closure record");
  record_closure_call(record, method, path, headers, body);
  g_closure_response.status = strcmp(path, "/created") == 0 ? 201 : 200;
  g_closure_response.headers = (char *)(uintptr_t)CLOSURE_HEADERS;
  g_closure_response.contentType = (char *)(uintptr_t) "application/json";
  g_closure_response.streamFd = -1;
  g_closure_response.isComplete = true;
  snprintf(g_closure_body, sizeof(g_closure_body), "seen:%s%s", method, path);
  g_closure_response.partialBody = g_closure_body;
  return &g_closure_response;
}

// The reply carries the closure's status line, its headers, and its body with
// a Content-Length that matches the body byte for byte.
static void assert_closure_reply(const char *reply, const char *body) {
  char length[64];
  snprintf(length, sizeof(length), "Content-Length: %zu\r\n", strlen(body));
  assert(strstr(reply, "X-Closure: yes\r\n") != NULL &&
         "the closure's header block is on the wire");
  assert(strstr(reply, "Content-Type: application/json\r\n") != NULL);
  assert(strstr(reply, length) != NULL && "the length is the body sent");
  const char *sent_body = strstr(reply, "\r\n\r\n");
  assert(sent_body != NULL && strcmp(sent_body + 4, body) == 0 &&
         "and the body is exactly the closure's");
}

// One closure exchange: the labelled reply, and the handler's own count.
static void expect_closure_call(const char *request, const char *status_line,
                                const char *body, int calls) {
  char reply[2048];
  server_test_exchange(CLOSURE_PORT, request, reply, sizeof(reply));
  assert(strstr(reply, status_line) == reply && "the status line comes first");
  assert_closure_reply(reply, body);
  assert(g_closure.calls == calls && "one request, one handler call");
}

// The GET arm: the handler word, the method, the path and the headers cross,
// and a request without a body arrives with an empty one.
static void expect_closure_get(void) {
  expect_closure_call("GET /created HTTP/1.1\r\nHost: x\r\n\r\n",
                      "HTTP/1.1 201 Created", "seen:GET/created", 1);
  assert(strcmp(g_closure.method, "GET") == 0);
  assert(strcmp(g_closure.path, "/created") == 0);
  assert(strstr(g_closure.headers, "Host: x") != NULL);
  assert(strcmp(g_closure.body, "") == 0 && "a bodyless GET has no body");
}

// The POST arm: the body and its framing headers cross the bridge too.
static void expect_closure_post(void) {
  expect_closure_call("POST /echo HTTP/1.1\r\nHost: x\r\nX-In: 1\r\n"
                      "Content-Length: 5\r\n\r\nhello",
                      "HTTP/1.1 200 OK", "seen:POST/echo", 2);
  assert(strcmp(g_closure.method, "POST") == 0);
  assert(strstr(g_closure.headers, "X-In: 1") != NULL);
  assert(strstr(g_closure.headers, "Content-Length: 5") != NULL);
  assert(strcmp(g_closure.body, "hello") == 0 && "the POST body crossed too");
}

// A closure listener answers real clients, and every field of the request
// crosses the bridge on repeated exchanges [HTTP-SERVER].
void test_closure_listener_serves_requests(void) {
  memset(&g_closure, 0, sizeof(g_closure));
  g_closure.handler = closure_handler;
  int64_t server_id =
      http_create_server(CLOSURE_PORT, (char *)(uintptr_t) "127.0.0.1");
  assert(server_id > 0);
  assert(http_listen_closure(server_id, &g_closure) == 0);
  expect_closure_get();
  expect_closure_post();
  assert(http_stop_server(server_id) == 0);
}

// An id outside the table, an in-range id that names no server, and a NULL
// closure are all -1: the bridge has no other failure vocabulary.
static void expect_closure_handle_refusals(int64_t server_id) {
  assert(http_listen_closure(-1, &g_closure) == -1);
  assert(http_listen_closure(0, &g_closure) == -1);
  assert(http_listen_closure(MAX_SERVERS, &g_closure) == -1);
  assert(http_listen_closure(MAX_SERVERS - 1, &g_closure) == -1);
  assert(http_listen_closure(server_id, NULL) == -1 && "no closure, no listen");
}

// Listening twice is -1 and a rival server's bind of the live port is -4; the
// failed attempts register no closure and leave the live listener alone.
static void expect_closure_listen_conflicts(int64_t server_id) {
  assert(http_listen_closure(server_id, &g_closure) == 0);
  assert(http_listen_closure(server_id, &g_closure) == -1 &&
         "a second listen on the same server is refused");
  int64_t rival =
      http_create_server(CLOSURE_PORT, (char *)(uintptr_t) "127.0.0.1");
  assert(rival > 0);
  assert(http_listen_closure(rival, &g_closure) == -4); // the port is taken
  assert(http_stop_server(rival) == 0);
}

// The closure bridge shares the handle table and its refusals, and the live
// listener still answers after every one of them [HTTP-SERVER].
void test_closure_listen_rejections(void) {
  memset(&g_closure, 0, sizeof(g_closure));
  g_closure.handler = closure_handler;
  int64_t server_id =
      http_create_server(CLOSURE_PORT, (char *)(uintptr_t) "127.0.0.1");
  assert(server_id > 0);
  expect_closure_handle_refusals(server_id);
  expect_closure_listen_conflicts(server_id);
  expect_closure_call("GET /created HTTP/1.1\r\nHost: x\r\n\r\n",
                      "HTTP/1.1 201 Created", "seen:GET/created", 1);
  assert(http_stop_server(server_id) == 0);
}

// Reads the live server's claimed-client slot under its own mutex, the way the
// runtime publishes it; the field is the one a stop from another thread tears
// down [HTTP-SERVER].
static bool client_claimed(HttpServer *server) {
  pthread_mutex_lock(&server->mutex);
  bool claimed = server->active_client_fd >= 0;
  pthread_mutex_unlock(&server->mutex);
  return claimed;
}

// Waits until the listener has accepted and claimed the paused client, which
// is the state the foreign stop acts on.
static void await_claimed_client(int64_t server_id) {
  for (int i = 0; i < 200 && !client_claimed(servers[server_id]); i++) {
    usleep(10000);
  }
  assert(client_claimed(servers[server_id]) &&
         "the listener must accept and claim the client");
}

// The client waited on by a server stopped from another thread sees EOF, not a
// hang: the foreign stop tears its socket down [HTTP-SERVER].
static void expect_client_read_ends(int client_fd) {
  char leftover[256];
  server_test_read_to_eof(client_fd, leftover, sizeof(leftover));
  assert(strcmp(leftover, "") == 0 &&
         "a foreign stop tears the client socket down: the wait ends in EOF");
}

static void expect_closure_port_reusable(void) {
  int64_t reborn = http_create_server(CLOSURE_HELD_PORT,
                                      (char *)(uintptr_t) "127.0.0.1");
  assert(reborn > 0);
  assert(http_listen(reborn, http_test_server_handler) == 0 &&
         "the listener socket closed with the server");
  assert(http_stop_server(reborn) == 0);
}

// Stopping a server from another thread while a client is connected shuts the
// CLIENT socket down as well as the listener, so the blocked read returns, the
// loop fiber exits, and the port comes back [HTTP-SERVER].
void test_stop_server_with_client_connected(void) {
  int64_t server_id =
      http_create_server(CLOSURE_HELD_PORT, (char *)(uintptr_t) "127.0.0.1");
  assert(server_id > 0);
  assert(http_listen(server_id, http_test_server_handler) == 0);
  int client_fd = server_test_connect(CLOSURE_HELD_PORT);
  // A header block with no terminator: the server cannot finish the request,
  // so it is still blocked on this client when the stop arrives.
  const char *partial = "POST /held HTTP/1.1\r\nHost: x\r\n";
  assert(send(client_fd, partial, strlen(partial), 0) ==
         (ssize_t)strlen(partial));
  await_claimed_client(server_id);
  assert(http_stop_server(server_id) == 0 && "the stop must not hang");
  expect_client_read_ends(client_fd);
  close(client_fd);
  expect_closure_port_reusable();
}
