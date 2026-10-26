#include "http_server_internal.h"
#include "http_shared.h"
#include "test_alloc.h"
#include <assert.h>
#include <openssl/err.h>
#include <openssl/pem.h>
#include <openssl/ssl.h>
#include <openssl/x509v3.h>

// Runtime APIs and cross-TU test entry points: http_runtime_tests.h
#include "http_runtime_tests.h"

// Port burned by test_server_handle_exhaustion (see http_server_live_tests.c).
#define SERVER_TEST_PORT 18089

void test_http_create_client(void) {
  printf("Testing http_create_client...\n");

  // Test valid client creation
  int64_t client_id = http_create_client("http://example.com:8080", 5000);
  assert(client_id > 0);
  printf("✅ Created client with ID: %" PRId64 "\n", client_id);

  // Test invalid URL
  int64_t invalid_client = http_create_client(NULL, 5000);
  assert(invalid_client < 0);
  printf("✅ Correctly rejected NULL URL\n");

  // Test invalid timeout
  int64_t timeout_client = http_create_client("http://example.com", -1);
  assert(timeout_client < 0);
  printf("✅ Correctly rejected negative timeout\n");

  // Clean up
  http_close_client(client_id);
  printf("✅ http_create_client tests passed!\n\n");
}

void test_http_create_server(void) {
  printf("Testing http_create_server...\n");

  // Test valid server creation
  int64_t server_id = http_create_server(8080, "127.0.0.1");
  assert(server_id > 0);
  printf("✅ Created server with ID: %" PRId64 "\n", server_id);

  // Test invalid port
  int64_t invalid_server = http_create_server(0, "127.0.0.1");
  assert(invalid_server < 0);
  printf("✅ Correctly rejected invalid port\n");

  // Test invalid address
  int64_t addr_server = http_create_server(8081, NULL);
  assert(addr_server < 0);
  printf("✅ Correctly rejected NULL address\n");

  // Clean up
  http_stop_server(server_id);
  printf("✅ http_create_server tests passed!\n\n");
}

void test_http_server_lifecycle(void) {
  printf("Testing HTTP server lifecycle...\n");

  // Create server
  int64_t server_id = http_create_server(8082, "127.0.0.1");
  assert(server_id > 0);
  printf("✅ Server created: %" PRId64 "\n", server_id);

  // Start listening
  int64_t listen_result = http_listen(server_id, http_test_server_handler);
  assert(listen_result == 0);
  printf("✅ Server listening started\n");

  // Stop server
  int64_t stop_result = http_stop_server(server_id);
  assert(stop_result == 0);
  printf("✅ Server stopped successfully\n");

  printf("✅ HTTP server lifecycle tests passed!\n\n");
}

// Every RequestReadStatus maps to its exact rejection status and JSON body.
void test_rejection_mapping(void) {
  assert(http_rejection_status(REQUEST_TOO_LARGE) == 413);
  assert(http_rejection_status(REQUEST_TIMED_OUT) == 408);
  assert(http_rejection_status(REQUEST_MALFORMED) == 400);
  assert(http_rejection_status(REQUEST_INCOMPLETE) == 400);
  assert(http_rejection_status(REQUEST_READ_FAILED) == 400);
  assert(http_rejection_status(REQUEST_UNSUPPORTED_FRAMING) == 400);
  assert(strcmp(http_rejection_body(REQUEST_TOO_LARGE),
                "{\"error\":\"HTTP request too large\"}") == 0);
  assert(strcmp(http_rejection_body(REQUEST_TIMED_OUT),
                "{\"error\":\"HTTP request timed out\"}") == 0);
  assert(strcmp(http_rejection_body(REQUEST_UNSUPPORTED_FRAMING),
                "{\"error\":\"unsupported HTTP transfer encoding\"}") == 0);
  assert(strcmp(http_rejection_body(REQUEST_MALFORMED),
                "{\"error\":\"malformed HTTP request\"}") == 0);
  assert(strcmp(http_rejection_body(REQUEST_READ_FAILED),
                "{\"error\":\"malformed HTTP request\"}") == 0);
}

