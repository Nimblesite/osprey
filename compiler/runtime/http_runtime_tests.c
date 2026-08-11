#include "http_server_internal.h"
#include "http_shared.h"
#include <assert.h>

// Include all runtime modules
extern int64_t http_create_client(char *base_url, int64_t timeout);
extern int64_t http_close_client(int64_t client_id);
extern int64_t http_get(int64_t client_id, char *path, char *headers);
extern int64_t http_post(int64_t client_id, char *path, char *body,
                         char *headers);
extern int64_t http_put(int64_t client_id, char *path, char *body,
                        char *headers);
extern int64_t http_delete(int64_t client_id, char *path, char *headers);

extern int64_t http_create_server(int64_t port, char *address);
extern int64_t http_listen(int64_t server_id, int64_t handler);
extern int64_t http_stop_server(int64_t server_id);

extern int64_t websocket_connect(char *url);
extern int64_t websocket_send(int64_t ws_id, char *message);
extern int64_t websocket_close(int64_t ws_id);

extern int64_t websocket_create_server(int64_t port, char *address, char *path);
extern int64_t websocket_server_listen(int64_t server_id);
extern int64_t websocket_server_broadcast(int64_t server_id, char *message);
extern int64_t websocket_stop_server(int64_t server_id);

extern int64_t http_get_response(int64_t client_id, char *path, char *headers);
extern int64_t http_response_status(int64_t handle);
extern char *http_response_body(int64_t handle);
extern char *http_response_header(int64_t handle, char *name);
extern int64_t http_response_free(int64_t handle);

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
  int64_t listen_result = http_listen(server_id, 1);
  assert(listen_result == 0);
  printf("✅ Server listening started\n");

  // Stop server
  int64_t stop_result = http_stop_server(server_id);
  assert(stop_result == 0);
  printf("✅ Server stopped successfully\n");

  printf("✅ HTTP server lifecycle tests passed!\n\n");
}

void test_url_parsing(void) {
  printf("Testing URL parsing...\n");

  char *host;
  int port;
  char *path;

  // Test basic URL
  int result =
      parse_url("http://example.com:8080/api/test", &host, &port, &path);
  assert(result == 0);
  assert(strcmp(host, "example.com") == 0);
  assert(port == 8080);
  assert(strcmp(path, "/api/test") == 0);
  printf("✅ Parsed: http://example.com:8080/api/test\n");
  free(host);
  free(path);

  // Test URL without port
  result = parse_url("http://localhost/test", &host, &port, &path);
  assert(result == 0);
  assert(strcmp(host, "localhost") == 0);
  assert(port == 80);
  assert(strcmp(path, "/test") == 0);
  printf("✅ Parsed: http://localhost/test (default port 80)\n");
  free(host);
  free(path);

  // Test URL without path
  result = parse_url("http://api.example.com:3000", &host, &port, &path);
  assert(result == 0);
  assert(strcmp(host, "api.example.com") == 0);
  assert(port == 3000);
  assert(strcmp(path, "/") == 0);
  printf("✅ Parsed: http://api.example.com:3000 (default path /)\n");
  free(host);
  free(path);

  printf("✅ URL parsing tests passed!\n\n");
}

void test_http_method_strings(void) {
  printf("Testing HTTP method strings...\n");

  assert(strcmp(http_method_to_string(HTTP_GET), "GET") == 0);
  assert(strcmp(http_method_to_string(HTTP_POST), "POST") == 0);
  assert(strcmp(http_method_to_string(HTTP_PUT), "PUT") == 0);
  assert(strcmp(http_method_to_string(HTTP_DELETE), "DELETE") == 0);

  printf("✅ HTTP method strings correct\n");
  printf("✅ HTTP method tests passed!\n\n");
}

void test_http_client_request_mock(void) {
  printf("Testing HTTP client request (mock)...\n");

  // Create client
  int64_t client_id = http_create_client("http://httpbin.org", 10000);
  assert(client_id > 0);
  printf("✅ Created client for httpbin.org: %" PRId64 "\n", client_id);

  // Note: This would actually try to make a real HTTP request
  // For testing in isolation, we'll just verify the client was created
  // In a full test, you could make a request to httpbin.org/get

  // Clean up
  http_close_client(client_id);
  printf("✅ HTTP client request test passed (mock)!\n\n");
}

