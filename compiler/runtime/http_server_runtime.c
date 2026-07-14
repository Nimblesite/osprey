#include "http_shared.h"
#include <ctype.h>
#include <limits.h>

extern int64_t fiber_spawn(int64_t (*fn)(void));

static const char *simple_response_body = "Hello, World!";

#define MAX_HTTP_REQUEST_BYTES (1024U * 1024U)
#define MAX_HTTP_HEADER_BYTES (32U * 1024U)

typedef enum {
  REQUEST_COMPLETE,
  REQUEST_INCOMPLETE,
  REQUEST_MALFORMED,
  REQUEST_TOO_LARGE,
  REQUEST_READ_FAILED
} RequestReadStatus;

typedef struct {
  char *data;
  char method[16];
  char path[256];
  char *headers;
  char *body;
  size_t received_bytes;
  size_t header_bytes;
  size_t expected_body_bytes;
} HttpRequestBuffer;

// `send` is allowed to write fewer bytes than requested (and can be interrupted
// after writing none). HTTP Content-Length promises the whole payload, so every
// response write must drain the buffer before the connection is closed.
static bool socket_was_interrupted(void) {
#ifdef _WIN32
  return WSAGetLastError() == WSAEINTR;
#else
  return errno == EINTR;
#endif
}

static int send_some(int socket_fd, const char *data, size_t length) {
  int chunk = length > (size_t)INT_MAX ? INT_MAX : (int)length;
  int written = send(socket_fd, data, chunk, 0);
  if (written > 0) {
    return written;
  }
  return written < 0 && socket_was_interrupted() ? 0 : -1;
}

static int send_all(int socket_fd, const char *data, size_t length) {
  size_t offset = 0;
  while (offset < length) {
    int written = send_some(socket_fd, data + offset, length - offset);
    if (written < 0) {
      return -1;
    }
    offset += (size_t)written;
  }
  return 0;
}

static bool header_name_is(const char *text, size_t length,
                           const char *expected) {
  if (length != strlen(expected)) {
    return false;
  }
  for (size_t i = 0; i < length; i++) {
    if (tolower((unsigned char)text[i]) !=
        tolower((unsigned char)expected[i])) {
      return false;
    }
  }
  return true;
}

static RequestReadStatus parse_decimal(const char *start, const char *end,
                                       size_t *result) {
  while (start < end && (*start == ' ' || *start == '\t')) {
    start++;
  }
  if (start == end || !isdigit((unsigned char)*start)) {
    return REQUEST_MALFORMED;
  }
  size_t value = 0;
  while (start < end && isdigit((unsigned char)*start)) {
    size_t digit = (size_t)(*start++ - '0');
    if (value > (MAX_HTTP_REQUEST_BYTES - digit) / 10U) {
      return REQUEST_TOO_LARGE;
    }
    value = value * 10U + digit;
  }
  while (start < end && (*start == ' ' || *start == '\t')) {
    start++;
  }
  if (start != end) {
    return REQUEST_MALFORMED;
  }
  *result = value;
  return REQUEST_COMPLETE;
}

static RequestReadStatus record_content_length(const char *value,
                                               const char *end, bool *found,
                                               size_t *length) {
  size_t candidate = 0;
  RequestReadStatus status = parse_decimal(value, end, &candidate);
  if (status != REQUEST_COMPLETE) {
    return status;
  }
  if (*found && candidate != *length) {
    return REQUEST_MALFORMED;
  }
  *found = true;
  *length = candidate;
  return REQUEST_COMPLETE;
}

static RequestReadStatus parse_content_length(const HttpRequestBuffer *request,
                                              size_t *length) {
  const char *header_end = request->data + request->header_bytes - 4U;
  const char *line = strstr(request->data, "\r\n");
  bool found = false;
  *length = 0;
  if (!line) {
    return REQUEST_MALFORMED;
  }
  for (line += 2; line < header_end;) {
    const char *line_end = strstr(line, "\r\n");
    const char *colon = line_end ? memchr(line, ':', (size_t)(line_end - line)) : NULL;
    if (!line_end || line_end > header_end || !colon) {
      return REQUEST_MALFORMED;
    }
    if (header_name_is(line, (size_t)(colon - line), "Content-Length")) {
      RequestReadStatus status =
          record_content_length(colon + 1, line_end, &found, length);
      if (status != REQUEST_COMPLETE) {
        return status;
      }
    }
    line = line_end + 2;
  }
  return REQUEST_COMPLETE;
}

static RequestReadStatus receive_chunk(int socket_fd,
                                       HttpRequestBuffer *request) {
  size_t remaining = MAX_HTTP_REQUEST_BYTES - request->received_bytes;
  if (remaining == 0) {
    return REQUEST_TOO_LARGE;
  }
  ssize_t received;
  do {
    received = recv(socket_fd, request->data + request->received_bytes,
                    remaining, 0);
  } while (received < 0 && socket_was_interrupted());
  if (received < 0) {
    return REQUEST_READ_FAILED;
  }
  if (received == 0) {
    return REQUEST_INCOMPLETE;
  }
  request->received_bytes += (size_t)received;
  request->data[request->received_bytes] = '\0';
  return REQUEST_COMPLETE;
}

