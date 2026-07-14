#include "http_server_internal.h"

#include <ctype.h>
#include <time.h>

typedef struct {
  size_t content_length;
  bool content_length_found;
  bool transfer_encoding_found;
} FramingHeaders;

static pthread_mutex_t request_sequence_mutex = PTHREAD_MUTEX_INITIALIZER;
static uint64_t next_request_sequence = 1;

#ifndef HTTP_MONOTONIC_MS
static uint64_t monotonic_milliseconds(void) {
#ifdef _WIN32
  return (uint64_t)GetTickCount64();
#else
  struct timespec now;
  return clock_gettime(CLOCK_MONOTONIC, &now) == 0
             ? (uint64_t)now.tv_sec * 1000U + (uint64_t)now.tv_nsec / 1000000U
             : 0;
#endif
}
#define HTTP_MONOTONIC_MS monotonic_milliseconds
#endif

bool http_socket_interrupted(void) {
#ifdef _WIN32
  return WSAGetLastError() == WSAEINTR;
#else
  return errno == EINTR;
#endif
}

static uint64_t take_request_sequence(void) {
  pthread_mutex_lock(&request_sequence_mutex);
  uint64_t sequence = next_request_sequence++;
  pthread_mutex_unlock(&request_sequence_mutex);
  return sequence;
}

static bool log_byte_is_safe(unsigned char byte) {
  return byte >= (unsigned char)'!' && byte <= (unsigned char)'~';
}

void sanitize_log_token(const char *input, char *output, size_t capacity) {
  if (capacity == 0) {
    return;
  }
  size_t index = 0;
  for (; input && input[index] && index + 1U < capacity; index++) {
    unsigned char byte = (unsigned char)input[index];
    output[index] = log_byte_is_safe(byte) ? (char)byte : '_';
  }
  output[index] = '\0';
}

static const char *skip_optional_space(const char *start, const char *end) {
  while (start < end && (*start == ' ' || *start == '\t')) {
    start++;
  }
  return start;
}

static const char *trim_optional_space(const char *start, const char *end) {
  while (end > start && (end[-1] == ' ' || end[-1] == '\t')) {
    end--;
  }
  return end;
}

static RequestReadStatus parse_decimal(const char *start, const char *end,
                                       size_t *result) {
  start = skip_optional_space(start, end);
  end = trim_optional_space(start, end);
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
  *result = value;
  return start == end ? REQUEST_COMPLETE : REQUEST_MALFORMED;
}

static bool header_name_is(const char *text, size_t length,
                           const char *expected) {
  if (length != strlen(expected)) {
    return false;
  }
  for (size_t index = 0; index < length; index++) {
    if (tolower((unsigned char)text[index]) !=
        tolower((unsigned char)expected[index])) {
      return false;
    }
  }
  return true;
}

static RequestReadStatus record_content_length(const char *value,
                                               const char *end,
                                               FramingHeaders *framing) {
  size_t candidate = 0;
  RequestReadStatus status = parse_decimal(value, end, &candidate);
  if (status != REQUEST_COMPLETE) {
    return status;
  }
  if (framing->content_length_found && candidate != framing->content_length) {
    return REQUEST_MALFORMED;
  }
  framing->content_length_found = true;
  framing->content_length = candidate;
  return REQUEST_COMPLETE;
}

static void record_client_request_id(const char *value, const char *end,
                                     HttpRequestBuffer *request) {
  value = skip_optional_space(value, end);
  end = trim_optional_space(value, end);
  size_t length = (size_t)(end - value);
  if (length >= sizeof(request->client_request_id)) {
    length = sizeof(request->client_request_id) - 1U;
  }
  memcpy(request->client_request_id, value, length);
  request->client_request_id[length] = '\0';
  sanitize_log_token(request->client_request_id, request->client_request_id,
                     sizeof(request->client_request_id));
}

static RequestReadStatus
inspect_header_value(const char *line, const char *colon, const char *line_end,
                     HttpRequestBuffer *request, FramingHeaders *framing) {
  size_t name_length = (size_t)(colon - line);
  if (header_name_is(line, name_length, "Transfer-Encoding")) {
    framing->transfer_encoding_found = true;
  } else if (header_name_is(line, name_length, "Content-Length")) {
    return record_content_length(colon + 1, line_end, framing);
  } else if (header_name_is(line, name_length, "X-Osprey-Request-Id")) {
    record_client_request_id(colon + 1, line_end, request);
  }
  return REQUEST_COMPLETE;
}

static RequestReadStatus inspect_header_line(const char *line,
                                             const char *line_end,
                                             HttpRequestBuffer *request,
                                             FramingHeaders *framing) {
  const char *colon = memchr(line, ':', (size_t)(line_end - line));
  if (!colon || colon == line) {
    return REQUEST_MALFORMED;
  }
  for (const char *cursor = line; cursor < colon; cursor++) {
    if ((unsigned char)*cursor <= (unsigned char)' ') {
      return REQUEST_MALFORMED;
    }
  }
  return inspect_header_value(line, colon, line_end, request, framing);
}

static RequestReadStatus scan_header_lines(HttpRequestBuffer *request,
                                           const char *line,
                                           const char *header_end,
                                           FramingHeaders *framing) {
  while (line < header_end) {
    const char *line_end = strstr(line, "\r\n");
    if (!line_end || line_end > header_end) {
      return REQUEST_MALFORMED;
    }
    RequestReadStatus status =
        inspect_header_line(line, line_end, request, framing);
    if (status != REQUEST_COMPLETE) {
      return status;
    }
    line = line_end + 2;
  }
  return REQUEST_COMPLETE;
}

