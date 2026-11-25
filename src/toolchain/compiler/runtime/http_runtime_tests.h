#ifndef HTTP_RUNTIME_TESTS_H
#define HTTP_RUNTIME_TESTS_H

#include "memory_hooks.h"
#include "string_runtime.h"
#include "http_shared.h"

#include <assert.h>
#include <stdlib.h>
#include <string.h>

// `*out` as a complete reply with no stream: what a test handler hands back.
static inline HttpResponse *test_reply(HttpResponse *out, int64_t status, const char *headers,
                                       const char *content_type, const char *body) {
  *out = (HttpResponse){.status = status,
                        .headers = (char *)(uintptr_t)headers,
                        .contentType = (char *)(uintptr_t)content_type,
                        .streamFd = -1,
                        .isComplete = true,
                        .partialBody = (char *)(uintptr_t)body};
  return out;
}

// `len` bytes of `fill` and a NUL, on the heap. The caller frees it.
static inline char *test_bytes_filled(size_t len, char fill) {
  char *bytes = malloc(len + 1);
  assert(bytes != NULL);
  memset(bytes, fill, len);
  bytes[len] = '\0';
  return bytes;
}

// A heap copy of exactly `len` bytes plus a NUL, so a sanitizer build traps any
// read past the end. The caller frees it.
static inline char *test_bytes_copy(const char *bytes, size_t len) {
  return memcpy(test_bytes_filled(len, 0), bytes, len);
}

// Shared declarations for the HTTP runtime suite TUs (http_runtime_tests.c,
// http_origin_tests.c, http_tls_tests.c, http_server_live_tests.c), which link
// into one suite binary.

// Runtime APIs under test.
extern int64_t http_create_client(char *base_url, int64_t timeout);
extern int64_t http_close_client(int64_t client_id);
extern int64_t http_get(int64_t client_id, char *path, char *headers);
extern int64_t http_post(int64_t client_id, char *path, char *body,
                         char *headers);
extern int64_t http_put(int64_t client_id, char *path, char *body,
                        char *headers);
extern int64_t http_delete(int64_t client_id, char *path, char *headers);

extern int64_t http_create_server(int64_t port, char *address);
extern int64_t http_listen(int64_t server_id, HttpRequestHandler handler);
extern int64_t http_stop_server(int64_t server_id);

extern int64_t http_get_response(int64_t client_id, char *path, char *headers);
extern int64_t http_post_response(int64_t client_id, char *path, char *body,
                                  char *headers);
extern int64_t http_response_status(int64_t handle);
extern char *http_response_body(int64_t handle);
extern char *http_response_header(int64_t handle, char *name);
extern int64_t http_response_free(int64_t handle);

// Defined in http_server_live_tests.c; the lifecycle test needs a real
// callback now that http_listen is declared with its true signature.
HttpResponse *http_test_server_handler(char *method, char *path,
                                       char *headers, char *body);

// Raw loopback client helpers, used by both live-server TUs: connect to a
// listener on `port`, read one reply to EOF, and one full exchange.
int server_test_connect(int port);
void server_test_read_to_eof(int fd, char *out, size_t capacity);
void server_test_exchange(int port, const char *request, char *reply,
                          size_t capacity);

// The OS-assigned port the live-server TUs share (http_server_live_tests.c):
// their listeners, their clients and this TU's creation checks all use it
// [CI-TEST-PORTS].
int server_test_port(void);

// A raw TCP origin answering with exact bytes, NULs included, defined in
// http_origin_tests.c. It listens on one OS-assigned port, shared with the
// clients below through origin_port() [CI-TEST-PORTS].
int origin_port(void);

typedef struct {
  int listen_fd;
  const char *reply; // NULL: accept and close without writing a byte
  size_t reply_len;
  unsigned reply_delay_us; // wait this long before replying
  bool hold_open; // keep the socket open after replying, until the client leaves
  unsigned drip_us; // send one byte at a time, this far apart
  bool raw_request; // the peer speaks no HTTP (a TLS hello): take one segment, then reply
  size_t trickle_bytes; // when set: read this much at a time, trickle_us apart,
  unsigned trickle_us;  // until the client leaves, and never reply
  char request[MAX_HTTP_BUFFER];
  pthread_t thread;
} ScriptedOrigin;

