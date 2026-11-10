// Request reading in the HTTP server [HTTP-SERVER]: bodies split across reads,
// duplicate or malformed lengths, oversized and incomplete bodies, and the
// absolute read deadline, against the scripted sockets of
// http_server_request_mocks.h.
#include "http_server_request_mocks.h"

static void test_split_body_reaches_handler(void) {
  const char *body = "{\"account\":1,\"cents\":100,\"note\":\"split\"}";
  const char *chunks[] = {split_request_headers(body), body};
  reset_receive(chunks, sizeof(chunks) / sizeof(chunks[0]));
  memset(captured_body, 0, sizeof(captured_body));
  reset_server_fixture(NULL);
  server.socket_fd = 7;
  server.is_listening = true;
  server.handler = capture_handler;
  assert(server_loop_fiber(&server) == 0);
  assert(strcmp(captured_body, body) == 0);
  assert(receive_index == 2);
  pthread_mutex_destroy(&server.mutex);
}

static void test_same_read_header_and_body(void) {
  const char *chunks[] = {"POST / HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}"};
  assert_complete_body(chunks, sizeof(chunks) / sizeof(chunks[0]), "{}");
}

static void test_split_header_is_assembled(void) {
  const char *chunks[] = {"POST / HTTP/1.1\r\nContent-Len", "gth: 2\r\n\r\n{}"};
  assert_complete_body(chunks, sizeof(chunks) / sizeof(chunks[0]), "{}");
}

static void test_body_is_assembled_across_three_chunks(void) {
  const char *chunks[] = {"POST / HTTP/1.1\r\nContent-Length: 7\r\n\r\n",
                          "{\"a", "\":1", "}"};
  assert_complete_body(chunks, sizeof(chunks) / sizeof(chunks[0]), "{\"a\":1}");
}

static void test_duplicate_content_lengths(void) {
  const char *equal[] = {"POST / HTTP/1.1\r\nContent-Length: 2\r\n"
                         "Content-Length: 2\r\n\r\n{}"};
  assert_complete_body(equal, sizeof(equal) / sizeof(equal[0]), "{}");
  const char *conflict[] = {"POST / HTTP/1.1\r\nContent-Length: 2\r\n"
                            "Content-Length: 3\r\n\r\n{}"};
  HttpRequestBuffer request;
  assert(read_chunks(conflict, 1, &request) == REQUEST_MALFORMED);
  free(request.data);
}

static void test_malformed_length_is_rejected(void) {
  const char *chunks[] = {"POST /api/withdraw HTTP/1.1\r\n"
                          "Content-Length: twelve\r\n\r\n"};
  reset_receive(chunks, sizeof(chunks) / sizeof(chunks[0]));
  HttpRequestBuffer request;
  assert(read_http_request(42, &request) == REQUEST_MALFORMED);
  assert(request.header_bytes > 0);
  free(request.data);
}

static void test_incomplete_body_is_rejected(void) {
  const char *chunks[] = {"POST /api/withdraw HTTP/1.1\r\n"
                          "Content-Length: 10\r\n\r\n",
                          "{\"x\":1}", NULL};
  reset_receive(chunks, sizeof(chunks) / sizeof(chunks[0]));
  HttpRequestBuffer request;
  assert(read_http_request(42, &request) == REQUEST_INCOMPLETE);
  assert(request.expected_body_bytes == 10);
  assert(request.received_bytes - request.header_bytes == 7);
  free(request.data);
}

static void test_oversized_body_is_rejected(void) {
  const char *chunks[] = {"POST /api/withdraw HTTP/1.1\r\n"
                          "Content-Length: 1048577\r\n\r\n"};
  reset_receive(chunks, sizeof(chunks) / sizeof(chunks[0]));
  HttpRequestBuffer request;
  assert(read_http_request(42, &request) == REQUEST_TOO_LARGE);
  free(request.data);
}

static void test_partial_request_waits_with_a_deadline(void) {
  const char *chunks[] = {"POST /api/withdraw HTTP/1.1\r\n"};
  reset_receive(chunks, sizeof(chunks) / sizeof(chunks[0]));
  const int results[] = {0};
  set_select_results(results, sizeof(results) / sizeof(results[0]));
  HttpRequestBuffer request;
  assert(read_http_request(42, &request) == REQUEST_TIMED_OUT);
  assert(select_calls > 0);
  free(request.data);
}

