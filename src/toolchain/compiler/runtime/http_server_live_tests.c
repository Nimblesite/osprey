// Live-server tests, split from http_runtime_tests.c: raw clients against a
// REAL listener, so the reject path, the self-stop path and the listen/stop
// state machine are asserted end to end [HTTP-SERVER].
#include "http_runtime_tests.h"
#include "http_server_internal.h"
#include "http_shared.h"

#include <assert.h>
#include <pthread.h>
#ifndef _WIN32
#include <sys/resource.h>
#endif

// ---------------------------------------------------------------------------
// Live server: raw clients against a REAL listener, so the reject path, the
// self-stop path and the listen/stop state machine are asserted end to end
// [HTTP-SERVER].
// ---------------------------------------------------------------------------

#define SERVER_TEST_PORT 18089

static int64_t g_self_stop_server = -1;

// Raw-client helpers shared with http_server_closure_tests.c, which drives its
// own listeners on other ports. This one connects to a live listener on `port`,
// retrying while the listener fiber binds; every reply sets Connection: close,
// so read-to-EOF is the whole reply.
int server_test_connect(int port) {
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_port = htons(port);
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
  return fd;
}

// Reads `fd` to EOF, or until the reply buffer is full.
void server_test_read_to_eof(int fd, char *out, size_t capacity) {
  size_t len = 0;
  while (len + 1 < capacity) {
    ssize_t n = recv(fd, out + len, capacity - 1 - len, 0);
    if (n <= 0) {
      break;
    }
    len += (size_t)n;
  }
  out[len] = '\0';
}

// Writes `request` verbatim to the listener on `port` and captures the reply.
void server_test_exchange(int port, const char *request, char *reply,
                          size_t capacity) {
  int fd = server_test_connect(port);
  struct timeval tv = {10, 0};
  assert(setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv)) == 0);
  size_t total = strlen(request);
  size_t sent = 0;
  while (sent < total) {
    ssize_t n = send(fd, request + sent, total - sent, 0);
    assert(n > 0);
    sent += (size_t)n;
  }
  server_test_read_to_eof(fd, reply, capacity);
  close(fd);
}

// One exchange with the shared live test server.
static void server_exchange(const char *request, char *reply,
                            size_t capacity) {
  server_test_exchange(SERVER_TEST_PORT, request, reply, capacity);
}

static HttpResponse g_server_test_response;

HttpResponse *http_test_server_handler(char *method, char *path,
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
  assert(http_listen(server_id, http_test_server_handler) == 0);
  // Listening twice on one server is refused, and so is a second bind of the
  // same port by another server.
  assert(http_listen(server_id, http_test_server_handler) == -7);
  int64_t rival =
      http_create_server(SERVER_TEST_PORT, (char *)(uintptr_t) "127.0.0.1");
  assert(rival > 0);
  assert(http_listen(rival, http_test_server_handler) == -4); // port already bound
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
  assert(http_listen(reborn, http_test_server_handler) == 0);
  assert(http_stop_server(reborn) == 0);
}

// Server handles outside the table are rejected; one inside it that names no
// server is a no-op [HTTP-SERVER].
void test_server_handle_bounds(void) {
  assert(http_listen(-1, http_test_server_handler) == -1);
  assert(http_listen(0, http_test_server_handler) == -1);
  assert(http_listen(MAX_SERVERS, http_test_server_handler) == -1);
  assert(http_listen(MAX_SERVERS - 1, http_test_server_handler) == -1);
  assert(http_stop_server(-1) == -1);
  assert(http_stop_server(0) == -1);
  assert(http_stop_server(MAX_SERVERS) == -1);
  assert(http_stop_server(MAX_SERVERS - 1) == 0); // in range, no server
}

extern int64_t fiber_spawn_env(int64_t (*fn)(void *), void *env);
extern int64_t fiber_await(int64_t fiber_id);

