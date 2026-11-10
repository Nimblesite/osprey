// Scripted-origin client tests, split from http_runtime_tests.c so each
// test TU stays under the repo size budget. The origin is a raw TCP server
// answering with EXACT bytes, so the client is asserted against wire
// responses the Osprey server would never produce [HTTP-STATUS-CLIENT],
// [HTTP-RESPONSE-HANDLE].
#include "test_loopback.h"
#include "http_runtime_tests.h"
#include "http_shared.h"
#include "io_error.h"
#include "socket_safety.h"

#include <signal.h>
#include <assert.h>
#include <stdio.h>

// A chunked response is decoded before the caller sees it, and header lookup
// ignores case [HTTP-RESPONSE-HANDLE].
void test_client_chunked_response(void) {
  ScriptedOrigin origin;
  origin_start(&origin, "HTTP/1.1 200 OK\r\n"
                        "Transfer-Encoding: chunked\r\n"
                        "X-Mixed-Case: Yes\r\n"
                        "\r\n"
                        "5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n");
  int64_t client_id = origin_client();
  int64_t handle = http_get_response(client_id, (char *)(uintptr_t) "/chunked",
                                     (char *)(uintptr_t) "");
  origin_stop(&origin);
  assert(handle >= 1);
  assert(http_response_status(handle) == 200);
  char *body = http_response_body(handle);
  assert(body != NULL && strcmp(body, "hello world") == 0);
  osp_release(body);
  char *header = http_response_header(handle, (char *)(uintptr_t) "x-mixed-case");
  assert(header != NULL && strcmp(header, "Yes") == 0);
  free(header);
  // A header name that is a prefix of a real one must NOT match.
  assert(http_response_header(handle, (char *)(uintptr_t) "X-Mixed") == NULL);
  assert(http_response_header(handle, NULL) == NULL);
  assert(http_response_free(handle) == 0);
  assert(strstr(origin.request, "GET /chunked HTTP/1.1\r\n") == origin.request);
  assert(strstr(origin.request, "Host: 127.0.0.1\r\n") != NULL);
  assert(strstr(origin.request, "Connection: close\r\n") != NULL);
  http_close_client(client_id);
}

// HTTP/1.0 status lines parse; anything that is not a status line is -8, and a
// server that closes without writing is -7 [HTTP-STATUS-CLIENT].
void test_client_status_line_handling(void) {
  ScriptedOrigin origin;
  origin_start(&origin, "HTTP/1.0 404 Not Found\r\n\r\nmissing");
  int64_t client_id = origin_client();
  assert(http_get(client_id, (char *)(uintptr_t) "/gone",
                  (char *)(uintptr_t) "") == 404);
  origin_stop(&origin);

  origin_start(&origin, "NOT-HTTP AT ALL\r\n\r\n");
  assert(http_get(client_id, (char *)(uintptr_t) "/junk",
                  (char *)(uintptr_t) "") == -8);
  origin_stop(&origin);

  origin_start(&origin, NULL); // accepted, then closed in silence
  assert(http_get(client_id, (char *)(uintptr_t) "/silent",
                  (char *)(uintptr_t) "") == -7);
  origin_stop(&origin);
  http_close_client(client_id);
}

// Every method surface reaches the wire with its own verb, and a body arrives
// with a matching Content-Length [HTTP-STATUS-CLIENT].
void test_client_methods_and_body(void) {
  const char *ok = "HTTP/1.1 204 No Content\r\n\r\n";
  ScriptedOrigin origin;
  int64_t client_id = origin_client();

  origin_start(&origin, ok);
  assert(http_post(client_id, (char *)(uintptr_t) "/submit",
                   (char *)(uintptr_t) "data",
                   (char *)(uintptr_t) "X-Trace: 7\r\n") == 204);
  origin_stop(&origin);
  assert(strstr(origin.request, "POST /submit HTTP/1.1\r\n") == origin.request);
  assert(strstr(origin.request, "X-Trace: 7\r\n") != NULL);
  assert(strstr(origin.request, "Content-Length: 4\r\n\r\ndata") != NULL);

  origin_start(&origin, ok);
  assert(http_put(client_id, (char *)(uintptr_t) "/item",
                  (char *)(uintptr_t) "v", (char *)(uintptr_t) "") == 204);
  origin_stop(&origin);
  assert(strstr(origin.request, "PUT /item HTTP/1.1\r\n") == origin.request);

  origin_start(&origin, ok);
  assert(http_delete(client_id, (char *)(uintptr_t) "/item",
                     (char *)(uintptr_t) "") == 204);
  origin_stop(&origin);
  assert(strstr(origin.request, "DELETE /item HTTP/1.1\r\n") == origin.request);
  http_close_client(client_id);
}