static void test_deadline_is_absolute_across_reads(void) {
  const char *chunks[] = {"POST / HTTP/1.1\r\nContent-Length: 1\r\n\r\n"};
  reset_receive(chunks, sizeof(chunks) / sizeof(chunks[0]));
  const int results[] = {1, 0};
  const uint64_t times[] = {1000, 1000, 5500};
  set_select_results(results, 2);
  set_monotonic_values(times, 3);
  HttpRequestBuffer request;
  assert(read_http_request(42, &request) == REQUEST_TIMED_OUT);
  assert(select_calls == 2);
  assert(select_timeouts[0] == 5000);
  assert(select_timeouts[1] == 500);
  free(request.data);
}

static void test_transfer_encoding_is_rejected(void) {
  const char *chunks[] = {"POST /api/withdraw HTTP/1.1\r\n"
                          "Transfer-Encoding: chunked\r\n\r\n"};
  reset_receive(chunks, sizeof(chunks) / sizeof(chunks[0]));
  HttpRequestBuffer request;
  assert(read_http_request(42, &request) != REQUEST_COMPLETE);
  free(request.data);
}

static void test_te_with_content_length_is_rejected(void) {
  const char *chunks[] = {"POST /api/withdraw HTTP/1.1\r\n"
                          "Transfer-Encoding: chunked\r\n"
                          "Content-Length: 4\r\n\r\n"};
  reset_receive(chunks, sizeof(chunks) / sizeof(chunks[0]));
  HttpRequestBuffer request;
  assert(read_http_request(42, &request) != REQUEST_COMPLETE);
  free(request.data);
}
static void test_logs_are_correlated_and_sanitized(void) {
  HttpRequestBuffer request = {0};
  strcpy(request.method, "\x1b[31mGET");
  strcpy(request.path, "/ok?token=secret\x1b[2J");
  captured_log[0] = '\0';
  http_log_exchange(&request, REQUEST_COMPLETE, 200, 0, true);
  assert(strchr(captured_log, '\x1b') == NULL &&
         strstr(captured_log, "secret") == NULL);
  assert(strstr(captured_log, "request_id=") != NULL);
  assert(strstr(captured_log, "client_request_id=") != NULL);

  // A request too broken to yield a method or a path is still LOGGED, and its
  // placeholders must survive to the line. The first placeholder was "?", which
  // is also the query delimiter this logger truncates at -- so `path=?` was
  // erased back to `path=`, the empty field the placeholder exists to prevent,
  // and every unparseable request logged as though the path had simply been
  // omitted. [HTTP-SERVER]
  HttpRequestBuffer unparseable = {0};
  strcpy(unparseable.method, HTTP_LOG_UNKNOWN);
  strcpy(unparseable.path, HTTP_LOG_UNKNOWN);
  strcpy(unparseable.client_request_id, HTTP_LOG_UNKNOWN);
  captured_log[0] = '\0';
  http_log_exchange(&unparseable, REQUEST_MALFORMED, 400, 34, true);
  assert(strstr(captured_log, "method=" HTTP_LOG_UNKNOWN " ") != NULL);
  assert(strstr(captured_log, "path=" HTTP_LOG_UNKNOWN " ") != NULL &&
         "the path placeholder must reach the log, not be eaten as a query");
  assert(strstr(captured_log, "path= ") == NULL);
}

static void test_bridge_request_id_is_parsed_and_sanitized(void) {
  const char *chunks[] = {"GET / HTTP/1.1\r\n"
                          "X-Osprey-Request-Id: bridge-\x1b[31m\r\n\r\n"};
  HttpRequestBuffer request;
  assert(read_chunks(chunks, 1, &request) == REQUEST_COMPLETE);
  assert(strchr(request.client_request_id, '\x1b') == NULL);
  assert(strncmp(request.client_request_id, "bridge-", 7) == 0);
  captured_log[0] = '\0';
  http_log_exchange(&request, REQUEST_COMPLETE, 200, 0, true);
  assert(strstr(captured_log, request.client_request_id) != NULL);
  free(request.data);
}

static void test_server_request_ids_increase(void) {
  const char *chunks[] = {"GET / HTTP/1.1\r\n\r\n"};
  HttpRequestBuffer first;
  assert(read_chunks(chunks, 1, &first) == REQUEST_COMPLETE);
  uint64_t first_id = first.server_request_id;
  free(first.data);
  HttpRequestBuffer second;
  assert(read_chunks(chunks, 1, &second) == REQUEST_COMPLETE);
  assert(second.server_request_id > first_id);
  free(second.data);
}