static RequestReadStatus inspect_headers(HttpRequestBuffer *request,
                                         size_t *target_bytes) {
  const char *end = strstr(request->data, "\r\n\r\n");
  if (!end) {
    return request->received_bytes >= MAX_HTTP_HEADER_BYTES ? REQUEST_TOO_LARGE
                                                            : REQUEST_INCOMPLETE;
  }
  request->header_bytes = (size_t)(end - request->data) + 4U;
  if (request->header_bytes > MAX_HTTP_HEADER_BYTES) {
    return REQUEST_TOO_LARGE;
  }
  RequestReadStatus status =
      parse_content_length(request, &request->expected_body_bytes);
  if (status != REQUEST_COMPLETE) {
    return status;
  }
  if (request->expected_body_bytes >
      MAX_HTTP_REQUEST_BYTES - request->header_bytes) {
    return REQUEST_TOO_LARGE;
  }
  *target_bytes = request->header_bytes + request->expected_body_bytes;
  return REQUEST_COMPLETE;
}

static RequestReadStatus finish_request(HttpRequestBuffer *request,
                                        size_t target_bytes) {
  if (sscanf(request->data, "%15s %255s", request->method, request->path) != 2) {
    return REQUEST_MALFORMED;
  }
  char *line_end = strstr(request->data, "\r\n");
  if (!line_end) {
    return REQUEST_MALFORMED;
  }
  char *headers_end = request->data + request->header_bytes - 4U;
  request->data[target_bytes] = '\0';
  *headers_end = '\0';
  request->headers = line_end + 2 < headers_end ? line_end + 2 : headers_end;
  request->body = request->data + request->header_bytes;
  return REQUEST_COMPLETE;
}

static RequestReadStatus read_http_request(int socket_fd,
                                           HttpRequestBuffer *request) {
  memset(request, 0, sizeof(*request));
  request->data = malloc(MAX_HTTP_REQUEST_BYTES + 1U);
  if (!request->data) {
    return REQUEST_READ_FAILED;
  }
  request->data[0] = '\0';
  size_t target_bytes = 0;
  while (true) {
    RequestReadStatus status = receive_chunk(socket_fd, request);
    if (status != REQUEST_COMPLETE) {
      return status;
    }
    if (request->header_bytes == 0) {
      status = inspect_headers(request, &target_bytes);
      if (status != REQUEST_COMPLETE && status != REQUEST_INCOMPLETE) {
        return status;
      }
    }
    if (request->header_bytes > 0 && request->received_bytes >= target_bytes) {
      return finish_request(request, target_bytes);
    }
  }
}

static const char *status_reason(int64_t status) {
  switch (status) {
  case 200:
    return "OK";
  case 201:
    return "Created";
  case 400:
    return "Bad Request";
  case 404:
    return "Not Found";
  case 405:
    return "Method Not Allowed";
  case 413:
    return "Content Too Large";
  case 422:
    return "Unprocessable Content";
  default:
    return "Error";
  }
}

static bool send_response(int client_fd, int64_t status, const char *headers,
                          const char *body) {
  char wire_headers[MAX_HTTP_BUFFER];
  size_t body_bytes = strlen(body);
  int header_bytes = snprintf(wire_headers, sizeof(wire_headers),
                              "HTTP/1.1 %" PRId64 " %s\r\n%sContent-Length: "
                              "%zu\r\nConnection: close\r\n\r\n",
                              status, status_reason(status), headers, body_bytes);
  if (header_bytes < 0 || (size_t)header_bytes >= sizeof(wire_headers)) {
    return false;
  }
  return send_all(client_fd, wire_headers, (size_t)header_bytes) == 0 &&
         send_all(client_fd, body, body_bytes) == 0;
}

static const char *read_status_name(RequestReadStatus status) {
  switch (status) {
  case REQUEST_INCOMPLETE:
    return "incomplete";
  case REQUEST_MALFORMED:
    return "malformed";
  case REQUEST_TOO_LARGE:
    return "too_large";
  case REQUEST_READ_FAILED:
    return "io_error";
  default:
    return "complete";
  }
}

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

static void log_exchange(const HttpRequestBuffer *request,
                         RequestReadStatus read_status, int64_t status,
                         size_t response_bytes, bool sent) {
  size_t body_received = request->received_bytes > request->header_bytes
                             ? request->received_bytes - request->header_bytes
                             : 0;
  fprintf(stderr,
          "[http] method=%s path=%s request_bytes=%zu header_bytes=%zu "
          "body_bytes=%zu expected_body_bytes=%zu status=%" PRId64
          " response_body_bytes=%zu read=%s sent=%s\n",
          request->method, request->path, request->received_bytes,
          request->header_bytes, body_received, request->expected_body_bytes,
          status, response_bytes, read_status_name(read_status),
          sent ? "true" : "false");
}