// A request larger than any fixed buffer reaches the origin whole and exact:
// model APIs take request bodies of megabytes, and nothing but the request may
// leave the process [HTTP-STATUS-CLIENT].
#define LARGE_REQUEST_BODY (512 * 1024)

typedef struct {
  int listen_fd;
  char *request;
  size_t capacity;
} LargeOrigin;

static void *large_origin_thread(void *arg) {
  LargeOrigin *origin = (LargeOrigin *)arg;
  int fd = accept(origin->listen_fd, NULL, NULL);
  if (fd < 0) {
    return NULL;
  }
  origin_read_request(fd, origin->request, origin->capacity);
  const char *reply = "HTTP/1.1 204 No Content\r\n\r\n";
  (void)osp_socket_send(fd, reply, strlen(reply));
  close(fd);
  return NULL;
}

static void *closing_origin_thread(void *arg) {
  int fd = accept(*(int *)arg, NULL, NULL);
  if (fd >= 0) {
    close(fd);
  }
  return NULL;
}

// An origin that hangs up while a body is being sent is an error for the caller,
// not a SIGPIPE that kills the process [HTTP-STATUS-CLIENT].
void test_client_peer_closes_during_body(void) {
#ifndef _WIN32
  void (*previous)(int) = signal(SIGPIPE, SIG_DFL);
  size_t size = 16 * 1024 * 1024;
  char *body = malloc(size + 1);
  assert(body != NULL);
  memset(body, 'x', size);
  body[size] = '\0';
  int listen_fd = test_loopback_listen(ORIGIN_PORT);
  pthread_t thread;
  assert(pthread_create(&thread, NULL, closing_origin_thread, &listen_fd) == 0);
  int64_t client_id = origin_client();
  assert(http_post(client_id, (char *)(uintptr_t) "/gone", body,
                   (char *)(uintptr_t) "") < 0);
  assert(pthread_join(thread, NULL) == 0);
  close(listen_fd);
  http_close_client(client_id);
  free(body);
  signal(SIGPIPE, previous);
#endif
}

void test_client_large_request_body(void) {
  char *body = malloc(LARGE_REQUEST_BODY + 1);
  assert(body != NULL);
  for (size_t i = 0; i < LARGE_REQUEST_BODY; i++) {
    body[i] = (char)('a' + (char)(i % 26));
  }
  body[LARGE_REQUEST_BODY] = '\0';
  LargeOrigin origin;
  origin.capacity = LARGE_REQUEST_BODY + MAX_HTTP_BUFFER;
  origin.request = calloc(origin.capacity, 1);
  assert(origin.request != NULL);
  origin.listen_fd = test_loopback_listen(ORIGIN_PORT);
  pthread_t thread;
  assert(pthread_create(&thread, NULL, large_origin_thread, &origin) == 0);

  int64_t client_id = origin_client();
  assert(http_post(client_id, (char *)(uintptr_t) "/large", body,
                   (char *)(uintptr_t) "X-Trace: 9") == 204);
  assert(pthread_join(thread, NULL) == 0);
  close(origin.listen_fd);
  http_close_client(client_id);

  char expected[64];
  snprintf(expected, sizeof(expected), "Content-Length: %d\r\n\r\n",
           LARGE_REQUEST_BODY);
  const char *sent = strstr(origin.request, expected);
  assert(sent != NULL);
  assert(strstr(origin.request, "X-Trace: 9\r\n") != NULL);
  sent += strlen(expected);
  assert(strlen(sent) == LARGE_REQUEST_BODY);
  assert(memcmp(sent, body, LARGE_REQUEST_BODY) == 0);
  free(origin.request);
  free(body);
}

