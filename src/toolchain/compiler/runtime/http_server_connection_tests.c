#include "http_runtime_tests.h"
#include <assert.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>
#include <time.h>

// Connections of the native listener [HTTP-SERVER-CONNECTIONS]: one client
// never holds up another, an HTTP/1.1 connection stays open for its next
// request as Node.js keeps it, and every way a connection ends is answered with
// the `Connection` header Node.js sends for it.

#define CONNECTION_TEST_PORT 18096
#define LOAD_CLIENTS 16
#define LOAD_REQUESTS 50
#define PROMPT_MS 1000
#define HANDLER_WORK_US 100

static const char KEPT[] = "\r\nConnection: keep-alive\r\nKeep-Alive: timeout=5\r\n\r\n";
static const char CLOSED[] = "\r\nConnection: close\r\n\r\n";

static atomic_int g_served;
// Handlers inside at once, and whether two ever were: a handler's externs (a
// shared SQLite handle, a C library's globals) are not known to be thread-safe.
static atomic_int g_inside;
static atomic_bool g_overlapped;

// Read-only after start: every worker may return it at once.
static HttpResponse g_reply = {.status = 200,
                               .headers = (char *)(uintptr_t) "Content-Type: text/plain\r\n",
                               .contentType = (char *)(uintptr_t) "text/plain",
                               .streamFd = -1,
                               .isComplete = true,
                               .partialBody = (char *)(uintptr_t) "served"};

static HttpResponse *counting_handler(char *method, char *path, char *headers, char *body) {
  (void)method, (void)path, (void)headers, (void)body;
  if (atomic_fetch_add(&g_inside, 1) > 0) {
    atomic_store(&g_overlapped, true);
  }
  usleep(HANDLER_WORK_US);
  atomic_fetch_add(&g_served, 1);
  atomic_fetch_sub(&g_inside, 1);
  return &g_reply;
}

static int64_t start_connection_server(void) {
  atomic_store(&g_served, 0);
  atomic_store(&g_overlapped, false);
  int64_t server = http_create_server(CONNECTION_TEST_PORT, (char *)(uintptr_t) "127.0.0.1");
  assert(server > 0 && http_listen(server, counting_handler) == 0);
  return server;
}

static int open_client(void) {
  int fd = server_test_connect(CONNECTION_TEST_PORT);
  struct timeval timeout = {3, 0};
  assert(setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout)) == 0);
  return fd;
}

static void send_text(int fd, const char *text) {
  size_t total = strlen(text);
  for (size_t sent = 0; sent < total;) {
    ssize_t n = send(fd, text + sent, total - sent, 0);
    assert(n > 0);
    sent += (size_t)n;
  }
}

// One whole reply, framed by its Content-Length: the connection may stay open.
static void read_reply(int fd, char *reply, size_t capacity) {
  size_t length = 0;
  char *end = NULL;
  while (!end) {
    assert(length + 1 < capacity);
    ssize_t n = recv(fd, reply + length, 1, 0);
    assert(n == 1 && "the reply's headers must arrive");
    length += 1;
    reply[length] = '\0';
    end = strstr(reply, "\r\n\r\n");
  }
  const char *declared = strstr(reply, "Content-Length: ");
  assert(declared);
  size_t body = strtoul(declared + strlen("Content-Length: "), NULL, 10);
  assert(length + body + 1 < capacity);
  for (size_t got = 0; got < body;) {
    ssize_t n = recv(fd, reply + length + got, body - got, 0);
    assert(n > 0 && "the reply's body must arrive");
    got += (size_t)n;
  }
  reply[length + body] = '\0';
}

static bool closed_by_server(int fd) {
  char byte;
  return recv(fd, &byte, 1, 0) == 0;
}

static uint64_t now_ms(void) {
  struct timespec now;
  clock_gettime(CLOCK_MONOTONIC, &now);
  return (uint64_t)now.tv_sec * 1000U + (uint64_t)now.tv_nsec / 1000000U;
}

static void assert_served(const char *reply, const char *connection) {
  assert(strstr(reply, "HTTP/1.1 200 OK\r\n") == reply);
  assert(strstr(reply, "Content-Length: 6") != NULL);
  assert(strstr(reply, connection) != NULL);
  assert(strstr(reply, "\r\n\r\nserved") != NULL);
}

