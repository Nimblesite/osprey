#include "http_shared.h"

#include <assert.h>

static int mock_accept(int socket_fd, struct sockaddr *address,
                       socklen_t *address_length);
static ssize_t mock_recv(int socket_fd, void *buffer, size_t length, int flags);
static ssize_t mock_send(int socket_fd, const void *data, size_t length,
                         int flags);
static int mock_close(int socket_fd);

#define accept mock_accept
#define close mock_close
#define recv mock_recv
#define send mock_send
#include "http_server_runtime.c"
#undef accept
#undef close
#undef recv
#undef send

HttpServer *servers[MAX_SERVERS] = {0};
pthread_mutex_t runtime_mutex = PTHREAD_MUTEX_INITIALIZER;

static HttpServer server;
static const char *receive_chunks[2];
static size_t receive_index;
static char captured_body[256];

int64_t get_next_id(void) { return 1; }

int64_t fiber_spawn(int64_t (*fn)(void)) {
  (void)fn;
  return -1;
}

static int mock_accept(int socket_fd, struct sockaddr *address,
                       socklen_t *address_length) {
  (void)address;
  (void)address_length;
  assert(socket_fd == 7);
  return 42;
}

static ssize_t mock_recv(int socket_fd, void *buffer, size_t length, int flags) {
  assert(socket_fd == 42);
  assert(flags == 0);
  assert(receive_index < 2);
  const char *chunk = receive_chunks[receive_index++];
  size_t size = strlen(chunk);
  assert(size <= length);
  memcpy(buffer, chunk, size);
  return (ssize_t)size;
}

static ssize_t mock_send(int socket_fd, const void *data, size_t length,
                         int flags) {
  (void)data;
  assert(socket_fd == 42);
  assert(flags == 0);
  return (ssize_t)length;
}

static int mock_close(int socket_fd) {
  assert(socket_fd == 42);
  server.is_listening = false;
  return 0;
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

int main(void) {
  const char *body = "{\"account\":1,\"cents\":100,\"note\":\"split\"}";
  static char headers[256];
  snprintf(headers, sizeof(headers),
           "POST /api/withdraw HTTP/1.1\r\n"
           "Host: 127.0.0.1\r\nContent-Type: application/json\r\n"
           "Content-Length: %zu\r\n\r\n",
           strlen(body));
  receive_chunks[0] = headers;
  receive_chunks[1] = body;
  server.socket_fd = 7;
  server.is_listening = true;
  server.handler = capture_handler;
  servers[1] = &server;

  assert(server_loop_fiber() == 0);
  assert(strcmp(captured_body, body) == 0);
  assert(receive_index == 2);
  return 0;
}
