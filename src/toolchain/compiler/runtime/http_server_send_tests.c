#include "http_shared.h"
#include "http_server_internal.h"

#include <assert.h>

// Include the production translation unit with only its socket primitives
// replaced: a loopback socket cannot deterministically produce short writes or
// EINTR, so the send answers are scripted. HTTP_SHUTDOWN_BOTH lives inside
// http_server_runtime.c, so its mock comes after these includes.
#define MOCK_SERVER_SOCKET 7
#define MOCK_CLIENT_SOCKET 42

// One scripted send answer: what it returns and the errno that goes with it.
typedef struct {
  ssize_t result;
  int error_number;
} SendAction;

static const SendAction *script_cursor;
static size_t script_remaining;
static char captured[128];
static size_t captured_length;
static size_t socket_calls, spawn_calls;

static int mock_shutdown(int socket_fd, int how);

static void reset_script(const SendAction *actions, size_t count) {
  script_cursor = actions;
  script_remaining = count;
  captured_length = 0;
  memset(captured, 0, sizeof(captured));
}

static int accept_server_call(int socket_fd, bool option_matches) {
  assert(socket_fd == MOCK_SERVER_SOCKET && option_matches);
  return 0;
}

static ssize_t mock_send(int socket_fd, const void *data, size_t length,
                         int flags) {
  assert(socket_fd == MOCK_CLIENT_SOCKET);
  assert(flags == HTTP_SEND_FLAGS);
  assert(script_remaining > 0);
  SendAction action = *script_cursor;
  script_cursor++;
  script_remaining--;
  if (action.result <= 0) {
    errno = action.error_number;
    return action.result;
  }
  size_t written = (size_t)action.result < length ? (size_t)action.result
                                                  : length;
  assert(captured_length + written <= sizeof(captured));
  memcpy(captured + captured_length, data, written);
  captured_length += written;
  return (ssize_t)written;
}

static int mock_socket(int domain, int type, int protocol) {
  assert(domain == AF_INET && type == SOCK_STREAM && protocol == 0);
  socket_calls++;
  return MOCK_SERVER_SOCKET;
}

static int mock_bind(int socket_fd, const struct sockaddr *address,
                     socklen_t length) {
  (void)address;
  (void)length;
  return accept_server_call(socket_fd, true);
}

static int mock_listen(int socket_fd, int backlog) {
  return accept_server_call(socket_fd, backlog == SOMAXCONN);
}

static int mock_setsockopt(int socket_fd, int level, int option,
                           const char *value, socklen_t length) {
  (void)value;
  (void)length;
  return accept_server_call(socket_fd,
                            level == SOL_SOCKET && option == SO_REUSEADDR);
}

static int mock_close(int socket_fd) {
  return accept_server_call(socket_fd, true);
}

#define bind mock_bind
#ifdef close
#undef close
#endif
#define close mock_close
#define listen mock_listen
#define send mock_send
#define setsockopt mock_setsockopt
#define shutdown mock_shutdown
#define socket mock_socket
#include "http_server_request.c"
#include "http_server_response.c"
#include "http_server_runtime.c"
#undef bind
#undef close
#undef listen
#undef send
#undef setsockopt
#undef shutdown
#undef socket

static int mock_shutdown(int socket_fd, int how) {
  return accept_server_call(socket_fd, how == HTTP_SHUTDOWN_BOTH);
}

// Minimal definitions for the rest of http_server_runtime.c; these tests call
// only its static send_all helper.
HttpServer *servers[MAX_SERVERS] = {0};
pthread_mutex_t runtime_mutex = PTHREAD_MUTEX_INITIALIZER;
static int64_t spawn_result = -1;
static int64_t awaited_fiber_id = -1;
static bool expect_registry_lock;

int64_t get_next_id(void) { return 1; }

int64_t fiber_spawn(int64_t (*fn)(void)) { (void)fn; return -1; }

int64_t fiber_spawn_env(int64_t (*fn)(void *), void *environment) {
  (void)fn;
  (void)environment;
  spawn_calls++;
  if (expect_registry_lock) {
    int lock_status = pthread_mutex_trylock(&runtime_mutex);
    if (lock_status == 0) {
      pthread_mutex_unlock(&runtime_mutex);
    }
    assert(lock_status == EBUSY);
  }
  return spawn_result;
}

int64_t fiber_await(int64_t fiber_id) { awaited_fiber_id = fiber_id; return 0; }

// The two ways a write stops: the send fails, or it sends nothing at all.
static void expect_send_fails(ssize_t result, int error_number) {
  const SendAction script[] = {{result, error_number}};
  reset_script(script, sizeof(script) / sizeof(script[0]));
  assert(http_send_all(MOCK_CLIENT_SOCKET, "abc", 3) == -1);
  assert(script_remaining == 0);
}

static void test_short_writes_and_eintr_are_retried(void) {
  static const SendAction script[] = {{3, 0}, {-1, EINTR}, {2, 0}, {99, 0}};
  reset_script(script, sizeof(script) / sizeof(script[0]));

  assert(http_send_all(MOCK_CLIENT_SOCKET, "abcdefgh", 8) == 0);
  assert(script_remaining == 0);
  assert(captured_length == 8);
  assert(memcmp(captured, "abcdefgh", 8) == 0);
}

static void test_closed_or_failed_socket_stops_the_write(void) {
  expect_send_fails(-1, EPIPE);
  expect_send_fails(0, 0);
}

static void test_empty_buffer_needs_no_socket_call(void) {
  reset_script(NULL, 0);
  assert(http_send_all(MOCK_CLIENT_SOCKET, "", 0) == 0);
  assert(script_remaining == 0);
}

static void test_listen_publication_and_double_listen_are_serialized(void) {
  spawn_result = 9;
  awaited_fiber_id = -1;
  socket_calls = 0;
  spawn_calls = 0;
  expect_registry_lock = true;
  int64_t server_id = http_create_server(19001, "127.0.0.1");
  assert(server_id == 1);
  assert(http_listen(server_id, NULL) == 0);
  assert(socket_calls == 1 && spawn_calls == 1);
  assert(http_listen(server_id, NULL) == -7);
  assert(socket_calls == 1 && spawn_calls == 1);
  assert(http_stop_server(server_id) == 0);
  assert(awaited_fiber_id == 9);
}

int main(void) {
  test_short_writes_and_eintr_are_retried();
  test_closed_or_failed_socket_stops_the_write();
  test_empty_buffer_needs_no_socket_call();
  test_listen_publication_and_double_listen_are_serialized();
  return 0;
}