// sanitize_log_token maps every byte outside '!'..'~' to '_', truncates at
// capacity-1, always terminates, and leaves a zero-capacity buffer untouched.
void test_sanitize_log_token(void) {
  char out[16];
  sanitize_log_token("GET", out, sizeof(out));
  assert(strcmp(out, "GET") == 0);
  sanitize_log_token("a b\tc\r\nd", out, sizeof(out));
  assert(strcmp(out, "a_b_c__d") == 0);
  char hostile[] = {'x', 0x1b, 0x7f, (char)0x80, '!', '~', ' ', '\0'};
  sanitize_log_token(hostile, out, sizeof(out));
  assert(strcmp(out, "x___!~_") == 0);
  sanitize_log_token("abcdefghij", out, 4); // truncates to capacity-1
  assert(strcmp(out, "abc") == 0);
  sanitize_log_token(NULL, out, sizeof(out));
  assert(out[0] == '\0');
  char untouched = 'Z';
  sanitize_log_token("anything", &untouched, 0);
  assert(untouched == 'Z');
}

// http_socket_interrupted reflects errno EXACTLY — EINTR and nothing else.
void test_socket_interrupted(void) {
  errno = EINTR;
  assert(http_socket_interrupted() == true);
  errno = 0;
  assert(http_socket_interrupted() == false);
  errno = EAGAIN;
  assert(http_socket_interrupted() == false);
}

// Response handles: invalid and freed handles are rejected with -1/NULL, and
// a slot frees exactly once.
void test_response_handle_rejection(void) {
  assert(http_response_status(-1) == -1);
  assert(http_response_status(999999) == -1);
  assert(http_response_body(-1) == NULL);
  assert(http_response_header(-1, (char *)(uintptr_t) "X") == NULL);
  assert(http_response_free(-1) == -1);
  assert(http_response_free(999999) == -1);
  assert(http_get_response(-1, (char *)(uintptr_t) "/", (char *)(uintptr_t) "") <
         0); // invalid client id
}

// In-process loopback exchange: a REAL server thread, a REAL client request,
// and byte-exact status/body/header assertions through the response handle
// accessors. Runs LAST: it exercises the full request path end to end.
static HttpResponse g_live_response;

static HttpResponse *live_handler(char *method, char *path, char *headers,
                                  char *body) {
  (void)headers;
  (void)body;
  assert(strcmp(method, "GET") == 0);
  assert(strcmp(path, "/ping") == 0);
  g_live_response.status = 200;
  g_live_response.headers = (char *)(uintptr_t) "X-Test: yes\r\n";
  g_live_response.contentType = (char *)(uintptr_t) "text/plain";
  g_live_response.streamFd = -1;
  g_live_response.isComplete = true;
  g_live_response.partialBody = (char *)(uintptr_t) "pong";
  return &g_live_response;
}

#define LIVE_PORT 18085
#define LIVE_TRIES 50

// Cross-TU forwarders for the allocation injector: test_alloc.h's arming
// controls are static inline in this TU, so other suite TUs reach them only
// through these out-of-line symbols (same pattern as http_test_server_handler).
void test_alloc_fail_after(long successes) { osp_alloc_fail_after(successes); }
void test_alloc_fail_off(void) { osp_alloc_fail_off(); }

// Allocation rejection must precede handle exhaustion; the following live
// loopback exchange also proves both services recover after rejection.
static void check_http_creation_allocation_failures(void) {
  long live = osp_alloc_live();
  osp_alloc_fail_next();
  assert(http_create_client("http://127.0.0.1:1", 1) == -3);
  assert(osp_alloc_live() == live);
  osp_alloc_fail_next(); // the server record itself
  assert(http_create_server(SERVER_TEST_PORT,
                            (char *)(uintptr_t) "127.0.0.1") == -3);
  assert(osp_alloc_live() == live);
  osp_alloc_fail_after(1); // the record succeeds; its address copy does not
  assert(http_create_server(SERVER_TEST_PORT,
                            (char *)(uintptr_t) "127.0.0.1") == -3);
  assert(osp_alloc_live() == live &&
         "the half-built record must be released, not abandoned");
  osp_alloc_fail_off();
}