// Transport failures each report their own code rather than a status
// [HTTP-STATUS-CLIENT].
void test_client_transport_failures(void) {
  assert(http_create_client(NULL, 5000) == -1);
  assert(http_create_client((char *)(uintptr_t) "http://h", -1) == -2);
  assert(http_create_client((char *)(uintptr_t) "http://:0/", 5000) == -4);

  int64_t refused =
      http_create_client((char *)(uintptr_t) "http://127.0.0.1:1", 1000);
  assert(refused > 0);
  assert(http_get(refused, (char *)(uintptr_t) "/", (char *)(uintptr_t) "") ==
         -5);
  assert(http_get(refused, NULL, (char *)(uintptr_t) "") == -2);
  http_close_client(refused);

  char long_host[400];
  memcpy(long_host, "http://", 7);
  memset(long_host + 7, 'a', sizeof(long_host) - 10);
  memcpy(long_host + sizeof(long_host) - 3, "/x", 3);
  int64_t unresolvable = http_create_client(long_host, 1000);
  assert(unresolvable > 0);
  assert(http_get(unresolvable, (char *)(uintptr_t) "/",
                  (char *)(uintptr_t) "") == -4);
  http_close_client(unresolvable);
}

// A client handle outside the table must be REJECTED, never used as an index:
// clients[] holds MAX_CLIENTS entries and a raw index is an out-of-bounds
// access [HTTP-STATUS-CLIENT].
void test_client_large_response(void) {
  enum { BIG_BODY = 12000 };
  static char reply[BIG_BODY + 256];
  int head = snprintf(reply, sizeof(reply),
                      "HTTP/1.1 200 OK\r\nContent-Length: %d\r\n\r\n", BIG_BODY);
  assert(head > 0);
  memset(reply + head, 'Z', BIG_BODY);
  reply[head + BIG_BODY] = '\0';
  ScriptedOrigin origin;
  origin_start(&origin, reply);
  int64_t client_id = origin_client();
  int64_t handle = http_get_response(client_id, (char *)(uintptr_t) "/big",
                                     (char *)(uintptr_t) "");
  origin_stop(&origin);
  assert(handle >= 1);
  char *body = http_response_body(handle);
  assert(body != NULL && strlen(body) == BIG_BODY);
  assert(body[0] == 'Z' && body[BIG_BODY - 1] == 'Z');
  osp_release(body);
  // A response with no Transfer-Encoding header is returned unchanged.
  char *length_header =
      http_response_header(handle, (char *)(uintptr_t) "Content-Length");
  assert(length_header != NULL && strcmp(length_header, "12000") == 0);
  free(length_header);
  assert(http_response_free(handle) == 0);
  http_close_client(client_id);
}

// Sends `chunks` as the body of a chunked response and returns what the client
// made of it, with the origin already finished [HTTP-CHUNKED-DECODE].
static int64_t chunked_exchange(const char *chunks, int64_t client_id) {
  static char reply[512];
  int written = snprintf(reply, sizeof(reply),
                         "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n%s",
                         chunks);
  assert(written > 0 && (size_t)written < sizeof(reply));
  ScriptedOrigin origin;
  origin_start(&origin, reply);
  int64_t handle = http_get_response(client_id, (char *)(uintptr_t) "/c",
                                     (char *)(uintptr_t) "");
  origin_stop(&origin);
  return handle;
}

static void expect_chunked_body(const char *chunks, const char *expected) {
  int64_t client_id = origin_client();
  int64_t handle = chunked_exchange(chunks, client_id);
  assert(handle >= 1);
  char *body = http_response_body(handle);
  assert(body != NULL && strcmp(body, expected) == 0);
  osp_release(body);
  assert(http_response_free(handle) == 0);
  http_close_client(client_id);
}

// Broken framing is an error with its reason on the channel. It takes no
// response slot, leaks nothing and leaves the client usable.
static void expect_chunked_refused(const char *chunks, const char *reason) {
  char message[128];
  snprintf(message, sizeof(message), "http response: chunked body: %s", reason);
  int64_t client_id = origin_client();
  long live = test_alloc_live();
  osp_io_error_clear();
  assert(chunked_exchange(chunks, client_id) == -14);
  assert(osp_io_error() != NULL && strcmp(osp_io_error(), message) == 0);
  assert(test_alloc_live() == live);
  osp_io_error_clear();
  int64_t handle = chunked_exchange("2\r\nok\r\n0\r\n\r\n", client_id);
  assert(handle >= 1);
  assert(osp_io_error() == NULL);
  assert(http_response_free(handle) == 0);
  http_close_client(client_id);
}

// Fail the response allocation by size so resolver allocations remain intact.
static void expect_response_allocation_failure(const char *reply, size_t bytes) {
  ScriptedOrigin origin;
  origin_start(&origin, reply);
  int64_t client_id = origin_client();
  test_alloc_fail_size(bytes);
  assert(http_get(client_id, (char *)(uintptr_t) "/alloc", (char *)(uintptr_t) "") == -9);
  test_alloc_fail_off();
  http_close_client(client_id);
  origin_stop(&origin);
}