static void expect_reply(int fd, const char *request, const char *connection) {
  char reply[1024];
  send_text(fd, request);
  read_reply(fd, reply, sizeof(reply));
  assert_served(reply, connection);
}

static void expect_closed_and_finish(int fd) {
  assert(closed_by_server(fd));
  close(fd);
}

// A client that sends half a request holds one connection, never the server.
void test_server_serves_a_client_while_another_is_silent(void) {
  int64_t server = start_connection_server();
  int silent = open_client();
  send_text(silent, "GET /slow HTTP/1.1\r\nHost: x\r\n");
  usleep(50000); // the silent connection is accepted first
  uint64_t started = now_ms();
  int prompt = open_client();
  send_text(prompt, "GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
  char reply[1024];
  read_reply(prompt, reply, sizeof(reply));
  assert(now_ms() - started < PROMPT_MS && "a silent client must not stall the next one");
  assert_served(reply, CLOSED);
  assert(closed_by_server(prompt));
  close(prompt);
  close(silent);
  assert(http_stop_server(server) == 0);
  assert(atomic_load(&g_served) == 1);
}

// HTTP/1.1 keeps the connection for the next request until the client asks to
// close; HTTP/1.0 closes unless the client asks to keep it.
void test_server_keeps_alive_as_node_does(void) {
  int64_t server = start_connection_server();
  int fd = open_client();
  for (int round = 0; round < 3; round++) {
    expect_reply(fd, "GET / HTTP/1.1\r\nHost: x\r\n\r\n", KEPT);
  }
  expect_reply(fd, "GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n", CLOSED);
  expect_closed_and_finish(fd);

  fd = open_client();
  expect_reply(fd, "GET / HTTP/1.0\r\n\r\n", CLOSED);
  expect_closed_and_finish(fd);

  fd = open_client();
  expect_reply(fd, "GET / HTTP/1.0\r\nConnection: keep-alive\r\n\r\n", KEPT);
  expect_reply(fd, "GET / HTTP/1.1\r\nHost: x\r\nconnection: Close\r\n\r\n", CLOSED);
  expect_closed_and_finish(fd);
  assert(http_stop_server(server) == 0);
  assert(atomic_load(&g_served) == 7);
}

// A client that half-closes after its request still gets the whole reply, and
// the connection ends at once; a second request sent ahead of the first reply
// is not answered, and the first reply says the connection closes.
void test_server_ends_half_closed_and_pipelined_connections(void) {
  int64_t server = start_connection_server();
  char reply[1024];
  int fd = open_client();
  send_text(fd, "GET / HTTP/1.1\r\nHost: x\r\n\r\n");
  assert(shutdown(fd, SHUT_WR) == 0);
  uint64_t started = now_ms();
  read_reply(fd, reply, sizeof(reply));
  assert(strstr(reply, "\r\n\r\nserved") != NULL);
  assert(closed_by_server(fd) && now_ms() - started < PROMPT_MS);
  close(fd);

  fd = open_client();
  expect_reply(fd, "GET /a HTTP/1.1\r\nHost: x\r\n\r\nGET /b HTTP/1.1\r\nHost: x\r\n\r\n", CLOSED);
  expect_closed_and_finish(fd);
  assert(http_stop_server(server) == 0);
  assert(atomic_load(&g_served) == 2);
}

// Stopping the server ends a connection that is waiting for its next request.
void test_server_stop_ends_idle_connections(void) {
  int64_t server = start_connection_server();
  char reply[1024];
  int fd = open_client();
  send_text(fd, "GET / HTTP/1.1\r\nHost: x\r\n\r\n");
  read_reply(fd, reply, sizeof(reply));
  assert_served(reply, KEPT);
  uint64_t started = now_ms();
  assert(http_stop_server(server) == 0);
  assert(closed_by_server(fd) && now_ms() - started < PROMPT_MS);
  close(fd);
}

static _Thread_local HttpResponse t_status_reply;

// Answers `/<status>` with that status and no body.
static HttpResponse *status_handler(char *method, char *path, char *headers, char *body) {
  (void)method, (void)headers, (void)body;
  t_status_reply = g_reply;
  t_status_reply.status = strtol(path + 1, NULL, 10);
  t_status_reply.partialBody = (char *)(uintptr_t) "";
  return &t_status_reply;
}

// The reason phrase of every status Node.js names, as Node.js spells it: a
// reply's first line is part of what a client and a byte-for-byte comparison see.
void test_server_names_statuses_as_node_does(void) {
  static const struct {
    int status;
    const char *line;
  } named[] = {
      {200, "HTTP/1.1 200 OK\r\n"},
      {201, "HTTP/1.1 201 Created\r\n"},
      {202, "HTTP/1.1 202 Accepted\r\n"},
      {204, "HTTP/1.1 204 No Content\r\n"},
      {301, "HTTP/1.1 301 Moved Permanently\r\n"},
      {302, "HTTP/1.1 302 Found\r\n"},
      {304, "HTTP/1.1 304 Not Modified\r\n"},
      {400, "HTTP/1.1 400 Bad Request\r\n"},
      {401, "HTTP/1.1 401 Unauthorized\r\n"},
      {403, "HTTP/1.1 403 Forbidden\r\n"},
      {404, "HTTP/1.1 404 Not Found\r\n"},
      {409, "HTTP/1.1 409 Conflict\r\n"},
      {410, "HTTP/1.1 410 Gone\r\n"},
      {413, "HTTP/1.1 413 Payload Too Large\r\n"},
      {422, "HTTP/1.1 422 Unprocessable Entity\r\n"},
      {415, "HTTP/1.1 415 Unsupported Media Type\r\n"},
      {429, "HTTP/1.1 429 Too Many Requests\r\n"},
      {500, "HTTP/1.1 500 Internal Server Error\r\n"},
      {501, "HTTP/1.1 501 Not Implemented\r\n"},
      {502, "HTTP/1.1 502 Bad Gateway\r\n"},
      {503, "HTTP/1.1 503 Service Unavailable\r\n"},
      {504, "HTTP/1.1 504 Gateway Timeout\r\n"},
  };
  int64_t server = http_create_server(CONNECTION_TEST_PORT, (char *)(uintptr_t) "127.0.0.1");
  assert(server > 0 && http_listen(server, status_handler) == 0);
  int fd = open_client();
  for (size_t each = 0; each < sizeof(named) / sizeof(named[0]); each++) {
    char request[64];
    char reply[1024];
    (void)snprintf(request, sizeof(request), "GET /%d HTTP/1.1\r\nHost: x\r\n\r\n", named[each].status);
    send_text(fd, request);
    read_reply(fd, reply, sizeof(reply));
    assert(strstr(reply, named[each].line) == reply && "the status line must carry Node's reason phrase");
  }
  close(fd);
  assert(http_stop_server(server) == 0);
}

static void *keep_alive_client(void *unused) {
  (void)unused;
  char reply[1024];
  int fd = open_client();
  for (int request = 0; request < LOAD_REQUESTS; request++) {
    send_text(fd, "GET /load HTTP/1.1\r\nHost: x\r\n\r\n");
    read_reply(fd, reply, sizeof(reply));
    assert_served(reply, KEPT);
  }
  close(fd);
  return NULL;
}

// Many clients at once, each reusing its connection: every request is served
// exactly once, and one handler runs at a time.
void test_server_serves_concurrent_keep_alive_clients(void) {
  int64_t server = start_connection_server();
  pthread_t loaders[LOAD_CLIENTS];
  for (int each = 0; each < LOAD_CLIENTS; each++) {
    assert(pthread_create(&loaders[each], NULL, keep_alive_client, NULL) == 0);
  }
  for (int each = 0; each < LOAD_CLIENTS; each++) {
    assert(pthread_join(loaders[each], NULL) == 0);
  }
  assert(http_stop_server(server) == 0);
  assert(atomic_load(&g_served) == LOAD_CLIENTS * LOAD_REQUESTS);
  assert(!atomic_load(&g_overlapped) && "handlers must never run at once");
}