void test_websocket_create_server(void) {
  // [BUILTIN-WEBSOCKET] Runtime handle creation and validation.
  printf("Testing websocket_create_server...\n");

  // Test valid WebSocket server creation
  int64_t server_id = websocket_create_server(8083, "127.0.0.1", "/chat");
  assert(server_id > 0);
  printf("✅ Created WebSocket server with ID: %" PRId64 "\n", server_id);

  // Test invalid port
  int64_t invalid_server = websocket_create_server(0, "127.0.0.1", "/chat");
  assert(invalid_server < 0);
  printf("✅ Correctly rejected invalid port\n");

  // Test invalid address
  int64_t addr_server = websocket_create_server(8084, NULL, "/chat");
  assert(addr_server < 0);
  printf("✅ Correctly rejected NULL address\n");

  assert(websocket_server_listen(-1) < 0);
  assert(websocket_server_broadcast(-1, "message") < 0);
  assert(websocket_stop_server(-1) < 0);

  // Clean up
  websocket_stop_server(server_id);
  printf("✅ websocket_create_server tests passed!\n\n");
}

void test_websocket_client(void) {
  printf("Testing WebSocket client functions...\n");

  assert(websocket_connect(NULL) == -1);
  assert(websocket_connect("wss://example.com/chat") == -2);
  assert(websocket_send(-1, "message") < 0);
  assert(websocket_close(-1) < 0);

  // Test WebSocket connection creation (will fail without server, but tests
  // function)
  int64_t ws_id = websocket_connect("ws://echo.websocket.org");
  if (ws_id > 0) {
    printf("✅ WebSocket connection created with ID: %" PRId64 "\n", ws_id);

    // Test send (will likely fail without real connection)
    int64_t send_result = websocket_send(ws_id, "test message");
    printf("📤 WebSocket send result: %" PRId64 "\n", send_result);

    // Clean up
    websocket_close(ws_id);
    printf("✅ WebSocket connection closed\n");
  } else {
    printf("⚠️  WebSocket connection failed (expected without server): "
           "%" PRId64 "\n",
           ws_id);
  }

  printf("✅ WebSocket client tests completed!\n\n");
}

// base64_encode against the RFC 4648 vectors, plus non-ASCII bytes.
void test_base64_vectors(void) {
  const char *vec[][2] = {{"", ""},         {"f", "Zg=="},
                          {"fo", "Zm8="},   {"foo", "Zm9v"},
                          {"foob", "Zm9vYg=="}, {"fooba", "Zm9vYmE="},
                          {"foobar", "Zm9vYmFy"}};
  for (size_t i = 0; i < sizeof(vec) / sizeof(vec[0]); i++) {
    char *out = base64_encode((const unsigned char *)vec[i][0],
                              strlen(vec[i][0]));
    assert(out != NULL);
    assert(strcmp(out, vec[i][1]) == 0);
    free(out);
  }
  const unsigned char raw[] = {0x00, 0x01, 0xFE, 0xFF};
  char *bin = base64_encode(raw, sizeof(raw));
  assert(bin != NULL && strcmp(bin, "AAH+/w==") == 0);
  free(bin);
}

// A websocket key is base64 of 16 random bytes: exactly 24 chars, '=='-padded,
// and two draws differ.
void test_websocket_key(void) {
  char *k1 = generate_websocket_key();
  char *k2 = generate_websocket_key();
  assert(k1 != NULL && k2 != NULL);
  assert(strlen(k1) == 24 && strlen(k2) == 24);
  assert(strcmp(k1 + 22, "==") == 0);
  assert(strcmp(k1, k2) != 0);
  free(k1);
  free(k2);
}

// The RFC 6455 §1.3 sample handshake: the accept token for the sample key is
// pinned in the spec itself — the strongest possible golden for the SHA-1 +
// base64 accept pipeline.
void test_handshake_response_rfc6455(void) {
  char *resp = create_websocket_handshake_response("dGhlIHNhbXBsZSBub25jZQ==");
  assert(resp != NULL);
  assert(strstr(resp, "HTTP/1.1 101 Switching Protocols\r\n") == resp);
  assert(strstr(resp, "Upgrade: websocket") != NULL);
  assert(strstr(resp, "Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=") !=
         NULL);
  free(resp);
}