// Both response-buffer OOM paths report -9 [HTTP-STATUS-CLIENT].
void test_client_response_allocation_failures(void) {
  expect_response_allocation_failure(
      "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello", MAX_HTTP_BUFFER);
  char big[MAX_HTTP_BUFFER + 512];
  size_t body = MAX_HTTP_BUFFER + 256;
  size_t head = (size_t)snprintf(big, sizeof(big),
      "HTTP/1.1 200 OK\r\nContent-Length: %zu\r\n\r\n", body);
  memset(big + head, 'x', body);
  big[head + body] = '\0';
  expect_response_allocation_failure(big, MAX_HTTP_BUFFER * 2);
}

// Running out of memory while decoding is -9, not a verdict on the body, and it
// leaks nothing. The size is one no other allocation in the exchange asks for.
static void check_chunked_allocation_failure(void) {
  char chunks[300];
  memset(chunks, 'x', sizeof(chunks) - 1);
  chunks[sizeof(chunks) - 1] = '\0';
  memcpy(chunks, "11f\r\n", 5);
  memcpy(chunks + sizeof(chunks) - 8, "\r\n0\r\n\r\n", 7);
  int64_t client_id = origin_client();
  long live = test_alloc_live();
  osp_io_error_clear();
  test_alloc_fail_size(sizeof(chunks));
  assert(chunked_exchange(chunks, client_id) == -9);
  test_alloc_fail_off();
  assert(osp_io_error() == NULL);
  assert(test_alloc_live() == live);
  int64_t handle = chunked_exchange(chunks, client_id);
  assert(handle >= 1);
  char *body = http_response_body(handle);
  assert(body != NULL && strlen(body) == 0x11f && body[0] == 'x');
  osp_release(body);
  assert(http_response_free(handle) == 0);
  http_close_client(client_id);
}

void test_client_malformed_chunks(void) {
  static const char too_big[] = "chunk size exceeds the remaining body";
  static const char bad_size[] = "invalid chunk size";
  static const char no_crlf[] = "chunk data not followed by CRLF";
  static const char cut_short[] = "truncated";
  expect_chunked_refused("7FFFFFFF\r\nhi\r\n0\r\n\r\n", too_big);
  expect_chunked_refused("FFFFFFFFFFFFFFFF\r\nhi\r\n0\r\n\r\n", too_big);
  expect_chunked_refused("10\r\nhi\r\n0\r\n\r\n", too_big);
  expect_chunked_refused("3\r\nabc\r\n100\r\nhi\r\n", too_big);
  expect_chunked_refused("10000000000000005\r\nhello\r\n0\r\n\r\n", bad_size);
  expect_chunked_refused("-1\r\nhi\r\n0\r\n\r\n", bad_size);
  expect_chunked_refused("zz\r\nhello\r\n0\r\n\r\n", bad_size);
  expect_chunked_refused("3\r\nabc\r\nzz\r\n", bad_size);
  expect_chunked_refused("5\r\nhelloXX0\r\n\r\n", no_crlf);
  expect_chunked_refused("4", cut_short);
  expect_chunked_refused("5\r\nhello\r\n", cut_short);
  expect_chunked_refused("5\r\nhello\r\n0\r\n", cut_short);
  expect_chunked_body("0\r\n\r\n", "");
  expect_chunked_body("3\r\nabc\r\n0\r\nX-T: y\r\n\r\n", "abc");
  check_chunked_allocation_failure();
}

// An https:// client negotiates TLS before it sends anything; a plaintext peer
// cannot complete that handshake and the request fails as a TLS error rather
// than being sent in the clear [HTTP-STATUS-CLIENT].
void test_client_tls_handshake_failure(void) {
  ScriptedOrigin origin;
  origin_start(&origin, NULL); // speaks no TLS
  int64_t client_id = http_create_client(
      (char *)(uintptr_t) "https://127.0.0.1:18088", 1000);
  assert(client_id > 0);
  assert(http_get(client_id, (char *)(uintptr_t) "/secure",
                  (char *)(uintptr_t) "") == -12);
  origin_stop(&origin);
  assert(origin.request[0] != 'G'); // never reached the wire as plaintext
  http_close_client(client_id);
}
