#include "http_server_internal.h"

extern int64_t fiber_await(int64_t fiber_id);
extern int64_t fiber_spawn_env(int64_t (*fn)(void *), void *environment);

static const char *simple_response_body = "Hello, World!";

#ifdef _WIN32
#define HTTP_SHUTDOWN_BOTH SD_BOTH
#else
#define HTTP_SHUTDOWN_BOTH SHUT_RDWR
#endif

#define HTTP_ACCEPT_POLL_MS 100U

typedef enum { LISTENER_IDLE, LISTENER_READY, LISTENER_FAILED } ListenerStatus;

static void label_partial_request(HttpRequestBuffer *request) {
  if (request->data && request->received_bytes > 0) {
    (void)sscanf(request->data, "%15s %255s", request->method, request->path);
  }
  if (request->method[0] == '\0') {
    strcpy(request->method, "?");
  }
  if (request->path[0] == '\0') {
    strcpy(request->path, "?");
  }
}

static void reject_request(int client_fd, HttpRequestBuffer *request,
                           RequestReadStatus read_status) {
  label_partial_request(request);
  const char *body = http_rejection_body(read_status);
  int64_t status = http_rejection_status(read_status);
  bool sent = http_send_response(client_fd, status,
                                 "Content-Type: application/json\r\n", body);
  http_log_exchange(request, read_status, status, strlen(body), sent);
}

static struct HttpResponse *call_handler(HttpServer *server,
                                         HttpRequestBuffer *request) {
  return server->handler ? server->handler(request->method, request->path,
                                           request->headers, request->body)
                         : NULL;
}

static void serve_request(int client_fd, HttpServer *server,
                          HttpRequestBuffer *request) {
  struct HttpResponse *response = call_handler(server, request);
  bool has_response = response && response->partialBody;
  int64_t status = has_response ? response->status : 200;
  const char *headers = response && response->headers
                            ? response->headers
                            : "Content-Type: text/plain\r\n";
  const char *body =
      has_response ? response->partialBody : simple_response_body;
  bool sent = http_send_response(client_fd, status, headers, body);
  http_log_exchange(request, REQUEST_COMPLETE, status, strlen(body), sent);
}

static void process_client(int client_fd, HttpServer *server) {
  HttpRequestBuffer request;
  RequestReadStatus status = read_http_request(client_fd, &request);
  if (status == REQUEST_COMPLETE) {
    serve_request(client_fd, server, &request);
  } else {
    reject_request(client_fd, &request, status);
  }
  free(request.data);
}

static bool server_is_listening(HttpServer *server) {
  pthread_mutex_lock(&server->mutex);
  bool listening = server->is_listening;
  pthread_mutex_unlock(&server->mutex);
  return listening;
}

static int server_socket(HttpServer *server) {
  pthread_mutex_lock(&server->mutex);
  int socket_fd = server->socket_fd;
  pthread_mutex_unlock(&server->mutex);
  return socket_fd;
}

static bool claim_client(HttpServer *server, int client_fd) {
  pthread_mutex_lock(&server->mutex);
  bool claimed = server->is_listening;
  if (claimed) {
    server->active_client_fd = client_fd;
  }
  pthread_mutex_unlock(&server->mutex);
  return claimed;
}

static void release_client(HttpServer *server, int client_fd) {
  pthread_mutex_lock(&server->mutex);
  if (server->active_client_fd == client_fd) {
    server->active_client_fd = -1;
  }
  pthread_mutex_unlock(&server->mutex);
}

static void destroy_server(HttpServer *server) {
  free(server->address);
  pthread_mutex_destroy(&server->mutex);
  free(server);
}

static void close_server_listener(HttpServer *server);

static void finish_server_loop(HttpServer *server) {
  pthread_mutex_lock(&server->mutex);
  server->loop_scheduled = false;
  bool destroy = server->destroy_on_exit;
  pthread_mutex_unlock(&server->mutex);
  if (destroy) {
    close_server_listener(server);
    destroy_server(server);
  }
}

static void record_server_thread(HttpServer *server) {
  pthread_mutex_lock(&server->mutex);
  server->server_thread = pthread_self();
  server->thread_known = true;
  pthread_mutex_unlock(&server->mutex);
}

static void handle_accepted_client(HttpServer *server, int client_fd) {
  if (!http_configure_client_socket(client_fd)) {
    close(client_fd);
    return;
  }
  if (claim_client(server, client_fd)) {
    process_client(client_fd, server);
    release_client(server, client_fd);
  }
  close(client_fd);
}