static void test_accepted_socket_suppresses_sigpipe(void) {
  socket_sigpipe_suppressed = false;
  setsockopt_result = 0;
#ifdef SO_NOSIGPIPE
  assert(http_configure_client_socket(42));
  assert(socket_sigpipe_suppressed);
  setsockopt_result = -1;
  assert(!http_configure_client_socket(42));
#else
  assert(HTTP_SEND_FLAGS != 0);
#endif
}

static void test_self_stop_defers_server_destruction(void) {
  static char address[] = "127.0.0.1";
  reset_server_fixture(address);
  server.is_listening = true;
  server.loop_scheduled = true;
  server.thread_known = true;
  server.server_fiber_id = 1;
  server.server_thread = pthread_self();
  protect_server_fixture(address);
  assert(http_stop_server(1) == 0);
  assert(protected_frees[0] == address);
  assert(protected_frees[1] == &server);
  pthread_mutex_destroy(&server.mutex);
}

static void test_external_stop_awaits_before_destruction(void) {
  static char address[] = "127.0.0.1";
  reset_server_fixture(address);
  server.server_fiber_id = 9;
  protect_server_fixture(address);
  awaited_fiber_id = -1;
  assert(http_stop_server(1) == 0);
  assert(awaited_fiber_id == 9);
  assert(protected_frees[0] == NULL && protected_frees[1] == NULL);
}

static void run_framing_tests(void) {
  test_split_body_reaches_handler();
  test_same_read_header_and_body();
  test_split_header_is_assembled();
  test_body_is_assembled_across_three_chunks();
  test_duplicate_content_lengths();
  test_malformed_length_is_rejected();
  test_incomplete_body_is_rejected();
  test_oversized_body_is_rejected();
}

// --- rejection paths [HTTP-SERVER] -------------------------------------------
// "The listener rejects malformed framing, transfer encoding, timeouts, and
// oversized requests before invoking the handler." Each case below drives one
// of those rejections to its RETURN VALUE, because a request the listener
// merely mishandles quietly is one the handler ends up seeing.

// EINTR means "the wait was interrupted, ask again"; every other errno means
// the read is over. Conflating them either spins forever on a dead socket or
// drops a live request on a signal.
static void test_interrupt_detection_is_eintr_and_nothing_else(void) {
  errno = EINTR;
  assert(http_socket_interrupted());
  errno = EBADF;
  assert(!http_socket_interrupted());
  errno = 0;
  assert(!http_socket_interrupted());
}

// Log tokens are attacker-controlled: a request id with a newline in it would
// otherwise forge a whole log line. Every byte outside printable ASCII becomes
// '_', the token is NUL-terminated within the capacity given, and a capacity of
// zero writes NOTHING rather than a stray terminator into a caller's buffer.
static void test_log_token_sanitizing_covers_its_boundaries(void) {
  char out[8];
  memset(out, 'x', sizeof(out));
  sanitize_log_token("abc", out, 0);
  assert(out[0] == 'x' && "a zero capacity is not room for a terminator");

  sanitize_log_token(NULL, out, sizeof(out));
  assert(out[0] == '\0' && "no token is the empty token");

  sanitize_log_token("a b\tc\x7f", out, sizeof(out));
  assert(strcmp(out, "a_b_c_") == 0 && "space, tab and DEL are all unsafe");

  sanitize_log_token("abcdefghij", out, 4);
  assert(strcmp(out, "abc") == 0 && "the terminator fits inside the capacity");
}

// Header NAMES: a line with no colon, a line whose colon is first, and a name
// containing a space are all malformed. A name that merely resembles a framing
// header is not that header -- matching it would let `Xontent-Length` set the
// body length.
static void test_malformed_header_names_are_rejected(void) {
  HttpRequestBuffer request;
  const char *no_colon[] = {"POST / HTTP/1.1\r\nNoColonHere\r\n\r\n"};
  assert(read_chunks(no_colon, 1, &request) == REQUEST_MALFORMED);
  free(request.data);

  const char *empty_name[] = {"POST / HTTP/1.1\r\n: value\r\n\r\n"};
  assert(read_chunks(empty_name, 1, &request) == REQUEST_MALFORMED);
  free(request.data);

  const char *spaced_name[] = {"POST / HTTP/1.1\r\nBad Header: x\r\n\r\n"};
  assert(read_chunks(spaced_name, 1, &request) == REQUEST_MALFORMED);
  free(request.data);

  // Same length as "Content-Length", one letter different: it must NOT set the
  // body length, so the request completes with an empty body.
  const char *near_miss[] = {"POST / HTTP/1.1\r\nXontent-Length: 2\r\n\r\n"};
  assert(read_chunks(near_miss, 1, &request) == REQUEST_COMPLETE);
  assert(request.expected_body_bytes == 0);
  assert(strcmp(request.body, "") == 0);
  free(request.data);
}

