#include "http_server_pool.h"
#include "memory_parked_wait.h"
#include "memory_hooks.h"
#include "string_runtime.h"

// Native listener and callback bridge [HTTP-SERVER].


// Managed closures must be direct data roots: malloc'd server records are
// outside the tracing heap. ARC owns one reference until listener teardown.
static void *handler_closures[MAX_SERVERS];
typedef HttpResponse *(*ClosureHandler)(void *, char *, char *, char *, char *);

static const char *simple_response_body = "Hello, World!";


static void label_partial_request(HttpRequestBuffer *request) {
  if (request->data && request->received_bytes > 0) {
    (void)sscanf(request->data, "%15s %255s", request->method, request->path);
  }
  if (request->method[0] == '\0') {
    strcpy(request->method, HTTP_LOG_UNKNOWN);
  }
  if (request->path[0] == '\0') {
    strcpy(request->path, HTTP_LOG_UNKNOWN);
  }
}

static void reject_request(int client_fd, HttpRequestBuffer *request,
                           RequestReadStatus read_status) {
  label_partial_request(request);
  const char *body = http_rejection_body(read_status);
  int64_t status = http_rejection_status(read_status);
  size_t bytes = strlen(body);
  bool sent = http_send_response(client_fd, status,
                                 "Content-Type: application/json\r\n", body,
                                 bytes, false);
  http_log_exchange(request, read_status, status, bytes, sent);
}

static struct HttpResponse *call_handler(HttpServer *server,
                                         HttpRequestBuffer *request) {
  void *closure = handler_closures[server->id];
  if (closure) {
    ClosureHandler callback;
    memcpy(&callback, closure, sizeof(callback));
    return callback(closure, request->method, request->path, request->headers,
                    request->body);
  }
  return server->handler ? server->handler(request->method, request->path,
                                           request->headers, request->body)
                         : NULL;
}

static bool serve_request(int client_fd, HttpServer *server,
                          HttpRequestBuffer *request) {
  // The borrowed body normally already has a terminator at its exact length.
  // Own an explicit byte span when a C terminator would hide part of it.
  char *managed_body = NULL;
  size_t request_bytes = request->expected_body_bytes;
  if (memchr(request->body, '\0', request_bytes)) {
    managed_body = osp_string_dup_internal(request->body, request_bytes);
    if (!managed_body) {
      reject_request(client_fd, request, REQUEST_READ_FAILED);
      return false;
    }
    request->body = managed_body;
  }
  http_pool_enter_handler(server);
  http_serving_client(client_fd);
  struct HttpResponse *response = call_handler(server, request);
  http_serving_client(-1);
  http_pool_leave_handler(server);
  bool has_response = response && response->partialBody;
  int64_t status = has_response ? response->status : 200;
  const char *headers = response && response->headers
                            ? response->headers
                            : "Content-Type: text/plain\r\n";
  const char *body =
      has_response ? response->partialBody : simple_response_body;
  size_t bytes = (size_t)osp_strlen(body);
  bool sent = http_send_response(client_fd, status, headers, body, bytes,
                                 request->keep_alive);
  http_log_exchange(request, REQUEST_COMPLETE, status, bytes, sent);
  /* The Osprey handler transferred +1 on its HttpResponse across the C
   * callback boundary; the response (and the header/body strings its layout
   * word marks) dies here, once the bytes are on the wire and logged.
   * [GC-ARC-PERCEUS]. No-op off ARC. */
  osp_release(response);
  osp_release(managed_body);
  return sent && request->keep_alive;
}

// A worker waiting for request bytes cannot touch the heap, so it parks and
// never holds up a collection [MEM-GC-SAFEPOINT].
static RequestReadStatus read_parked(int client_fd, HttpRequestBuffer *request) {
  jmp_buf registers;
  OSP_SPILL_REGISTERS(registers);
  osp_mem_baton_park(&registers);
  RequestReadStatus status = read_http_request(client_fd, request);
  osp_mem_baton_unpark();
  return status;
}

bool http_serve_connection(HttpServer *server, int client_fd) {
  HttpRequestBuffer request;
  RequestReadStatus status = read_parked(client_fd, &request);
  bool keep = false;
  if (status == REQUEST_COMPLETE) {
    keep = serve_request(client_fd, server, &request);
  } else {
    reject_request(client_fd, &request, status);
  }
  free(request.data);
  return keep;
}