typedef long (*origin_receive)(void *conn, char *out, size_t capacity);
void origin_read_request_via(void *conn, origin_receive receive, char *out, size_t capacity);
void origin_read_request(int fd, char *out, size_t capacity);
void origin_start(ScriptedOrigin *origin, const char *reply);
void origin_start_bytes(ScriptedOrigin *origin, const char *reply, size_t len);
void origin_stop(ScriptedOrigin *origin);
int64_t origin_client(void);
// A client of the origin port that waits `timeout_ms` for each exchange.
int64_t origin_client_timed(int64_t timeout_ms);

// One exchange per call on shared_origin_client, each its own origin. Handles are
// never reused, so the hundreds of cases share one client rather than spend handles.
extern int64_t shared_origin_client;
extern bool origin_holds_connections;
extern unsigned origin_drip_us;
int64_t origin_exchange(const char *reply, size_t len);
void expect_response_n(const char *reply, size_t len, int64_t status, const char *body);
void expect_failure_n(const char *reply, size_t len, int64_t code, const char *message);
void expect_refused_n(const char *reply, size_t len, const char *message);

void expect_response_bytes_n(const char *reply, size_t len, int64_t status,
                             const char *body, size_t body_len);

#define EXPECT_REFUSED(reply, message) \
  expect_refused_n(reply, sizeof(reply) - 1, message)
#define EXPECT_RESPONSE(reply, status, body) \
  expect_response_bytes_n(reply, sizeof(reply) - 1, status, body, sizeof(body) - 1)

// Test entry points, invoked in order by run_all_http_tests (main TU).
// The first two drive the allocation injector that lives in the main TU.
void test_alloc_fail_off(void);
void test_alloc_fail_size(size_t bytes);
long test_alloc_live(void);
void test_http_create_client(void);
void test_http_create_server(void);
void test_http_server_lifecycle(void);
void test_rejection_mapping(void);
void test_sanitize_log_token(void);
void test_socket_interrupted(void);
void test_response_handle_rejection(void);
void test_live_loopback_exchange(void);
void test_post_response_loopback(void);
void test_client_handle_bounds(void);
void test_client_handle_exhaustion(void);
void test_client_chunked_response(void);
void test_client_status_line_handling(void);
void test_client_methods_and_body(void);
void test_client_large_request_body(void);
void test_client_peer_closes_during_body(void);
void test_client_transport_failures(void);
void test_client_large_response(void);
void test_client_malformed_chunks(void);
void test_chunked_decoder(void);
void test_response_framing(void);
void test_response_read_bounds(void);
void expect_scan_agrees(const char *bytes, size_t len);
void test_client_response_allocation_failures(void);
void test_client_tls_handshake_failure(void);
void test_client_tls_peer_verification(void);
void test_server_rejects_bad_requests(void);
void test_server_self_stop_from_handler(void);
void test_server_handle_bounds(void);
void test_server_labels_an_unparseable_request(void);
void test_closure_listener_serves_requests(void);
void test_closure_listen_rejections(void);
void test_stop_server_with_client_connected(void);
void test_spawn_with_handlers_survives_every_failed_allocation(void);
void test_listener_spawn_failure_is_reported(void);
void test_client_gone_follows_the_served_client(void);
void test_keep_alive_waits_for_its_listeners(void);
// [HTTP-SERVER-CONNECTIONS] http_server_connection_tests.c
void test_server_serves_a_client_while_another_is_silent(void);
void test_server_keeps_alive_as_node_does(void);
void test_server_ends_half_closed_and_pipelined_connections(void);
void test_server_stop_ends_idle_connections(void);
void test_server_serves_concurrent_keep_alive_clients(void);
void test_server_names_statuses_as_node_does(void);
void test_server_handle_exhaustion(void);

#endif // HTTP_RUNTIME_TESTS_H