// Frame parsing: unmasked, client-masked (XOR unmasking), the 126 extended
// length form, the empty frame, and every truncation rejection.
void test_parse_websocket_frame(void) {
  char *payload = NULL;
  const unsigned char plain[] = {0x81, 5, 'h', 'e', 'l', 'l', 'o'};
  assert(parse_websocket_frame((const char *)plain, sizeof(plain), &payload) ==
         5);
  assert(payload != NULL && strcmp(payload, "hello") == 0);
  free(payload);
  unsigned char masked[] = {0x81, 0x82, 1, 2, 3, 4, 'a' ^ 1, 'b' ^ 2};
  payload = NULL;
  assert(parse_websocket_frame((const char *)masked, sizeof(masked),
                               &payload) == 2);
  assert(payload != NULL && strcmp(payload, "ab") == 0);
  free(payload);
  unsigned char ext[204] = {0x81, 126, 0, 200};
  memset(ext + 4, 'x', 200);
  payload = NULL;
  assert(parse_websocket_frame((const char *)ext, sizeof(ext), &payload) == 200);
  assert(payload != NULL && strlen(payload) == 200 && payload[0] == 'x');
  free(payload);
  const unsigned char empty[] = {0x81, 0};
  payload = NULL;
  assert(parse_websocket_frame((const char *)empty, sizeof(empty), &payload) ==
         0);
  assert(payload != NULL && payload[0] == '\0');
  free(payload);
  const unsigned char runt[] = {0x81};
  assert(parse_websocket_frame((const char *)runt, 1, &payload) == -1);
  const unsigned char big64[] = {0x81, 127}; // 64-bit lengths unsupported
  assert(parse_websocket_frame((const char *)big64, 2, &payload) == -1);
  const unsigned char trunc_ext[] = {0x81, 126, 0}; // extended length cut off
  assert(parse_websocket_frame((const char *)trunc_ext, 3, &payload) == -1);
  const unsigned char trunc_mask[] = {0x81, 0x81, 1, 2}; // mask key cut off
  assert(parse_websocket_frame((const char *)trunc_mask, 4, &payload) == -1);
  const unsigned char trunc_pay[] = {0x81, 3, 'a'}; // payload cut off
  assert(parse_websocket_frame((const char *)trunc_pay, 3, &payload) == -1);
}

// send_websocket_frame writes the exact server wire bytes (unmasked text
// frame), verified over a socketpair, and round-trips through the parser.
void test_send_frame_wire_format(void) {
  int pair[2];
  assert(socketpair(AF_UNIX, SOCK_STREAM, 0, pair) == 0);
  assert(send_websocket_frame(pair[0], "hi") == 4);
  unsigned char wire[8] = {0};
  assert(read(pair[1], wire, sizeof(wire)) == 4);
  assert(wire[0] == 0x81 && wire[1] == 2 && wire[2] == 'h' && wire[3] == 'i');
  static char big[131];
  memset(big, 'A', 130);
  big[130] = '\0';
  assert(send_websocket_frame(pair[0], big) == 134); // 4-byte extended header
  unsigned char ext[140] = {0};
  ssize_t got = 0;
  while (got < 134) {
    ssize_t n = read(pair[1], ext + got, sizeof(ext) - (size_t)got);
    assert(n > 0);
    got += n;
  }
  assert(ext[0] == 0x81 && ext[1] == 126 && ext[2] == 0 && ext[3] == 130);
  char *payload = NULL;
  assert(parse_websocket_frame((const char *)ext, (size_t)got, &payload) ==
         130);
  assert(payload != NULL && strcmp(payload, big) == 0);
  free(payload);
  assert(send_websocket_frame(pair[0], NULL) == -1);
  static char huge[5001];
  memset(huge, 'B', 5000);
  huge[5000] = '\0';
  assert(send_websocket_frame(pair[0], huge) == -1); // DoS guard at 4096
  close(pair[0]);
  close(pair[1]);
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

void test_live_loopback_exchange(void) {
  printf("Testing live loopback HTTP exchange...\n");
  int64_t server_id = http_create_server(LIVE_PORT, (char *)(uintptr_t) "127.0.0.1");
  assert(server_id > 0);
  assert(http_listen(server_id, (int64_t)(uintptr_t)live_handler) == 0);
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

void run_all_http_tests(void) {
  printf("🧪 Starting HTTP Runtime Test Suite\n");
  printf("=====================================\n\n");

  test_http_create_client();
  test_http_create_server();
  test_http_server_lifecycle();
  test_url_parsing();
  test_http_method_strings();
  test_http_client_request_mock();
  test_websocket_create_server();
  test_websocket_client();
  test_base64_vectors();
  test_websocket_key();
  test_handshake_response_rfc6455();
  test_parse_websocket_frame();
  test_send_frame_wire_format();
  test_rejection_mapping();
  test_sanitize_log_token();
  test_socket_interrupted();
  test_response_handle_rejection();
  test_live_loopback_exchange(); // LAST: full end-to-end request path

  printf("🎉 All HTTP runtime tests passed!\n");
  printf("=====================================\n");
}

int main(void) {
  run_all_http_tests();
  return 0;
}