static bool valid_listener_socket(int socket_fd) {
#ifdef _WIN32
  return socket_fd >= 0;
#else
  return socket_fd >= 0 && socket_fd < FD_SETSIZE;
#endif
}

static int select_listener(int socket_fd, fd_set *read_set,
                           struct timeval *timeout) {
#ifdef _WIN32
  (void)socket_fd;
  return select(0, read_set, NULL, NULL, timeout);
#else
  return select(socket_fd + 1, read_set, NULL, NULL, timeout);
#endif
}

static ListenerStatus listener_status(HttpServer *server) {
  int socket_fd = server_socket(server);
  if (!valid_listener_socket(socket_fd)) {
    return LISTENER_FAILED;
  }
  fd_set read_set;
  FD_ZERO(&read_set);
  FD_SET(socket_fd, &read_set);
  struct timeval timeout = {0, HTTP_ACCEPT_POLL_MS * 1000U};
  int ready = select_listener(socket_fd, &read_set, &timeout);
  if (ready > 0) {
    return LISTENER_READY;
  }
  return ready == 0 || http_socket_interrupted() ? LISTENER_IDLE
                                                 : LISTENER_FAILED;
}

static bool accept_ready_client(HttpServer *server) {
  struct sockaddr_in address;
  socklen_t length = sizeof(address);
  int client_fd =
      accept(server_socket(server), (struct sockaddr *)&address, &length);
  if (client_fd >= 0) {
    handle_accepted_client(server, client_fd);
    return true;
  }
  return http_socket_interrupted() || !server_is_listening(server);
}

static int64_t server_loop_fiber(void *environment) {
  HttpServer *server = environment;
  record_server_thread(server);
  while (server_is_listening(server)) {
    ListenerStatus status = listener_status(server);
    if (status == LISTENER_FAILED ||
        (status == LISTENER_READY && !accept_ready_client(server))) {
      break;
    }
  }
  finish_server_loop(server);
  return 0;
}

static bool valid_server_id(int64_t server_id) {
  return server_id > 0 && server_id < MAX_SERVERS;
}

static HttpServer *unregister_server(int64_t server_id) {
  if (!valid_server_id(server_id)) {
    return NULL;
  }
  pthread_mutex_lock(&runtime_mutex);
  HttpServer *server = servers[server_id];
  servers[server_id] = NULL;
  pthread_mutex_unlock(&runtime_mutex);
  return server;
}

static HttpServer *allocate_server(int64_t id, int port, const char *address) {
  HttpServer *server = calloc(1, sizeof(*server));
  if (!server) {
    return NULL;
  }
  server->address = strdup(address);
  if (!server->address || pthread_mutex_init(&server->mutex, NULL) != 0) {
    free(server->address);
    free(server);
    return NULL;
  }
  server->id = id;
  server->port = port;
  server->socket_fd = -1;
  server->active_client_fd = -1;
  server->server_fiber_id = -1;
  return server;
}

int64_t http_create_server(int64_t port, char *address) {
  if (port < 1 || port > 65535 || !address) {
    return !address ? -2 : -1;
  }
  int64_t id = get_next_id();
  if (!valid_server_id(id)) {
    return -3;
  }
  HttpServer *server = allocate_server(id, (int)port, address);
  if (!server) {
    return -3;
  }
  pthread_mutex_lock(&runtime_mutex);
  servers[id] = server;
  pthread_mutex_unlock(&runtime_mutex);
  return id;
}

static int open_listener(void) {
  int socket_fd = socket(AF_INET, SOCK_STREAM, 0);
  if (socket_fd < 0) {
    return -2;
  }
  int enabled = 1;
  if (setsockopt(socket_fd, SOL_SOCKET, SO_REUSEADDR, (const char *)&enabled,
                 sizeof(enabled)) < 0) {
    close(socket_fd);
    return -3;
  }
  return socket_fd;
}

static int bind_listener(HttpServer *server, int socket_fd) {
  struct sockaddr_in address;
  memset(&address, 0, sizeof(address));
  address.sin_family = AF_INET;
  address.sin_port = htons(server->port);
  address.sin_addr.s_addr = inet_addr(server->address);
  if (bind(socket_fd, (struct sockaddr *)&address, sizeof(address)) < 0) {
    return -4;
  }
  return listen(socket_fd, SOMAXCONN) < 0 ? -5 : 0;
}