void test_live_loopback_exchange(void) {
  check_http_creation_allocation_failures();
  printf("Testing live loopback HTTP exchange...\n");
  int64_t server_id = http_create_server(LIVE_PORT, (char *)(uintptr_t) "127.0.0.1");
  assert(server_id > 0);
  assert(http_listen(server_id, live_handler) == 0);
  int64_t client_id =
      http_create_client((char *)(uintptr_t) "http://127.0.0.1:18085", 5000);
  assert(client_id > 0);
  int64_t handle = -1;
  for (int i = 0; i < LIVE_TRIES && handle < 0; i++) {
    handle = http_get_response(client_id, (char *)(uintptr_t) "/ping",
                               (char *)(uintptr_t) "");
    if (handle < 0) {
      usleep(100000); // listener thread may still be binding
    }
  }
  assert(handle >= 0);
  assert(http_response_status(handle) == 200);
  char *body = http_response_body(handle);
  assert(body != NULL && strcmp(body, "pong") == 0);
  free(body);
  char *header = http_response_header(handle, (char *)(uintptr_t) "X-Test");
  assert(header != NULL && strcmp(header, "yes") == 0);
  free(header);
  char *missing = http_response_header(handle, (char *)(uintptr_t) "X-Absent");
  assert(missing == NULL);
  assert(http_response_free(handle) == 0);
  assert(http_response_free(handle) == -1); // double free rejected
  assert(http_response_body(handle) == NULL);
  http_close_client(client_id);
  http_stop_server(server_id);
  printf("✅ Live loopback exchange passed!\n\n");
}

// http_post_response is the POST twin of http_get_response: the request body
// reaches the handler and the response body/headers are retained behind the
// returned handle [HTTP-RESPONSE-HANDLE].
static char g_post_seen_body[MAX_HTTP_BUFFER];
static char g_post_seen_method[8];

static HttpResponse *post_handler(char *method, char *path, char *headers,
                                  char *body) {
  (void)path;
  (void)headers;
  snprintf(g_post_seen_method, sizeof(g_post_seen_method), "%s", method);
  snprintf(g_post_seen_body, sizeof(g_post_seen_body), "%s", body ? body : "");
  g_live_response.status = 201;
  g_live_response.headers =
      (char *)(uintptr_t) "X-Echo: posted\r\n";
  g_live_response.contentType = (char *)(uintptr_t) "text/plain";
  g_live_response.streamFd = -1;
  g_live_response.isComplete = true;
  g_live_response.partialBody = (char *)(uintptr_t) "created";
  return &g_live_response;
}

#define POST_PORT 18091

void test_post_response_loopback(void) {
  printf("Testing http_post_response loopback...\n");
  int64_t server_id =
      http_create_server(POST_PORT, (char *)(uintptr_t) "127.0.0.1");
  assert(server_id > 0);
  assert(http_listen(server_id, post_handler) == 0);
  int64_t client_id =
      http_create_client((char *)(uintptr_t) "http://127.0.0.1:18091", 5000);
  assert(client_id > 0);
  int64_t handle = -1;
  for (int i = 0; i < LIVE_TRIES && handle < 0; i++) {
    handle = http_post_response(client_id, (char *)(uintptr_t) "/submit",
                                (char *)(uintptr_t) "payload-42",
                                (char *)(uintptr_t) "");
    if (handle < 0) {
      usleep(100000);
    }
  }
  assert(handle >= 1);
  assert(strcmp(g_post_seen_method, "POST") == 0);
  assert(strcmp(g_post_seen_body, "payload-42") == 0);
  assert(http_response_status(handle) == 201);
  char *body = http_response_body(handle);
  assert(body != NULL && strcmp(body, "created") == 0);
  free(body);
  char *header = http_response_header(handle, (char *)(uintptr_t) "X-Echo");
  assert(header != NULL && strcmp(header, "posted") == 0);
  free(header);
  assert(http_response_free(handle) == 0);
  http_close_client(client_id);
  http_stop_server(server_id);
  printf("✅ http_post_response loopback passed!\n\n");
}

