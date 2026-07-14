#include "http_shared.h"

#include <assert.h>

// Include the production translation unit with only its socket send primitive
// replaced. This deterministically exercises short writes and EINTR, which a
// real loopback socket cannot guarantee on every kernel.
static ssize_t mock_send(int socket_fd, const void *data, size_t length,
                         int flags);
#define send mock_send
#include "http_server_runtime.c"
#undef send

// Minimal definitions for the rest of http_server_runtime.c; these tests call
// only its static send_all helper.
HttpServer *servers[MAX_SERVERS] = {0};
pthread_mutex_t runtime_mutex = PTHREAD_MUTEX_INITIALIZER;

int64_t get_next_id(void) { return 1; }

int64_t fiber_spawn(int64_t (*fn)(void)) {
  (void)fn;
  return -1;
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
  assert(flags == 0);
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

static void test_short_writes_and_eintr_are_retried(void) {
  static const ssize_t next_actions[] = {3, -1, 2, 99};
  static const int next_errnos[] = {0, EINTR, 0, 0};
  reset_mock(next_actions, next_errnos,
             sizeof(next_actions) / sizeof(next_actions[0]));

  assert(send_all(42, "abcdefgh", 8) == 0);
  assert(action_index == 4);
  assert(captured_length == 8);
  assert(memcmp(captured, "abcdefgh", 8) == 0);
}

static void test_closed_or_failed_socket_stops_the_write(void) {
  static const ssize_t failed_actions[] = {-1};
  static const int failed_errnos[] = {EPIPE};
  reset_mock(failed_actions, failed_errnos, 1);
  assert(send_all(42, "abc", 3) == -1);
  assert(action_index == 1);

  static const ssize_t closed_actions[] = {0};
  static const int closed_errnos[] = {0};
  reset_mock(closed_actions, closed_errnos, 1);
  assert(send_all(42, "abc", 3) == -1);
  assert(action_index == 1);
}

static void test_empty_buffer_needs_no_socket_call(void) {
  reset_mock(NULL, NULL, 0);
  assert(send_all(42, "", 0) == 0);
  assert(action_index == 0);
}

int main(void) {
  test_short_writes_and_eintr_are_retried();
  test_closed_or_failed_socket_stops_the_write();
  test_empty_buffer_needs_no_socket_call();
  return 0;
}