// Optional whitespace around a header value is not part of the value: RFC 9110
// allows it, so trimming it is what makes `Content-Length: 2  ` mean 2 rather
// than malformed.
static void test_header_values_tolerate_surrounding_space(void) {
  HttpRequestBuffer request;
  const char *padded[] = {"POST / HTTP/1.1\r\nContent-Length:   2  \r\n\r\n{}"};
  assert(read_chunks(padded, 1, &request) == REQUEST_COMPLETE);
  assert(request.expected_body_bytes == 2);
  assert(strcmp(request.body, "{}") == 0);
  free(request.data);
}

// A client request id longer than the field is TRUNCATED, never written past
// the end of it. The truncation is the interesting half: the field is a fixed
// array inside the request buffer, and a memcpy sized by the header would
// overwrite everything after it.
static void test_over_long_client_request_id_is_truncated(void) {
  char headers[256];
  char identifier[HTTP_CLIENT_REQUEST_ID_BYTES * 2];
  memset(identifier, 'a', sizeof(identifier) - 1U);
  identifier[sizeof(identifier) - 1U] = '\0';
  int written = snprintf(headers, sizeof(headers),
                         "POST / HTTP/1.1\r\nX-Osprey-Request-Id: %s\r\n\r\n",
                         identifier);
  assert(written > 0 && (size_t)written < sizeof(headers));

  HttpRequestBuffer request;
  const char *chunks[] = {headers};
  assert(read_chunks(chunks, 1, &request) == REQUEST_COMPLETE);
  assert(strlen(request.client_request_id) == HTTP_CLIENT_REQUEST_ID_BYTES - 1U);
  assert(request.client_request_id[0] == 'a');
  free(request.data);
}

// Oversize is rejected in TWO distinct places, and only one of them was
// exercised: a header block past MAX_HTTP_HEADER_BYTES that nonetheless ends
// properly, and a well-formed Content-Length that no buffer could hold once the
// headers are accounted for.
static void test_oversized_headers_and_body_are_rejected(void) {
  static char huge[MAX_HTTP_HEADER_BYTES + 128U];
  int prefix = snprintf(huge, sizeof(huge), "POST / HTTP/1.1\r\nX-Pad: ");
  assert(prefix > 0);
  size_t padding = sizeof(huge) - (size_t)prefix - 5U;
  memset(huge + prefix, 'p', padding);
  memcpy(huge + (size_t)prefix + padding, "\r\n\r\n", 5U);

  HttpRequestBuffer request;
  const char *chunks[] = {huge};
  assert(read_chunks(chunks, 1, &request) == REQUEST_TOO_LARGE &&
         "a complete but over-long header block is still too large");
  free(request.data);

  char body_headers[128];
  int written = snprintf(body_headers, sizeof(body_headers),
                         "POST / HTTP/1.1\r\nContent-Length: %u\r\n\r\n",
                         MAX_HTTP_REQUEST_BYTES);
  assert(written > 0 && (size_t)written < sizeof(body_headers));
  const char *body_chunks[] = {body_headers};
  assert(read_chunks(body_chunks, 1, &request) == REQUEST_TOO_LARGE &&
         "headers plus body must fit the buffer, not just the body");
  free(request.data);
}

