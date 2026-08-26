#ifndef HTTP_SERVER_INTERNAL_H
#define HTTP_SERVER_INTERNAL_H

#include "http_shared.h"

#define MAX_HTTP_REQUEST_BYTES (1024U * 1024U)
#define MAX_HTTP_HEADER_BYTES (32U * 1024U)
#define HTTP_REQUEST_TIMEOUT_MS 5000U
#define HTTP_CLIENT_REQUEST_ID_BYTES 64U

/* What a request-log field says when its value could not be read: a method and
   path that never parsed, or a client id the request never sent. An EMPTY field
   is indistinguishable from a missing one in a log line, which is the whole
   reason a placeholder exists.

   It must not contain '?' or '#'. `make_log_labels` truncates the path at the
   first of those to drop the query string, so a "?" placeholder was eaten right
   back to the empty field it was standing in for -- the log read
   `method=? path=` for every unparseable request. */
#define HTTP_LOG_UNKNOWN "-"

#ifdef MSG_NOSIGNAL
#define HTTP_SEND_FLAGS MSG_NOSIGNAL
#else
#define HTTP_SEND_FLAGS 0
#endif

typedef enum {
  REQUEST_COMPLETE,
  REQUEST_INCOMPLETE,
  REQUEST_MALFORMED,
  REQUEST_TOO_LARGE,
  REQUEST_READ_FAILED,
  REQUEST_TIMED_OUT,
  REQUEST_UNSUPPORTED_FRAMING
} RequestReadStatus;

typedef struct {
  char *data;
  char method[16];
  char path[256];
  char client_request_id[HTTP_CLIENT_REQUEST_ID_BYTES];
  char *headers;
  char *body;
  uint64_t server_request_id;
  size_t received_bytes;
  size_t header_bytes;
  size_t expected_body_bytes;
} HttpRequestBuffer;

bool http_socket_interrupted(void);
RequestReadStatus read_http_request(int socket_fd, HttpRequestBuffer *request);
void sanitize_log_token(const char *input, char *output, size_t capacity);
int http_send_all(int socket_fd, const char *data, size_t length);
bool http_configure_client_socket(int socket_fd);
bool http_send_response(int client_fd, int64_t status, const char *headers,
                        const char *body);
void http_log_exchange(const HttpRequestBuffer *request,
                       RequestReadStatus read_status, int64_t status,
                       size_t response_bytes, bool sent);
int64_t http_rejection_status(RequestReadStatus status);
const char *http_rejection_body(RequestReadStatus status);

#endif