// A request whose line yields no method and no path is still LOGGED, with both
// fields carrying HTTP_LOG_UNKNOWN rather than left empty. An empty field in a
// log line is indistinguishable from a missing one, and the rejection log is
// the only record that the exchange happened at all [HTTP-SERVER].
// Run one exchange with this process's stderr on a pipe, and return what the
// server logged. The log line is written by the server fiber inside THIS
// process, so a pipe on fd 2 is enough to read it; one line is far short of the
// pipe buffer, so the writer cannot block.
static void exchange_capturing_log(const char *request, char *reply,
                                   size_t reply_size, char *log,
                                   size_t log_size) {
  int pipe_fds[2];
  assert(pipe(pipe_fds) == 0);
  fflush(stderr);
  int saved = dup(STDERR_FILENO);
  assert(saved >= 0);
  assert(dup2(pipe_fds[1], STDERR_FILENO) == STDERR_FILENO);
  assert(close(pipe_fds[1]) == 0);

  server_exchange(request, reply, reply_size);

  fflush(stderr);
  assert(dup2(saved, STDERR_FILENO) == STDERR_FILENO);
  assert(close(saved) == 0); // the last write end; the read below cannot block
  ssize_t got = read(pipe_fds[0], log, log_size - 1);
  log[got > 0 ? (size_t)got : 0] = '\0';
  assert(close(pipe_fds[0]) == 0);
}

void test_server_labels_an_unparseable_request(void) {
  int64_t server_id =
      http_create_server(SERVER_TEST_PORT, (char *)(uintptr_t) "127.0.0.1");
  assert(server_id > 0);
  assert(http_listen(server_id, http_test_server_handler) == 0);

  // A header block with no request line at all: `sscanf` finds neither token.
  char reply[1024];
  char log[1024];
  exchange_capturing_log("\r\n\r\n", reply, sizeof(reply), log, sizeof(log));
  assert(strstr(reply, "HTTP/1.1 400") == reply);
  assert(strstr(reply, "malformed HTTP request") != NULL);

  // The LABELLING this test is named for, asserted rather than implied. An
  // operator reading the log has only these tokens to tell an unparseable
  // request from one that simply asked for `/`, and a placeholder that goes
  // missing makes those two look identical [HTTP-SERVER].
  assert(strstr(log, "[http] request_id=") != NULL &&
         "the exchange is logged at all");
  assert(strstr(log, "read=malformed") != NULL &&
         "an unparseable read is labelled malformed, never complete");
  assert(strstr(log, "status=400") != NULL && "and carries the status sent");
  // The trailing space matters: `path=- ` is the placeholder, `path= ` is the
  // empty field the query stripper once produced by eating the placeholder.
  assert(strstr(log, "method=- ") != NULL &&
         "a request line with no method logs the placeholder");
  assert(strstr(log, "path=- ") != NULL &&
         "and the path placeholder survives the query stripper");

  assert(http_stop_server(server_id) == 0);
}

// When the listener fiber cannot be spawned, `httpListen` reports -6 and the
// server is left EXACTLY as it was found: not listening, no loop scheduled, and
// no listening socket held. A server marked live whose loop never ran would
// accept nothing forever while every retry answered -7 ("already listening").
//
// Runs late: exhausting the fiber id space is permanent for the process
// [HTTP-SERVER], [CONCURRENCY-SPAWN-AWAIT].
// Descriptors this process holds open. A listener that reports failure while
// keeping its socket leaks one per attempt, and EVERY return value in this unit
// looks identical when it does -- the leak is only visible from outside the
// call, in the process's own table.
enum { FD_SCAN_LIMIT = 4096 };

static int open_descriptor_count(void) {
  int limit = getdtablesize();
  int scanned = limit < FD_SCAN_LIMIT ? limit : FD_SCAN_LIMIT;
  int open_count = 0;
  for (int fd = 0; fd < scanned; fd++) {
    if (fcntl(fd, F_GETFD) != -1) {
      open_count++;
    }
  }
  return open_count;
}

static int64_t idle_fiber(void *ignored) {
  (void)ignored;
  return 0;
}

// The size of a fiber's record, found by failing each size in turn until a spawn
// is refused: the runtime keeps the struct to itself.
static size_t fiber_record_bytes(void) {
  for (size_t bytes = 8; bytes <= 1024; bytes += 8) {
    test_alloc_fail_size(bytes);
    int64_t fiber = fiber_spawn_env(idle_fiber, NULL);
    test_alloc_fail_off();
    if (fiber == -2) {
      return bytes;
    }
    assert(fiber > 0 && fiber_await(fiber) == 0);
  }
  assert(false && "a spawn must allocate a record");
  return 0;
}