static void close_failed_listener(int socket_fd) {
  if (socket_fd >= 0) {
    close(socket_fd);
  }
}

static void mark_server_started(HttpServer *server, int socket_fd,
                                HttpRequestHandler handler) {
  pthread_mutex_lock(&server->mutex);
  server->handler = handler;
  server->socket_fd = socket_fd;
  server->is_listening = true;
  server->loop_scheduled = true;
  pthread_mutex_unlock(&server->mutex);
}

static void mark_spawn_failed(HttpServer *server) {
  pthread_mutex_lock(&server->mutex);
  server->is_listening = false;
  server->loop_scheduled = false;
  server->socket_fd = -1;
  pthread_mutex_unlock(&server->mutex);
}

static int spawn_server_loop(HttpServer *server) {
  int64_t fiber_id = fiber_spawn_env(server_loop_fiber, server);
  if (fiber_id < 0) {
    return -6;
  }
  pthread_mutex_lock(&server->mutex);
  server->server_fiber_id = fiber_id;
  pthread_mutex_unlock(&server->mutex);
  return 0;
}

static int launch_server(HttpServer *server, int socket_fd,
                         HttpRequestHandler handler) {
  mark_server_started(server, socket_fd, handler);
  int status = spawn_server_loop(server);
  if (status < 0) {
    mark_spawn_failed(server);
    close_failed_listener(socket_fd);
  }
  return status;
}

static bool server_can_listen(HttpServer *server) {
  pthread_mutex_lock(&server->mutex);
  bool available =
      !server->is_listening && !server->loop_scheduled && server->socket_fd < 0;
  pthread_mutex_unlock(&server->mutex);
  return available;
}

static int listen_registered_server(HttpServer *server,
                                    HttpRequestHandler handler) {
  if (!server_can_listen(server)) {
    return -7;
  }
  int socket_fd = open_listener();
  int status = socket_fd < 0 ? socket_fd : bind_listener(server, socket_fd);
  if (status < 0) {
    close_failed_listener(socket_fd);
    return status;
  }
  status = launch_server(server, socket_fd, handler);
  if (status < 0) {
    return status;
  }
  fprintf(stderr, "HTTP server listening on %s:%d\n", server->address,
          server->port);
  return 0;
}

int64_t http_listen(int64_t server_id, HttpRequestHandler handler) {
  if (!valid_server_id(server_id)) {
    return -1;
  }
  pthread_mutex_lock(&runtime_mutex);
  HttpServer *server = servers[server_id];
  int status = server ? listen_registered_server(server, handler) : -1;
  pthread_mutex_unlock(&runtime_mutex);
  return status;
}

static bool stop_is_from_server(HttpServer *server) {
  return server->thread_known &&
         pthread_equal(server->server_thread, pthread_self()) != 0;
}

static void shutdown_server_sockets(HttpServer *server) {
  if (server->socket_fd >= 0) {
    (void)shutdown(server->socket_fd, HTTP_SHUTDOWN_BOTH);
  }
  if (server->active_client_fd >= 0) {
    (void)shutdown(server->active_client_fd, HTTP_SHUTDOWN_BOTH);
  }
}

static void close_server_listener(HttpServer *server) {
  pthread_mutex_lock(&server->mutex);
  int socket_fd = server->socket_fd;
  server->socket_fd = -1;
  pthread_mutex_unlock(&server->mutex);
  if (socket_fd >= 0) {
    close(socket_fd);
  }
}

static int64_t request_server_stop(HttpServer *server, bool *self_stop) {
  pthread_mutex_lock(&server->mutex);
  server->is_listening = false;
  *self_stop = stop_is_from_server(server);
  server->destroy_on_exit = *self_stop;
  int64_t fiber_id = server->server_fiber_id;
  shutdown_server_sockets(server);
  pthread_mutex_unlock(&server->mutex);
  return fiber_id;
}

int64_t http_stop_server(int64_t server_id) {
  HttpServer *server = unregister_server(server_id);
  if (!server) {
    return valid_server_id(server_id) ? 0 : -1;
  }
  bool self_stop = false;
  int64_t fiber_id = request_server_stop(server, &self_stop);
  if (self_stop) {
    return 0;
  }
  if (fiber_id > 0 && fiber_await(fiber_id) < 0) {
    return -2;
  }
  close_server_listener(server);
  destroy_server(server);
  return 0;
}