static void reject_request(int client_fd, HttpRequestBuffer *request,
                           RequestReadStatus read_status) {
  static const char *bad = "{\"error\":\"malformed HTTP request\"}";
  static const char *large = "{\"error\":\"HTTP request too large\"}";
  bool oversized = read_status == REQUEST_TOO_LARGE;
  const char *body = oversized ? large : bad;
  int64_t status = oversized ? 413 : 400;
  label_partial_request(request);
  bool sent = send_response(client_fd, status,
                            "Content-Type: application/json\r\n", body);
  log_exchange(request, read_status, status, strlen(body), sent);
}

static void serve_request(int client_fd, HttpServer *server,
                          HttpRequestBuffer *request) {
  struct HttpResponse *response = server->handler
                                      ? server->handler(request->method,
                                                        request->path,
                                                        request->headers,
                                                        request->body)
                                      : NULL;
  int64_t status = response && response->partialBody ? response->status : 200;
  const char *headers = response && response->headers ? response->headers :
                                                       "Content-Type: text/plain\r\n";
  const char *body = response && response->partialBody ? response->partialBody
                                                       : simple_response_body;
  bool sent = send_response(client_fd, status, headers, body);
  log_exchange(request, REQUEST_COMPLETE, status, strlen(body), sent);
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

static HttpServer *find_listening_server(void) {
  HttpServer *server = NULL;
  pthread_mutex_lock(&runtime_mutex);
  for (int i = 1; i < MAX_SERVERS; i++) {
    if (servers[i] && servers[i]->is_listening) {
      server = servers[i];
      break;
    }
  }
  pthread_mutex_unlock(&runtime_mutex);
  return server;
}

static int64_t server_loop_fiber(void) {
  HttpServer *server = find_listening_server();
  if (!server) {
    return -1;
  }
  while (server->is_listening) {
    struct sockaddr_in client_address;
    socklen_t address_length = sizeof(client_address);
    int client_fd = accept(server->socket_fd, (struct sockaddr *)&client_address,
                           &address_length);
    if (client_fd >= 0) {
      process_client(client_fd, server);
      close(client_fd);
    }
  }
  return 0;
}

// Create HTTP server - returns server_id or negative error
int64_t http_create_server(int64_t port, char *address) {
  if (port < 1 || port > 65535) {
    return -1;
  }

  if (!address) {
    return -2;
  }

  int64_t id = get_next_id();
  HttpServer *server = malloc(sizeof(HttpServer));
  if (!server) {
    return -3;
  }

  server->id = id;
  server->port = (int)port;
  server->address = strdup(address);
  if (!server->address) {
    free(server);
    return -3;
  }
  server->socket_fd = -1;
  server->is_listening = false;
  pthread_mutex_init(&server->mutex, NULL);

  pthread_mutex_lock(&runtime_mutex);
  servers[id] = server;
  pthread_mutex_unlock(&runtime_mutex);

  return id;
}

// Start HTTP server listening - returns 0 on success
int64_t http_listen(int64_t server_id, HttpRequestHandler handler) {
  pthread_mutex_lock(&runtime_mutex);
  HttpServer *server = servers[server_id];
  pthread_mutex_unlock(&runtime_mutex);

  if (!server) {
    return -1;
  }

  // Store the handler function pointer
  server->handler = handler;

  // Create socket
  server->socket_fd = socket(AF_INET, SOCK_STREAM, 0);
  if (server->socket_fd < 0) {
    return -2;
  }

  // Set socket options
  int opt = 1;
  // optval is `const char *` on Winsock, `const void *` on POSIX; the cast is
  // portable to both. [WINDOWS-PORT-PHASE2]
  if (setsockopt(server->socket_fd, SOL_SOCKET, SO_REUSEADDR, (const char *)&opt,
                 sizeof(opt)) < 0) {
    close(server->socket_fd);
    return -3;
  }

  // Bind socket
  struct sockaddr_in server_addr;
  server_addr.sin_family = AF_INET;
  server_addr.sin_port = htons(server->port);
  server_addr.sin_addr.s_addr = inet_addr(server->address);

  if (bind(server->socket_fd, (struct sockaddr *)&server_addr,
           sizeof(server_addr)) < 0) {
    close(server->socket_fd);
    return -4;
  }

  // Start listening
  if (listen(server->socket_fd, SOMAXCONN) < 0) {
    close(server->socket_fd);
    return -5;
  }

  server->is_listening = true;

  // Spawn a fiber to handle the server loop (non-blocking)
  int64_t fiber_id = fiber_spawn(server_loop_fiber);
  if (fiber_id < 0) {
    server->is_listening = false;
    close(server->socket_fd);
    return -6;
  }

  fprintf(stderr, "HTTP server listening on %s:%d\n", server->address, server->port);

  return 0;
}

// Stop HTTP server - returns 0 on success
int64_t http_stop_server(int64_t server_id) {
  pthread_mutex_lock(&runtime_mutex);
  HttpServer *server = servers[server_id];
  if (server) {
    servers[server_id] = NULL;
    server->is_listening = false;
    if (server->socket_fd >= 0) {
      close(server->socket_fd);
    }
    free(server->address);
    pthread_mutex_destroy(&server->mutex);
    free(server);
  }
  pthread_mutex_unlock(&runtime_mutex);

  return 0;
}
