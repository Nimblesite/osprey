#include "http_server_internal.h"

#include <limits.h>
#include <strings.h> // strncasecmp
#include <time.h>

typedef struct {
  int64_t status;
  const char *reason;
} StatusReason;

typedef struct {
  char method[16];
  char path[256];
  char client_id[HTTP_CLIENT_REQUEST_ID_BYTES];
} RequestLogLabels;

// The statuses Node.js names, spelled as Node.js spells them, so a reply's
// status line is the one a Node.js server sends. [HTTP-SERVER]
static const StatusReason status_reasons[] = {
    {200, "OK"},
    {201, "Created"},
    {202, "Accepted"},
    {204, "No Content"},
    {206, "Partial Content"},
    {301, "Moved Permanently"},
    {302, "Found"},
    {303, "See Other"},
    {304, "Not Modified"},
    {307, "Temporary Redirect"},
    {308, "Permanent Redirect"},
    {400, "Bad Request"},
    {401, "Unauthorized"},
    {403, "Forbidden"},
    {404, "Not Found"},
    {405, "Method Not Allowed"},
    {406, "Not Acceptable"},
    {408, "Request Timeout"},
    {409, "Conflict"},
    {410, "Gone"},
    {411, "Length Required"},
    {412, "Precondition Failed"},
    {413, "Payload Too Large"},
    {415, "Unsupported Media Type"},
    {422, "Unprocessable Entity"},
    {429, "Too Many Requests"},
    {500, "Internal Server Error"},
    {501, "Not Implemented"},
    {502, "Bad Gateway"},
    {503, "Service Unavailable"},
    {504, "Gateway Timeout"},
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
  return osp_socket_configure_no_sigpipe(socket_fd);
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

// The Connection header Node.js sends for each outcome [HTTP-SERVER-CONNECTIONS].
static const char *connection_header(bool keep_alive) {
  return keep_alive ? "Connection: keep-alive\r\nKeep-Alive: timeout=5\r\n"
                    : "Connection: close\r\n";
}

// A reply head being written: false once something did not fit.
typedef struct {
  char bytes[MAX_HTTP_BUFFER];
  size_t length;
  bool fits;
} ReplyHead;

static void head_add(ReplyHead *head, const char *text, size_t length) {
  head->fits = head->fits && length < sizeof(head->bytes) - head->length;
  if (head->fits) {
    memcpy(head->bytes + head->length, text, length);
    head->length += length;
  }
}

static void head_put(ReplyHead *head, const char *text) { head_add(head, text, strlen(text)); }

// What the runtime owns in a reply head [HTTP-SERVER-HEADERS].
typedef struct {
  char length[48];   // the body's Content-Length line
  char date[48];     // a Date line in IMF-fixdate
  const char *close; // the Connection line(s) this connection's outcome needs
  bool length_put, date_put, close_put;
} RuntimeHeaders;

// RFC 9110 5.6.7's IMF-fixdate for now, named in English whatever the locale.
static void imf_fixdate(char *out, size_t room) {
  static const char *const DAYS[] = {"Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"};
  static const char *const MONTHS[] = {"Jan", "Feb", "Mar", "Apr", "May", "Jun",
                                       "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"};
  time_t now = time(NULL);
  struct tm utc;
#ifdef _WIN32
  bool known = gmtime_s(&utc, &now) == 0;
#else
  bool known = gmtime_r(&now, &utc) != NULL;
#endif
  if (!known) {
    utc = (struct tm){.tm_year = 70, .tm_mday = 1}; // the epoch: a clock that cannot be read
  }
  (void)snprintf(out, room, "Date: %s, %02d %s %04d %02d:%02d:%02d GMT\r\n", DAYS[utc.tm_wday % 7],
                 utc.tm_mday, MONTHS[utc.tm_mon % 12], utc.tm_year + 1900, utc.tm_hour, utc.tm_min, utc.tm_sec);
}

static bool header_named(const char *line, size_t length, const char *name) {
  size_t n = strlen(name);
  return length > n && line[n] == ':' && strncasecmp(line, name, n) == 0;
}

// One line of the handler's headers, `length` bytes without its line ending. The
// runtime's own fields take the place the handler gave them: its Content-Length
// replaces the handler's, right or wrong, so the body is framed exactly once; its
// Connection replaces the handler's; the handler's Date is kept.
static void head_handler_line(ReplyHead *head, RuntimeHeaders *own, const char *line, size_t length) {
  if (header_named(line, length, "Content-Length")) {
    head_put(head, own->length_put ? "" : own->length);
    own->length_put = true;
  } else if (header_named(line, length, "Connection") || header_named(line, length, "Keep-Alive")) {
    head_put(head, own->close_put ? "" : own->close);
    own->close_put = true;
  } else {
    own->date_put = own->date_put || header_named(line, length, "Date");
    head_add(head, line, length);
    head_put(head, "\r\n");
  }
}

// The handler's header block, each line ended whether or not the handler ended
// it (#398): an unended line ran into the Content-Length and lost the framing.
static void head_handler_lines(ReplyHead *head, RuntimeHeaders *own, const char *headers) {
  for (const char *line = headers; *line != '\0';) {
    const char *end = strchr(line, '\n');
    size_t length = end ? (size_t)(end - line) : strlen(line);
    size_t text = length > 0 && line[length - 1] == '\r' ? length - 1 : length;
    if (text > 0) {
      head_handler_line(head, own, line, text);
    }
    line += end ? length + 1 : length;
  }
}

// The reply head: the status line, the handler's headers in its order, then
// whichever of Content-Length, Date and Connection it did not place, Node's order.
static void compose_head(ReplyHead *head, int64_t status, const char *headers, size_t body_bytes,
                         bool keep_alive) {
  RuntimeHeaders own = {.close = connection_header(keep_alive)};
  (void)snprintf(own.length, sizeof(own.length), "Content-Length: %zu\r\n", body_bytes);
  imf_fixdate(own.date, sizeof(own.date));
  char status_line[96];
  (void)snprintf(status_line, sizeof(status_line), "HTTP/1.1 %" PRId64 " %s\r\n", status, status_reason(status));
  head_put(head, status_line);
  head_handler_lines(head, &own, headers);
  head_put(head, own.length_put ? "" : own.length);
  head_put(head, own.date_put ? "" : own.date);
  head_put(head, own.close_put ? "" : own.close);
  head_put(head, "\r\n");
}

bool http_send_response(int client_fd, int64_t status, const char *headers,
                        const char *body, size_t body_bytes, bool keep_alive) {
  ReplyHead head = {.length = 0, .fits = true};
  compose_head(&head, status, headers, body_bytes, keep_alive);
  return head.fits && http_send_all(client_fd, head.bytes, head.length) == 0 &&
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
  /* The query and fragment are attacker-supplied and unbounded; the path alone
     is what identifies the route. Truncating here means the placeholder for an
     unparseable path must avoid these two characters -- hence HTTP_LOG_UNKNOWN
     rather than the "?" this used to silently erase. */
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
