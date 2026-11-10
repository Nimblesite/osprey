// The mock socket layer and fixtures of http_server_request_tests.c, included
// once at its top: every socket, select, clock and free call the server sources
// make is redirected to a scripted mock before those sources are compiled into
// the suite's single translation unit, so a test sets what the "network"
// returns and reads back what the server sent, logged and freed.
#ifndef OSPREY_HTTP_SERVER_REQUEST_MOCKS_H
#define OSPREY_HTTP_SERVER_REQUEST_MOCKS_H

#include "http_shared.h"

#include <assert.h>
#include <stdarg.h>

static int mock_accept(int socket_fd, struct sockaddr *address,
                       socklen_t *address_length);
static ssize_t mock_recv(int socket_fd, void *buffer, size_t length, int flags);
static ssize_t mock_send(int socket_fd, const void *data, size_t length,
                         int flags);
static int mock_select(int descriptor_count, fd_set *read_set,
                       fd_set *write_set, fd_set *error_set,
                       struct timeval *timeout);
static int mock_setsockopt(int socket_fd, int level, int option,
                           const char *value, socklen_t length);
static int mock_shutdown(int socket_fd, int how);
static int mock_close(int socket_fd);
static int mock_fprintf(FILE *stream, const char *format, ...);
static void mock_free(void *pointer);
static uint64_t mock_monotonic_ms(void);

#define accept mock_accept
#ifdef close
#undef close
#endif
#define close mock_close
#define fprintf mock_fprintf
#define free mock_free
#define recv mock_recv
#define select mock_select
#define send mock_send
#define setsockopt mock_setsockopt
#define shutdown mock_shutdown
#define HTTP_MONOTONIC_MS mock_monotonic_ms
#include "http_server_request.c"
#include "http_server_response.c"
#include "http_server_runtime.c"
#undef HTTP_MONOTONIC_MS
#undef accept
#undef close
#undef fprintf
#undef free
#undef recv
#undef select
#undef send
#undef setsockopt
#undef shutdown

HttpServer *servers[MAX_SERVERS] = {0};
pthread_mutex_t runtime_mutex = PTHREAD_MUTEX_INITIALIZER;

static HttpServer server;
static const char *receive_chunks[8];
static size_t receive_count;
static size_t receive_index;
static int receive_flags;
static char captured_body[256];
static char captured_log[2048];
static size_t select_calls;
static int select_results[8];
static uint64_t select_timeouts[8];
static size_t select_result_count;
static size_t select_result_index;
static int select_errno;
static uint64_t monotonic_values[8];
static size_t monotonic_count;
static size_t monotonic_index;
static bool socket_sigpipe_suppressed;
static int setsockopt_result;
static void *protected_frees[2];
static size_t protected_free_count;
static int64_t awaited_fiber_id;

int64_t get_next_id(void) { return 1; }

int64_t fiber_spawn(int64_t (*fn)(void)) {
  (void)fn;
  return -1;
}

int64_t fiber_spawn_env(int64_t (*fn)(void *), void *environment) {
  (void)fn;
  (void)environment;
  return -1;
}

int64_t fiber_await(int64_t fiber_id) {
  awaited_fiber_id = fiber_id;
  return 0;
}

static int mock_accept(int socket_fd, struct sockaddr *address,
                       socklen_t *address_length) {
  (void)address;
  (void)address_length;
  assert(socket_fd == 7);
  return 42;
}

static ssize_t mock_recv(int socket_fd, void *buffer, size_t length,
                         int flags) {
  assert(socket_fd == 42);
  assert(flags == receive_flags);
  if (receive_index >= receive_count) {
    errno = EAGAIN;
    return -1;
  }
  const char *chunk = receive_chunks[receive_index++];
  if (!chunk) {
    return 0;
  }
  size_t size = strlen(chunk);
  assert(size <= length);
  memcpy(buffer, chunk, size);
  return (ssize_t)size;
}

static int mock_select(int descriptor_count, fd_set *read_set,
                       fd_set *write_set, fd_set *error_set,
                       struct timeval *timeout) {
  (void)descriptor_count;
  (void)read_set;
  (void)write_set;
  (void)error_set;
  uint64_t milliseconds = (uint64_t)timeout->tv_sec * 1000U;
  milliseconds += (uint64_t)timeout->tv_usec / 1000U;
  assert(select_calls < sizeof(select_timeouts) / sizeof(select_timeouts[0]));
  select_timeouts[select_calls] = milliseconds;
  select_calls++;
  int result = select_result_index < select_result_count
                   ? select_results[select_result_index++]
                   : 1;
  if (result < 0) {
    // A failing select only means something together with its errno: EINTR is
    // "ask again", anything else ends the read. Leaving errno to whatever an
    // unrelated call last set would make which branch runs a property of the
    // harness rather than of the case under test.
    errno = select_errno;
  }
  return result;
}