// The wait before a read has three failure modes and they are NOT the same
// answer: a descriptor select cannot watch, a deadline already spent, and a
// select that failed for a reason other than a signal.
static void test_unreadable_descriptor_and_timeout_are_distinguished(void) {
  HttpRequestBuffer request;
  const char *nothing[] = {NULL};

  reset_receive(nothing, 1);
  assert(read_http_request(-1, &request) == REQUEST_READ_FAILED &&
         "a descriptor select cannot watch is a read failure, not a timeout");
  assert(select_calls == 0 && "and it is refused before select is consulted");
  free(request.data);

  // The deadline is absolute: the clock reads past it before the first select.
  const uint64_t spent[] = {1000U, 1000U + HTTP_REQUEST_TIMEOUT_MS};
  reset_receive(nothing, 1);
  set_monotonic_values(spent, sizeof(spent) / sizeof(spent[0]));
  assert(read_http_request(42, &request) == REQUEST_TIMED_OUT);
  assert(select_calls == 0 && "an expired deadline never waits");
  free(request.data);

  // A select that failed for a reason other than EINTR ends the read.
  const int failed[] = {-1};
  reset_receive(nothing, 1);
  set_select_results(failed, 1);
  select_errno = EBADF;
  assert(read_http_request(42, &request) == REQUEST_READ_FAILED);
  free(request.data);

  // EINTR is retried, and the retry succeeds: the request is read normally.
  const int interrupted_then_ready[] = {-1, 1};
  const char *complete[] = {"POST / HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}"};
  reset_receive(complete, 1);
  set_select_results(interrupted_then_ready,
                     sizeof(interrupted_then_ready) / sizeof(int));
  select_errno = EINTR;
  assert(read_http_request(42, &request) == REQUEST_COMPLETE);
  assert(select_calls == 2 && "the interrupted wait was retried, not abandoned");
  assert(strcmp(request.body, "{}") == 0);
  free(request.data);
  select_errno = 0;
}

// A socket that ERRORS mid-request is a read failure; one that merely ends is
// an incomplete request. Answering the same way for both would let a client
// truncate a body and have the handler see it as whole.
static void test_socket_error_and_close_differ(void) {
  HttpRequestBuffer request;
  // No chunk left: the mock recv reports -1, the way a broken socket does.
  const char *starved[] = {"POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\n"};
  assert(read_chunks(starved, 1, &request) == REQUEST_READ_FAILED);
  free(request.data);

  // A clean end-of-stream partway through a body is incomplete, not an error.
  const char *closed[] = {"POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\nab", NULL};
  assert(read_chunks(closed, 2, &request) == REQUEST_INCOMPLETE);
  free(request.data);
}

// A request line that is not `<method> <target>` is malformed, even when the
// header block around it is perfectly well formed.
static void test_missing_request_line_is_rejected(void) {
  HttpRequestBuffer request;
  const char *headerless[] = {"\r\n\r\n"};
  assert(read_chunks(headerless, 1, &request) == REQUEST_MALFORMED);
  free(request.data);
}

// [HTTP-SERVER-CLIENT-GONE] A ready descriptor can carry data or EOF.
static void test_serving_client_disconnect(void) {
  assert(http_client_gone() == 0);
#ifndef _WIN32
  http_serving_client(FD_SETSIZE);
  assert(http_client_gone() == 0);
#endif
  const char *data_then_close[] = {"x", NULL};
  const int readiness[] = {0, 1, 1, 1};
  reset_receive(data_then_close, 2);
  set_select_results(readiness, 4);
  http_serving_client(42);
  receive_flags = MSG_PEEK;
  assert(http_client_gone() == 0 && receive_index == 0);
  assert(http_client_gone() == 0 && receive_index == 1);
  assert(http_client_gone() == 1 && receive_index == 2);
  assert(http_client_gone() == 1); // A failed peek also indicates disconnect.
  receive_flags = 0;
  http_serving_client(-1);
  assert(http_client_gone() == 0 && select_calls == 4);
}

static void run_hardening_tests(void) {
  test_partial_request_waits_with_a_deadline();
  test_deadline_is_absolute_across_reads();
  test_transfer_encoding_is_rejected();
  test_te_with_content_length_is_rejected();
  test_logs_are_correlated_and_sanitized();
  test_bridge_request_id_is_parsed_and_sanitized();
  test_server_request_ids_increase();
  test_accepted_socket_suppresses_sigpipe();
  test_self_stop_defers_server_destruction();
  test_external_stop_awaits_before_destruction();
  test_interrupt_detection_is_eintr_and_nothing_else();
  test_log_token_sanitizing_covers_its_boundaries();
  test_malformed_header_names_are_rejected();
  test_header_values_tolerate_surrounding_space();
  test_over_long_client_request_id_is_truncated();
  test_oversized_headers_and_body_are_rejected();
  test_unreadable_descriptor_and_timeout_are_distinguished();
  test_socket_error_and_close_differ();
  test_missing_request_line_is_rejected();
  test_serving_client_disconnect();
}

int main(void) {
  run_framing_tests();
  run_hardening_tests();
  return 0;
}