static RequestReadStatus parse_headers(HttpRequestBuffer *request) {
  const char *header_end = request->data + request->header_bytes - 4U;
  const char *request_line_end = strstr(request->data, "\r\n");
  if (!request_line_end) {
    return REQUEST_MALFORMED;
  }
  FramingHeaders framing = {0};
  RequestReadStatus status =
      scan_header_lines(request, request_line_end + 2, header_end, &framing);
  if (status != REQUEST_COMPLETE) {
    return status;
  }
  request->expected_body_bytes = framing.content_length;
  return framing.transfer_encoding_found ? REQUEST_UNSUPPORTED_FRAMING
                                         : REQUEST_COMPLETE;
}

static RequestReadStatus validate_request_size(HttpRequestBuffer *request,
                                               size_t *target_bytes) {
  if (request->header_bytes > MAX_HTTP_HEADER_BYTES) {
    return REQUEST_TOO_LARGE;
  }
  RequestReadStatus status = parse_headers(request);
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

static RequestReadStatus inspect_headers(HttpRequestBuffer *request,
                                         size_t *target_bytes) {
  const char *end = strstr(request->data, "\r\n\r\n");
  if (!end) {
    return request->received_bytes >= MAX_HTTP_HEADER_BYTES
               ? REQUEST_TOO_LARGE
               : REQUEST_INCOMPLETE;
  }
  request->header_bytes = (size_t)(end - request->data) + 4U;
  return validate_request_size(request, target_bytes);
}

static void set_select_timeout(struct timeval *timeout, uint64_t milliseconds) {
  timeout->tv_sec = (long)(milliseconds / 1000U);
  timeout->tv_usec = (long)(milliseconds % 1000U) * 1000L;
}

static int select_for_read(int socket_fd, uint64_t milliseconds) {
  fd_set read_set;
  FD_ZERO(&read_set);
  FD_SET(socket_fd, &read_set);
  struct timeval timeout;
  set_select_timeout(&timeout, milliseconds);
#ifdef _WIN32
  return select(0, &read_set, NULL, NULL, &timeout);
#else
  return select(socket_fd + 1, &read_set, NULL, NULL, &timeout);
#endif
}

static bool valid_read_descriptor(int socket_fd) {
#ifdef _WIN32
  return socket_fd >= 0;
#else
  return socket_fd >= 0 && socket_fd < FD_SETSIZE;
#endif
}

static RequestReadStatus wait_until_readable(int socket_fd,
                                             uint64_t deadline_ms) {
  if (!valid_read_descriptor(socket_fd)) {
    return REQUEST_READ_FAILED;
  }
  while (true) {
    uint64_t now = HTTP_MONOTONIC_MS();
    if (now >= deadline_ms) {
      return REQUEST_TIMED_OUT;
    }
    int ready = select_for_read(socket_fd, deadline_ms - now);
    if (ready >= 0) {
      return ready > 0 ? REQUEST_COMPLETE : REQUEST_TIMED_OUT;
    }
    if (!http_socket_interrupted()) {
      return REQUEST_READ_FAILED;
    }
  }
}

static RequestReadStatus record_received(HttpRequestBuffer *request,
                                         ssize_t received) {
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

static RequestReadStatus
receive_chunk(int socket_fd, HttpRequestBuffer *request, uint64_t deadline_ms) {
  size_t remaining = MAX_HTTP_REQUEST_BYTES - request->received_bytes;
  if (remaining == 0) {
    return REQUEST_TOO_LARGE;
  }
  while (true) {
    RequestReadStatus status = wait_until_readable(socket_fd, deadline_ms);
    if (status != REQUEST_COMPLETE) {
      return status;
    }
    ssize_t received =
        recv(socket_fd, request->data + request->received_bytes, remaining, 0);
    if (received >= 0 || !http_socket_interrupted()) {
      return record_received(request, received);
    }
  }
}

static RequestReadStatus finish_request(HttpRequestBuffer *request,
                                        size_t target_bytes) {
  if (sscanf(request->data, "%15s %255s", request->method, request->path) !=
      2) {
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

static RequestReadStatus initialize_request(HttpRequestBuffer *request) {
  memset(request, 0, sizeof(*request));
  request->server_request_id = take_request_sequence();
  strcpy(request->client_request_id, "-");
  request->data = malloc(MAX_HTTP_REQUEST_BYTES + 1U);
  if (!request->data) {
    return REQUEST_READ_FAILED;
  }
  request->data[0] = '\0';
  return REQUEST_COMPLETE;
}

static RequestReadStatus inspect_request(HttpRequestBuffer *request,
                                         size_t *target_bytes) {
  if (request->header_bytes == 0) {
    RequestReadStatus status = inspect_headers(request, target_bytes);
    if (status != REQUEST_COMPLETE && status != REQUEST_INCOMPLETE) {
      return status;
    }
  }
  if (request->header_bytes > 0 && request->received_bytes >= *target_bytes) {
    return finish_request(request, *target_bytes);
  }
  return REQUEST_INCOMPLETE;
}

RequestReadStatus read_http_request(int socket_fd, HttpRequestBuffer *request) {
  RequestReadStatus status = initialize_request(request);
  if (status != REQUEST_COMPLETE) {
    return status;
  }
  uint64_t deadline_ms = HTTP_MONOTONIC_MS() + HTTP_REQUEST_TIMEOUT_MS;
  size_t target_bytes = 0;
  while (true) {
    status = receive_chunk(socket_fd, request, deadline_ms);
    if (status != REQUEST_COMPLETE) {
      return status;
    }
    status = inspect_request(request, &target_bytes);
    if (status != REQUEST_INCOMPLETE) {
      return status;
    }
  }
}