static ssize_t mock_send(int socket_fd, const void *data, size_t length,
                         int flags) {
  (void)data;
  assert(socket_fd == 42);
  assert(flags == HTTP_SEND_FLAGS);
  return (ssize_t)length;
}

static int mock_setsockopt(int socket_fd, int level, int option,
                           const char *value, socklen_t length) {
  (void)value;
  (void)length;
  assert(socket_fd == 42);
#ifdef SO_NOSIGPIPE
  socket_sigpipe_suppressed = level == SOL_SOCKET && option == SO_NOSIGPIPE;
#else
  (void)level;
  (void)option;
#endif
  return setsockopt_result;
}

static int mock_shutdown(int socket_fd, int how) {
  (void)how;
  assert(socket_fd == 7 || socket_fd == 42);
  return 0;
}

static int mock_close(int socket_fd) {
  assert(socket_fd == 7 || socket_fd == 42);
  server.is_listening = false;
  return 0;
}

static int mock_fprintf(FILE *stream, const char *format, ...) {
  (void)stream;
  va_list arguments;
  va_start(arguments, format);
  int written =
      vsnprintf(captured_log, sizeof(captured_log), format, arguments);
  va_end(arguments);
  return written;
}

static void mock_free(void *pointer) {
  for (size_t index = 0; index < protected_free_count; index++) {
    if (pointer == protected_frees[index]) {
      protected_frees[index] = NULL;
      return;
    }
  }
  free(pointer);
}

static uint64_t mock_monotonic_ms(void) {
  if (monotonic_index < monotonic_count) {
    return monotonic_values[monotonic_index++];
  }
  return monotonic_count > 0 ? monotonic_values[monotonic_count - 1U] : 1000U;
}

static void set_select_results(const int *results, size_t count) {
  assert(count <= sizeof(select_results) / sizeof(select_results[0]));
  memcpy(select_results, results, count * sizeof(results[0]));
  select_result_count = count;
  select_result_index = 0;
}

static void set_monotonic_values(const uint64_t *values, size_t count) {
  assert(count <= sizeof(monotonic_values) / sizeof(monotonic_values[0]));
  memcpy(monotonic_values, values, count * sizeof(values[0]));
  monotonic_count = count;
  monotonic_index = 0;
}

static struct HttpResponse *capture_handler(char *method, char *path,
                                            char *headers, char *body) {
  (void)headers;
  assert(strcmp(method, "POST") == 0);
  assert(strcmp(path, "/api/withdraw") == 0);
  snprintf(captured_body, sizeof(captured_body), "%s", body);
  static struct HttpResponse response = {
      .status = 200,
      .headers = "Content-Type: application/json\r\n",
      .contentType = "application/json",
      .streamFd = -1,
      .isComplete = true,
      .partialBody = "{}",
  };
  return &response;
}

static void reset_receive(const char **chunks, size_t count) {
  assert(count <= sizeof(receive_chunks) / sizeof(receive_chunks[0]));
  memset(receive_chunks, 0, sizeof(receive_chunks));
  memcpy(receive_chunks, chunks, count * sizeof(chunks[0]));
  receive_count = count;
  receive_index = 0;
  select_calls = 0;
  select_result_count = 0;
  select_result_index = 0;
  monotonic_count = 0;
  monotonic_index = 0;
}

static RequestReadStatus read_chunks(const char **chunks, size_t count,
                                     HttpRequestBuffer *request) {
  reset_receive(chunks, count);
  return read_http_request(42, request);
}

static void assert_complete_body(const char **chunks, size_t count,
                                 const char *expected) {
  HttpRequestBuffer request;
  assert(read_chunks(chunks, count, &request) == REQUEST_COMPLETE);
  assert(strcmp(request.body, expected) == 0);
  free(request.data);
}

static void reset_server_fixture(char *address) {
  memset(&server, 0, sizeof(server));
  pthread_mutex_init(&server.mutex, NULL);
  server.address = address;
  server.id = 1;
  server.socket_fd = -1;
  server.active_client_fd = -1;
  servers[1] = &server;
}

static void protect_server_fixture(char *address) {
  protected_frees[0] = address;
  protected_frees[1] = &server;
  protected_free_count = 2;
}

static const char *split_request_headers(const char *body) {
  static char headers[256];
  snprintf(headers, sizeof(headers),
           "POST /api/withdraw HTTP/1.1\r\n"
           "Host: 127.0.0.1\r\nContent-Type: application/json\r\n"
           "Content-Length: %zu\r\n\r\n",
           strlen(body));
  return headers;
}

#endif