bool http_server_is_listening(HttpServer *server) {
  pthread_mutex_lock(&server->mutex);
  bool listening = server->is_listening;
  pthread_mutex_unlock(&server->mutex);
  return listening;
}

int http_server_socket(HttpServer *server) {
  pthread_mutex_lock(&server->mutex);
  int socket_fd = server->socket_fd;
  pthread_mutex_unlock(&server->mutex);
  return socket_fd;
}

static void destroy_server(HttpServer *server) {
  void *closure = handler_closures[server->id];
  handler_closures[server->id] = NULL;
  osp_release(closure);
  fiber_release(server->server_fiber);
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

static int64_t server_loop_fiber(void *environment) {
  HttpServer *server = environment;
  record_server_thread(server);
  http_pool_run(server);
  finish_server_loop(server);
  return 0;
}

static HttpServer *unregister_server(int64_t server_id) {
  if (!http_valid_server_id(server_id)) {
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
  server->server_fiber = NULL;
  return server;
}

int64_t http_create_server(int64_t port, char *address) {
  if (port < 1 || port > 65535 || !address) {
    return !address ? -2 : -1;
  }
  int64_t id = get_next_id();
  if (!http_valid_server_id(id)) {
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
  Fiber *fiber = fiber_spawn_env(server_loop_fiber, server);
  if (!fiber) {
    return -6;
  }
  pthread_mutex_lock(&server->mutex);
  server->server_fiber = fiber;
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
  if (!http_valid_server_id(server_id)) {
    return -1;
  }
  pthread_mutex_lock(&runtime_mutex);
  HttpServer *server = servers[server_id];
  int status = server ? listen_registered_server(server, handler) : -1;
  pthread_mutex_unlock(&runtime_mutex);
  return status;
}

static int listen_with_closure(HttpServer *server, void *closure) {
  osp_retain(closure);
  handler_closures[server->id] = closure;
  int status = listen_registered_server(server, NULL);
  if (status < 0) {
    handler_closures[server->id] = NULL;
    osp_release(closure);
  }
  return status;
}

// Closure-aware bridge for Osprey function values. Raw C callbacks keep the
// existing bridge, including their compile-time specialised ABI.
int64_t http_listen_closure(int64_t server_id, void *closure) {
  if (!http_valid_server_id(server_id) || !closure) return -1;
  pthread_mutex_lock(&runtime_mutex);
  HttpServer *server = servers[server_id];
  if (!server || !server_can_listen(server)) {
    pthread_mutex_unlock(&runtime_mutex);
    return -1;
  }
  int status = listen_with_closure(server, closure);
  pthread_mutex_unlock(&runtime_mutex);
  return status;
}

static bool stop_is_from_server(HttpServer *server) {
  return (server->thread_known &&
          pthread_equal(server->server_thread, pthread_self()) != 0) ||
         http_pool_owns_thread(server);
}

// Shutting down the listener wakes the poller. Every connection a worker is
// serving is torn down too, except the stopping handler's own: it is still
// holding that socket open to write its own response, and tearing it down
// there discards the reply the caller is waiting for [HTTP-SERVER].
static void shutdown_server_sockets(HttpServer *server) {
  if (server->socket_fd >= 0) {
    (void)shutdown(server->socket_fd, HTTP_SHUTDOWN_BOTH);
  }
  http_pool_shutdown_clients(server);
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

static Fiber *request_server_stop(HttpServer *server, bool *self_stop) {
  pthread_mutex_lock(&server->mutex);
  server->is_listening = false;
  *self_stop = stop_is_from_server(server);
  server->destroy_on_exit = *self_stop;
  Fiber *fiber = server->server_fiber;
  shutdown_server_sockets(server);
  pthread_mutex_unlock(&server->mutex);
  return fiber;
}

int64_t http_stop_server(int64_t server_id) {
  HttpServer *server = unregister_server(server_id);
  if (!server) {
    return http_valid_server_id(server_id) ? 0 : -1;
  }
  bool self_stop = false;
  Fiber *fiber = request_server_stop(server, &self_stop);
  if (self_stop) {
    return 0;
  }
  if (fiber && fiber_join(fiber) < 0) {
    return -2;
  }
  close_server_listener(server);
  destroy_server(server);
  return 0;
}
