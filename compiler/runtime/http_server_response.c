#include "http_server_internal.h"

#include <limits.h>

typedef struct {
  int64_t status;
  const char *reason;
} StatusReason;

typedef struct {
  char method[16];
  char path[256];
  char client_id[HTTP_CLIENT_REQUEST_ID_BYTES];
} RequestLogLabels;

static const StatusReason status_reasons[] = {
    {200, "OK"},
    {201, "Created"},
    {400, "Bad Request"},
    {404, "Not Found"},
    {405, "Method Not Allowed"},
    {408, "Request Timeout"},
    {413, "Content Too Large"},
    {422, "Unprocessable Content"},
};

static const char *const read_status_names[] = {
    "complete", "incomplete", "malformed",           "too_large",
    "io_error", "timeout",    "unsupported_framing",
};

static int send_some(int socket_fd, const char *data, size_t length) {
  int chunk = length > (size_t)INT_MAX ? INT_MAX : (int)length;
  int written = send(socket_fd, data, chunk, HTTP_SEND_FLAGS);
  if (written > 0) {
    return written;
  }
  return written < 0 && http_socket_interrupted() ? 0 : -1;
}

int http_send_all(int socket_fd, const char *data, size_t length) {
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

bool http_configure_client_socket(int socket_fd) {
#ifdef SO_NOSIGPIPE
  int enabled = 1;
  return setsockopt(socket_fd, SOL_SOCKET, SO_NOSIGPIPE, (const char *)&enabled,
                    sizeof(enabled)) == 0;
#elif defined(MSG_NOSIGNAL) || defined(_WIN32)
  (void)socket_fd;
  return true;
#else
  (void)socket_fd;
  return false;
#endif
}

static const char *status_reason(int64_t status) {
  size_t count = sizeof(status_reasons) / sizeof(status_reasons[0]);
  for (size_t index = 0; index < count; index++) {
    if (status_reasons[index].status == status) {
      return status_reasons[index].reason;
    }
  }
  return "Error";
}

bool http_send_response(int client_fd, int64_t status, const char *headers,
                        const char *body) {
  char wire_headers[MAX_HTTP_BUFFER];
  size_t body_bytes = strlen(body);
  int header_bytes =
      snprintf(wire_headers, sizeof(wire_headers),
               "HTTP/1.1 %" PRId64 " %s\r\n%sContent-Length: "
               "%zu\r\nConnection: close\r\n\r\n",
               status, status_reason(status), headers, body_bytes);
  if (header_bytes < 0 || (size_t)header_bytes >= sizeof(wire_headers)) {
    return false;
  }
  return http_send_all(client_fd, wire_headers, (size_t)header_bytes) == 0 &&
         http_send_all(client_fd, body, body_bytes) == 0;
}

static const char *read_status_name(RequestReadStatus status) {
  size_t count = sizeof(read_status_names) / sizeof(read_status_names[0]);
  return status >= 0 && (size_t)status < count ? read_status_names[status]
                                               : "unknown";
}

static RequestLogLabels make_log_labels(const HttpRequestBuffer *request) {
  RequestLogLabels labels;
  sanitize_log_token(request->method, labels.method, sizeof(labels.method));
  sanitize_log_token(request->path, labels.path, sizeof(labels.path));
  char *query = strpbrk(labels.path, "?#");
  if (query) {
    *query = '\0';
  }
  sanitize_log_token(request->client_request_id, labels.client_id,
                     sizeof(labels.client_id));
  return labels;
}

static size_t received_body_bytes(const HttpRequestBuffer *request) {
  return request->received_bytes > request->header_bytes
             ? request->received_bytes - request->header_bytes
             : 0;
}

void http_log_exchange(const HttpRequestBuffer *request,
                       RequestReadStatus read_status, int64_t status,
                       size_t response_bytes, bool sent) {
  RequestLogLabels labels = make_log_labels(request);
  fprintf(stderr,
          "[http] request_id=%" PRIu64 " client_request_id=%s method=%s "
          "path=%s request_bytes=%zu header_bytes=%zu body_bytes=%zu "
          "expected_body_bytes=%zu status=%" PRId64
          " response_body_bytes=%zu read=%s sent=%s\n",
          request->server_request_id, labels.client_id, labels.method,
          labels.path, request->received_bytes, request->header_bytes,
          received_body_bytes(request), request->expected_body_bytes, status,
          response_bytes, read_status_name(read_status),
          sent ? "true" : "false");
}

int64_t http_rejection_status(RequestReadStatus status) {
  if (status == REQUEST_TOO_LARGE) {
    return 413;
  }
  return status == REQUEST_TIMED_OUT ? 408 : 400;
}

const char *http_rejection_body(RequestReadStatus status) {
  if (status == REQUEST_TOO_LARGE) {
    return "{\"error\":\"HTTP request too large\"}";
  }
  if (status == REQUEST_TIMED_OUT) {
    return "{\"error\":\"HTTP request timed out\"}";
  }
  if (status == REQUEST_UNSUPPORTED_FRAMING) {
    return "{\"error\":\"unsupported HTTP transfer encoding\"}";
  }
  return "{\"error\":\"malformed HTTP request\"}";
}