void test_client_handle_bounds(void) {
  assert(http_get(-1, (char *)(uintptr_t) "/x", (char *)(uintptr_t) "") == -1);
  assert(http_get(0, (char *)(uintptr_t) "/x", (char *)(uintptr_t) "") == -1);
  assert(http_get(MAX_CLIENTS, (char *)(uintptr_t) "/x",
                  (char *)(uintptr_t) "") == -1);
  assert(http_get(MAX_CLIENTS + 4096, (char *)(uintptr_t) "/x",
                  (char *)(uintptr_t) "") == -1);
  assert(http_close_client(-1) == -1);
  assert(http_close_client(MAX_CLIENTS + 4096) == -1);
  assert(http_get(MAX_CLIENTS - 1, (char *)(uintptr_t) "/x",
                  (char *)(uintptr_t) "") == -1); // in range, empty slot
  assert(http_close_client(MAX_CLIENTS - 1) == 0); // closing one is a no-op
}

// get_next_id() is a process-global counter shared with servers and sockets.
// Once it passes MAX_CLIENTS the table cannot hold the handle, and creation
// must SAY SO instead of writing past the end of clients[]. Runs LAST: it
// burns the shared id space that every other handle type draws from.
void test_client_handle_exhaustion(void) {
  int64_t last = 0;
  for (int i = 0; i < MAX_CLIENTS + 8; i++) {
    last = http_create_client((char *)(uintptr_t) "http://127.0.0.1:1", 1);
    if (last < 0) {
      break;
    }
    assert(http_close_client(last) == 0);
  }
  assert(last == -5); // handle table exhausted, nothing written out of bounds
  assert(http_create_client((char *)(uintptr_t) "http://127.0.0.1:1", 1) == -5);
}

// The server table is finite and its exhaustion is REPORTED. The id counter is
// shared with clients, so by this point it is already past the table -- and a
// create that answered with an out-of-range id would write servers[id] outside
// the array, which is exactly the defect the client handles carried.
void test_server_handle_exhaustion(void) {
  assert(http_create_server(SERVER_TEST_PORT,
                            (char *)(uintptr_t) "127.0.0.1") == -3);
  assert(http_create_server(1, (char *)(uintptr_t) "127.0.0.1") == -3);
}

void run_all_http_tests(void) {
  printf("🧪 Starting HTTP Runtime Test Suite\n");
  printf("=====================================\n\n");

  test_http_create_client();
  test_http_create_server();
  test_http_server_lifecycle();
  test_rejection_mapping();
  test_sanitize_log_token();
  test_socket_interrupted();
  test_response_handle_rejection();
  test_client_chunked_response();
  test_client_status_line_handling();
  test_client_methods_and_body();
  test_client_transport_failures();
  test_client_large_response();
  test_client_malformed_chunks();
  test_client_response_allocation_failures();
  test_client_tls_handshake_failure();
  test_client_handle_bounds();
  test_server_rejects_bad_requests();
  test_server_self_stop_from_handler();
  test_server_handle_bounds();
  test_live_loopback_exchange();    // full end-to-end request path
  test_post_response_loopback();    // POST body retained behind a handle
  test_client_tls_peer_verification(); // untrusted/wrong-host certs rejected
  test_server_labels_an_unparseable_request();
  // The three below burn finite id spaces and are permanent for the process.
  test_listener_spawn_failure_is_reported(); // burns the FIBER id space
  test_client_handle_exhaustion();           // burns the shared HTTP id space
  test_server_handle_exhaustion();           // observes that same exhaustion

  printf("🎉 All HTTP runtime tests passed!\n");
  printf("=====================================\n");
}

int main(void) {
  run_all_http_tests();
  return 0;
}

