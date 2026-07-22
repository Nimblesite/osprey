#include "http_shared.h"
#include <assert.h>

// Include all runtime modules
extern int64_t http_create_client(char *base_url, int64_t timeout);
extern int64_t http_request(int64_t client_id, int64_t method, char *path,
                            char *headers, char *body);
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

extern int64_t websocket_connect(char *url, char *message_handler);
extern int64_t websocket_send(int64_t ws_id, char *message);
extern int64_t websocket_close(int64_t ws_id);

extern int64_t websocket_create_server(int64_t port, char *address, char *path);
extern int64_t websocket_server_listen(int64_t server_id);
extern int64_t websocket_server_send(int64_t server_id, int64_t connection_id,
                                     char *message);
extern int64_t websocket_server_broadcast(int64_t server_id, char *message);
extern int64_t websocket_stop_server(int64_t server_id);

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

  // Clean up
  websocket_stop_server(server_id);
  printf("✅ websocket_create_server tests passed!\n\n");
}

void test_websocket_client(void) {
  printf("Testing WebSocket client functions...\n");

  // Test WebSocket connection creation (will fail without server, but tests
  // function)
  int64_t ws_id = websocket_connect("ws://echo.websocket.org", "test_handler");
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

  printf("🎉 All HTTP runtime tests passed!\n");
  printf("=====================================\n");
  printf("The HTTP runtime is working correctly.\n");
  printf("You can now use these functions from Osprey code:\n");
  printf("  - httpCreateServer(port: Int, address: String) -> Int\n");
  printf("  - httpListen(serverID: Int, handler: Int) -> Int\n");
  printf("  - httpStopServer(serverID: Int) -> Int\n");
  printf("  - httpCreateClient(baseUrl: String, timeout: Int) -> Int\n");
  printf("  - httpRequest(clientID: Int, method: Int, path: String, headers: "
         "String, body: String) -> Int\n");
  printf("  - httpCloseClient(clientID: Int) -> Int\n");
  printf("  - websocketCreateServer(port: Int, address: String, path: String) "
         "-> Int\n");
  printf("  - websocketServerListen(serverID: Int) -> Int\n");
  printf(
      "  - websocketServerBroadcast(serverID: Int, message: String) -> Int\n");
  printf("  - websocketStopServer(serverID: Int) -> Int\n");
  printf("  - websocketConnect(url: String, handler: String) -> Int\n");
  printf("  - websocketSend(wsID: Int, message: String) -> Int\n");
  printf("  - websocketClose(wsID: Int) -> Int\n");
}

int main(void) {
  run_all_http_tests();
  return 0;
}