void test_listener_spawn_failure_is_reported(void) {
  int64_t server_id =
      http_create_server(SERVER_TEST_PORT, (char *)(uintptr_t) "127.0.0.1");
  assert(server_id > 0);

  // The one way left for a spawn to fail: no memory for the fiber's record.
  size_t record = fiber_record_bytes();
  int before = open_descriptor_count();
  test_alloc_fail_size(record);
  assert(http_listen(server_id, http_test_server_handler) == -6);
  test_alloc_fail_off();

  // The socket was opened and BOUND before the spawn failed, so at the moment
  // -6 is returned it is a live listener on a real port. Unwinding it is the
  // whole obligation of this arm, and none of it reaches the caller: the record
  // must survive, disowned of its socket and of the loop it never scheduled.
  HttpServer *server = servers[server_id];
  assert(server != NULL && "a failed listen must not unregister the server");
  assert(server->socket_fd == -1 && "the bound socket must be disowned");
  assert(!server->is_listening && "a failed listen must not claim to listen");
  assert(!server->loop_scheduled && "and must schedule no accept loop");
  assert(server->server_fiber_id == -1 &&
         "a spawn that produced no fiber must record no fiber id");
  assert(open_descriptor_count() == before &&
         "the bound socket must be CLOSED, not merely forgotten");

  // Not -7: a -7 here would mean the failed attempt left the server marked as
  // listening, which is the state that can never be recovered from. And the
  // second attempt binds and unwinds its own socket, so a release that only
  // worked once is caught by the same count.
  test_alloc_fail_size(record);
  assert(http_listen(server_id, http_test_server_handler) == -6);
  test_alloc_fail_off();
  assert(open_descriptor_count() == before &&
         "every retry must release its socket too");
  assert(http_stop_server(server_id) == 0);
  assert(open_descriptor_count() == before &&
         "stopping a server that never listened releases nothing further");
}

static void *client_gone_elsewhere(void *ignored) {
  (void)ignored;
  return (void *)(intptr_t)http_client_gone();
}

// [HTTP-SERVER-CLIENT-GONE] over a real connected pair: only the served
// client's orderly close reads as gone, never a byte waiting, another thread or
// a socket no handler is serving.
void test_client_gone_follows_the_served_client(void) {
  int pair[2];
  assert(socketpair(AF_UNIX, SOCK_STREAM, 0, pair) == 0);
  assert(http_client_gone() == 0 && "outside a handler nobody has gone");
  http_serving_client(pair[0]);
  assert(http_client_gone() == 0 && "a quiet open client is still there");
  assert(send(pair[1], "x", 1, 0) == 1);
  assert(http_client_gone() == 0 && "a pipelined byte is a client still there");
  assert(http_client_gone() == 0 && "and the probe leaves it unread");
  char byte = 0;
  assert(recv(pair[0], &byte, 1, 0) == 1 && byte == 'x');
  assert(close(pair[1]) == 0);
  assert(http_client_gone() == 1 && "an orderly close is a client gone");
  assert(http_client_gone() == 1 && "and stays gone");
  pthread_t other;
  void *seen = (void *)1;
  assert(pthread_create(&other, NULL, client_gone_elsewhere, NULL) == 0);
  assert(pthread_join(other, &seen) == 0);
  assert(seen == NULL && "another thread serves no client");
  http_serving_client(-1);
  assert(http_client_gone() == 0 && "a finished handler serves nobody");
#ifndef _WIN32
  struct rlimit saved;
  assert(getrlimit(RLIMIT_NOFILE, &saved) == 0);
  struct rlimit wide = saved;
  wide.rlim_cur = FD_SETSIZE + 1;
  assert(setrlimit(RLIMIT_NOFILE, &wide) == 0);
  assert(dup2(pair[0], FD_SETSIZE) == FD_SETSIZE);
  http_serving_client(FD_SETSIZE);
  assert(http_client_gone() == 0 &&
         "a socket select cannot watch is not judged");
  http_serving_client(-1);
  assert(close(FD_SETSIZE) == 0);
  assert(setrlimit(RLIMIT_NOFILE, &saved) == 0);
#endif
  assert(close(pair[0]) == 0);
}
