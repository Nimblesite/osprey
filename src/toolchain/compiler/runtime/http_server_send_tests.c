#include "http_shared.h"

#include <assert.h>

// Include the production translation unit with only its socket send primitive
// replaced. This deterministically exercises short writes and EINTR, which a
// real loopback socket cannot guarantee on every kernel.
static ssize_t mock_send(int socket_fd, const void *data, size_t length,
                         int flags);
static int mock_socket(int domain, int type, int protocol);
static int mock_bind(int socket_fd, const struct sockaddr *address,
                     socklen_t length);
static int mock_listen(int socket_fd, int backlog);
static int mock_setsockopt(int socket_fd, int level, int option,
                           const char *value, socklen_t length);
static int mock_shutdown(int socket_fd, int how);
static int mock_close(int socket_fd);
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

// Minimal definitions for the rest of http_server_runtime.c; these tests call
// only its static send_all helper.
HttpServer *servers[MAX_SERVERS] = {0};
pthread_mutex_t runtime_mutex = PTHREAD_MUTEX_INITIALIZER;
static int64_t spawn_result = -1;
static int64_t awaited_fiber_id = -1;
static size_t socket_calls;
static size_t spawn_calls;
static bool expect_registry_lock;

int64_t get_next_id(void) { return 1; }

int64_t fiber_spawn(int64_t (*fn)(void)) {
  (void)fn;
  return -1;
}

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

int64_t fiber_await(int64_t fiber_id) {
  awaited_fiber_id = fiber_id;
  return 0;
}

static ssize_t actions[8];
static int action_errnos[8];
static size_t action_count;
static size_t action_index;
static char captured[128];
static size_t captured_length;

static void reset_mock(const ssize_t *next_actions, const int *next_errnos,
                       size_t count) {
  assert(count <= sizeof(actions) / sizeof(actions[0]));
  if (count > 0) {
    memcpy(actions, next_actions, count * sizeof(actions[0]));
    memcpy(action_errnos, next_errnos, count * sizeof(action_errnos[0]));
  }
  action_count = count;
  action_index = 0;
  captured_length = 0;
  memset(captured, 0, sizeof(captured));
}

static ssize_t mock_send(int socket_fd, const void *data, size_t length,
                         int flags) {
  assert(socket_fd == 42);
  assert(flags == HTTP_SEND_FLAGS);
  assert(action_index < action_count);
  ssize_t action = actions[action_index];
  int next_errno = action_errnos[action_index];
  action_index++;
  if (action <= 0) {
    errno = next_errno;
    return action;
  }
  size_t written = (size_t)action < length ? (size_t)action : length;
  assert(captured_length + written <= sizeof(captured));
  memcpy(captured + captured_length, data, written);
  captured_length += written;
  return (ssize_t)written;
}

static int mock_socket(int domain, int type, int protocol) {
  assert(domain == AF_INET && type == SOCK_STREAM && protocol == 0);
  socket_calls++;
  return 7;
}

static int mock_bind(int socket_fd, const struct sockaddr *address,
                     socklen_t length) {
  (void)address;
  (void)length;
  assert(socket_fd == 7);
  return 0;
}

static int mock_listen(int socket_fd, int backlog) {
  assert(socket_fd == 7 && backlog == SOMAXCONN);
  return 0;
}

static int mock_setsockopt(int socket_fd, int level, int option,
                           const char *value, socklen_t length) {
  (void)value;
  (void)length;
  assert(socket_fd == 7 && level == SOL_SOCKET && option == SO_REUSEADDR);
  return 0;
}

static int mock_shutdown(int socket_fd, int how) {
  assert(socket_fd == 7 && how == HTTP_SHUTDOWN_BOTH);
  return 0;
}

static int mock_close(int socket_fd) {
  assert(socket_fd == 7);
  return 0;
}

static void reset_lifecycle_mock(void) {
  spawn_result = 9;
  awaited_fiber_id = -1;
  socket_calls = 0;
  spawn_calls = 0;
  expect_registry_lock = true;
}

static void test_short_writes_and_eintr_are_retried(void) {
  static const ssize_t next_actions[] = {3, -1, 2, 99};
  static const int next_errnos[] = {0, EINTR, 0, 0};
  reset_mock(next_actions, next_errnos,
             sizeof(next_actions) / sizeof(next_actions[0]));

  assert(http_send_all(42, "abcdefgh", 8) == 0);
  assert(action_index == 4);
  assert(captured_length == 8);
  assert(memcmp(captured, "abcdefgh", 8) == 0);
}

static void test_closed_or_failed_socket_stops_the_write(void) {
  static const ssize_t failed_actions[] = {-1};
  static const int failed_errnos[] = {EPIPE};
  reset_mock(failed_actions, failed_errnos, 1);
  assert(http_send_all(42, "abc", 3) == -1);
  assert(action_index == 1);

  static const ssize_t closed_actions[] = {0};
  static const int closed_errnos[] = {0};
  reset_mock(closed_actions, closed_errnos, 1);
  assert(http_send_all(42, "abc", 3) == -1);
  assert(action_index == 1);
}

static void test_empty_buffer_needs_no_socket_call(void) {
  reset_mock(NULL, NULL, 0);
  assert(http_send_all(42, "", 0) == 0);
  assert(action_index == 0);
}

static void test_listen_publication_and_double_listen_are_serialized(void) {
  reset_lifecycle_mock();
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
