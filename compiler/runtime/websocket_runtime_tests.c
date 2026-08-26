// Assertion-driven tests for the WebSocket transport
// (docs/specs/0015-WebSockets.md). A failed assert aborts the binary.
//
// This suite OWNS websocket_client_runtime.c and websocket_server_runtime.c.
// Everything runs against a REAL loopback server in this process — a real
// accept loop, a real upgrade handshake, real frames on a real socket — so the
// transport is asserted end to end rather than one validation branch at a time.
// Spec anchors:
//   [BUILTIN-WEBSOCKET-CLIENT]    client handles, ws:// only, wss:// rejected
//   [BUILTIN-WEBSOCKET-SERVER]    upgrade, welcome frame, echo, broadcast
//   [BUILTIN-WEBSOCKET-LIFECYCLE] keep-alive blocks until SIGINT/SIGTERM
#include "http_shared.h"
#include <assert.h>
#include <signal.h>

int64_t websocket_connect(char *url);
int64_t websocket_send(int64_t ws_id, char *message);
int64_t websocket_close(int64_t ws_id);
int64_t websocket_create_server(int64_t port, char *address, char *path);
int64_t websocket_server_listen(int64_t server_id);
int64_t websocket_server_broadcast(int64_t server_id, char *message);
int64_t websocket_stop_server(int64_t server_id);
void websocket_keep_alive(void);

enum {
  WS_LIVE_PORT = 18086,
  WS_DEAD_PORT = 18087, // accepts, then closes without a handshake response
  WS_TRIES = 100,
  WS_POLL_US = 20000,
  WS_IO_TIMEOUT_S = 5
};

static const char *const WS_SAMPLE_KEY = "dGhlIHNhbXBsZSBub25jZQ==";
static const char *const WS_SAMPLE_ACCEPT = "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=";

// ---------------------------------------------------------------- raw client

// Loopback TCP connect with a bounded retry: the server's accept thread may
// still be binding when the first attempt runs.
static int ws_raw_connect(int port) {
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_port = htons((uint16_t)port);
  addr.sin_addr.s_addr = inet_addr("127.0.0.1");
  for (int i = 0; i < WS_TRIES; i++) {
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    assert(fd >= 0);
    if (connect(fd, (struct sockaddr *)&addr, sizeof(addr)) == 0) {
      struct timeval tv = {WS_IO_TIMEOUT_S, 0};
      assert(setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv)) == 0);
      return fd;
    }
    close(fd);
    usleep(WS_POLL_US);
  }
  assert(0 && "loopback WebSocket server never accepted");
  return -1;
}

static void ws_send_all(int fd, const void *buf, size_t len) {
  size_t sent = 0;
  while (sent < len) {
    ssize_t n = send(fd, (const char *)buf + sent, len - sent, 0);
    assert(n > 0);
    sent += (size_t)n;
  }
}

static bool ws_read_exact(int fd, void *buf, size_t len) {
  size_t got = 0;
  while (got < len) {
    ssize_t n = recv(fd, (char *)buf + got, len - got, 0);
    if (n <= 0) {
      return false;
    }
    got += (size_t)n;
  }
  return true;
}

// Reads the upgrade response one byte at a time so the welcome frame that
// follows it stays in the socket buffer intact.
static void ws_read_headers(int fd, char *out, size_t capacity) {
  size_t len = 0;
  while (len + 1 < capacity) {
    char c = 0;
    ssize_t n = recv(fd, &c, 1, 0);
    assert(n == 1);
    out[len++] = c;
    if (len >= 4 && memcmp(out + len - 4, "\r\n\r\n", 4) == 0) {
      break;
    }
  }
  out[len] = '\0';
}

// Reads exactly one server text frame off the wire and returns its payload.
// Length is decoded from the header before the body is read, so a frame split
// across TCP segments is reassembled instead of truncated.
static char *ws_read_text_frame(int fd) {
  unsigned char head[4] = {0};
  assert(ws_read_exact(fd, head, 2));
  size_t header_len = 2;
  size_t payload_len = head[1] & 0x7F;
  assert((head[1] & 0x80) == 0); // servers never mask
  if (payload_len == 126) {
    assert(ws_read_exact(fd, head + 2, 2));
    payload_len = ((size_t)head[2] << 8) | (size_t)head[3];
    header_len = 4;
  }
  char *frame = malloc(header_len + payload_len);
  assert(frame != NULL);
  memcpy(frame, head, header_len);
  assert(payload_len == 0 || ws_read_exact(fd, frame + header_len, payload_len));
  char *payload = NULL;
  assert(parse_websocket_frame(frame, header_len + payload_len, &payload) ==
         (int)payload_len);
  free(frame);
  assert(payload != NULL);
  return payload;
}

// Client frames MUST be masked (RFC 6455 §5.3); the server unmasks them.
static void ws_send_masked_text(int fd, const char *text) {
  size_t len = strlen(text);
  assert(len < 126);
  unsigned char frame[132];
  const unsigned char key[4] = {0x12, 0x34, 0x56, 0x78};
  frame[0] = 0x81;
  frame[1] = (unsigned char)(0x80 | len);
  memcpy(frame + 2, key, sizeof(key));
  for (size_t i = 0; i < len; i++) {
    frame[6 + i] = (unsigned char)text[i] ^ key[i % 4];
  }
  ws_send_all(fd, frame, 6 + len);
}

static void ws_send_upgrade(int fd, const char *key_header) {
  char request[512];
  int n = snprintf(request, sizeof(request),
                   "GET /chat HTTP/1.1\r\n"
                   "Host: 127.0.0.1\r\n"
                   "Upgrade: websocket\r\n"
                   "Connection: Upgrade\r\n"
                   "%s"
                   "Sec-WebSocket-Version: 13\r\n"
                   "\r\n",
                   key_header);
  assert(n > 0 && (size_t)n < sizeof(request));
  ws_send_all(fd, request, (size_t)n);
}

// The peer closed: recv drains anything already buffered and then returns 0.
static void ws_expect_peer_close(int fd) {
  char scratch[256];
  for (int i = 0; i < WS_TRIES; i++) {
    ssize_t n = recv(fd, scratch, sizeof(scratch), 0);
    if (n == 0) {
      return;
    }
    assert(n > 0); // a timeout (-1) means the server never closed
  }
  assert(0 && "server kept a non-WebSocket connection open");
}

// ------------------------------------------------------------- handle checks

// Handle validation on every entry point, before any socket exists
// [BUILTIN-WEBSOCKET-CLIENT] [BUILTIN-WEBSOCKET-SERVER].
static void test_handle_validation(void) {
  assert(websocket_connect(NULL) == -1);
  assert(websocket_connect("wss://example.com/chat") == -2); // TLS unsupported
  assert(websocket_connect("http://example.com/chat") == -2);
  assert(websocket_connect("ws://") == -3);        // parse_url rejects
  assert(websocket_connect("ws://h:0/") == -3);    // unusable port
  assert(websocket_send(-1, "message") == -2);
  assert(websocket_send(MAX_WEBSOCKETS, "message") == -2);
  assert(websocket_send(1, NULL) == -1);
  assert(websocket_send(MAX_WEBSOCKETS - 1, "unregistered") == -2);
  assert(websocket_close(-1) == -1);
  assert(websocket_close(MAX_WEBSOCKETS) == -1);
  assert(websocket_close(MAX_WEBSOCKETS - 1) == 0); // empty slot is a no-op

  int64_t server_id = websocket_create_server(8083, "127.0.0.1", "/chat");
  assert(server_id > 0);
  assert(websocket_create_server(0, "127.0.0.1", "/chat") == -1);
  assert(websocket_create_server(65536, "127.0.0.1", "/chat") == -1);
  assert(websocket_create_server(8084, NULL, "/chat") == -2);
  assert(websocket_create_server(8084, "127.0.0.1", NULL) == -2);
  assert(websocket_server_listen(-1) == -1);
  assert(websocket_server_listen(MAX_WEBSOCKET_SERVERS) == -1);
  assert(websocket_server_listen(MAX_WEBSOCKET_SERVERS - 1) == -1); // no server
  assert(websocket_server_broadcast(server_id, NULL) == -1);
  assert(websocket_server_broadcast(-1, "message") == -2);
  assert(websocket_server_broadcast(MAX_WEBSOCKET_SERVERS - 1, "m") == -2);
  assert(websocket_server_broadcast(server_id, "nobody-listening") == 0);
  assert(websocket_stop_server(-1) == -1);
  assert(websocket_stop_server(server_id) == 0);
  assert(websocket_stop_server(server_id) == 0); // already gone: still a no-op
}

// A port that is bound twice cannot listen twice.
static void test_listen_bind_conflict(void) {
  int64_t first = websocket_create_server(WS_LIVE_PORT, "127.0.0.1", "/chat");
  int64_t second = websocket_create_server(WS_LIVE_PORT, "127.0.0.1", "/chat");
  assert(first > 0 && second > 0);
  assert(websocket_server_listen(first) == 0);
  assert(websocket_server_listen(second) == -4); // bind() rejects the reuse
  assert(websocket_stop_server(second) == 0);
  assert(websocket_stop_server(first) == 0);
  // An address the host cannot bind fails the same way.
  int64_t unroutable = websocket_create_server(WS_LIVE_PORT, "203.0.113.1", "/");
  assert(unroutable > 0);
  assert(websocket_server_listen(unroutable) == -4);
  assert(websocket_stop_server(unroutable) == 0);
}

// ------------------------------------------------------------ live transport

// The full server path: upgrade, welcome frame, echo, broadcast, and teardown
// with a live connection still registered [BUILTIN-WEBSOCKET-SERVER].
static void test_live_loopback_exchange(void) {
  int64_t server_id = websocket_create_server(WS_LIVE_PORT, "127.0.0.1", "/chat");
  assert(server_id > 0);
  assert(websocket_server_listen(server_id) == 0);

  int fd = ws_raw_connect(WS_LIVE_PORT);
  char key_header[64];
  snprintf(key_header, sizeof(key_header), "Sec-WebSocket-Key: %s\r\n",
           WS_SAMPLE_KEY);
  ws_send_upgrade(fd, key_header);

  char headers[1024];
  ws_read_headers(fd, headers, sizeof(headers));
  assert(strstr(headers, "HTTP/1.1 101 Switching Protocols\r\n") == headers);
  assert(strstr(headers, "Upgrade: websocket") != NULL);
  assert(strstr(headers, WS_SAMPLE_ACCEPT) != NULL);

  char *welcome = ws_read_text_frame(fd);
  assert(strstr(welcome, "\"type\":\"welcome\"") != NULL);
  assert(strstr(welcome, "Osprey WebSocket Server") != NULL);
  free(welcome);

  ws_send_masked_text(fd, "hello");
  char *echo = ws_read_text_frame(fd);
  assert(strstr(echo, "\"type\":\"echo\"") != NULL);
  assert(strstr(echo, "\"original\":\"hello\"") != NULL);
  assert(strstr(echo, "Server received: hello") != NULL);
  free(echo);

  // Broadcast reaches exactly the registered connection and arrives verbatim.
  assert(websocket_server_broadcast(server_id, "broadcast-one") == 1);
  char *broadcast = ws_read_text_frame(fd);
  assert(strcmp(broadcast, "broadcast-one") == 0);
  free(broadcast);

  // The language-level client transport against the same server
  // [BUILTIN-WEBSOCKET-CLIENT]. The surface exposes no receive hook, so its
  // welcome frame is simply left in the socket buffer.
  char url[64];
  snprintf(url, sizeof(url), "ws://127.0.0.1:%d/chat", WS_LIVE_PORT);
  int64_t ws_id = websocket_connect(url);
  assert(ws_id > 0);
  assert(websocket_send(ws_id, "from-client") == 0);

  int64_t reached = 0;
  for (int i = 0; i < WS_TRIES && reached != 2; i++) {
    reached = websocket_server_broadcast(server_id, "broadcast-two");
    if (reached != 2) {
      usleep(WS_POLL_US); // the handler thread may still be registering
    }
  }
  assert(reached == 2);
  assert(websocket_close(ws_id) == 0);
  assert(websocket_send(ws_id, "after-close") == -2);

  // Stopping the server tears the surviving connection down and closes the fd.
  assert(websocket_stop_server(server_id) == 0);
  close(fd);
}

// Connections the server must refuse: one that says nothing, one that is not
// an upgrade, and one that upgrades without a key. Each is closed, not served.
static void test_non_websocket_connections(void) {
  int64_t server_id = websocket_create_server(WS_LIVE_PORT, "127.0.0.1", "/chat");
  assert(server_id > 0);
  assert(websocket_server_listen(server_id) == 0);

  int silent = ws_raw_connect(WS_LIVE_PORT);
  close(silent); // no bytes at all: the handler sees recv() <= 0

  int plain = ws_raw_connect(WS_LIVE_PORT);
  ws_send_all(plain, "GET /chat HTTP/1.1\r\nHost: x\r\n\r\n", 30);
  ws_expect_peer_close(plain);
  close(plain);

  int keyless = ws_raw_connect(WS_LIVE_PORT);
  ws_send_upgrade(keyless, ""); // upgrade headers, no Sec-WebSocket-Key
  ws_expect_peer_close(keyless);
  close(keyless);

  // The server survives all three and still serves a real client.
  int good = ws_raw_connect(WS_LIVE_PORT);
  char key_header[64];
  snprintf(key_header, sizeof(key_header), "Sec-WebSocket-Key: %s\r\n",
           WS_SAMPLE_KEY);
  ws_send_upgrade(good, key_header);
  char headers[1024];
  ws_read_headers(good, headers, sizeof(headers));
  assert(strstr(headers, WS_SAMPLE_ACCEPT) != NULL);
  char *welcome = ws_read_text_frame(good);
  assert(strstr(welcome, "welcome") != NULL);
  free(welcome);
  close(good);
  assert(websocket_stop_server(server_id) == 0);
}

// --------------------------------------------------------- client rejections

// A listener that accepts and immediately closes: the client's handshake read
// fails, which is the -8 contract rather than a hung connect.
static void *ws_rude_listener(void *arg) {
  int listen_fd = *(int *)arg;
  int client = accept(listen_fd, NULL, NULL);
  if (client >= 0) {
    close(client);
  }
  return NULL;
}

static void test_client_connect_failures(void) {
  // Nothing is listening: connect() is refused on loopback.
  assert(websocket_connect("ws://127.0.0.1:1/chat") == -6);

  // A name no resolver can answer: the label is past the DNS length limit.
  char long_host[400];
  memcpy(long_host, "ws://", 5);
  memset(long_host + 5, 'a', sizeof(long_host) - 8);
  memcpy(long_host + sizeof(long_host) - 3, "/x", 3);
  assert(websocket_connect(long_host) == -5);

  int listen_fd = socket(AF_INET, SOCK_STREAM, 0);
  assert(listen_fd >= 0);
  int opt = 1;
  assert(setsockopt(listen_fd, SOL_SOCKET, SO_REUSEADDR, (const char *)&opt,
                    sizeof(opt)) == 0);
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_port = htons(WS_DEAD_PORT);
  addr.sin_addr.s_addr = inet_addr("127.0.0.1");
  assert(bind(listen_fd, (struct sockaddr *)&addr, sizeof(addr)) == 0);
  assert(listen(listen_fd, 1) == 0);
  pthread_t thread;
  assert(pthread_create(&thread, NULL, ws_rude_listener, &listen_fd) == 0);
  char url[64];
  snprintf(url, sizeof(url), "ws://127.0.0.1:%d/chat", WS_DEAD_PORT);
  assert(websocket_connect(url) == -8); // handshake response never arrives
  assert(pthread_join(thread, NULL) == 0);
  close(listen_fd);
}

// ------------------------------------------------------------- keep-alive

static void *ws_shutdown_signaller(void *arg) {
  (void)arg;
  usleep(300000);
  assert(kill(getpid(), SIGTERM) == 0);
  return NULL;
}

// websocketKeepAlive blocks until SIGINT or SIGTERM, then stops every server
// [BUILTIN-WEBSOCKET-LIFECYCLE]. Runs LAST: it latches the shutdown flag.
static void test_keep_alive_until_signal(void) {
  int64_t server_id = websocket_create_server(WS_LIVE_PORT, "127.0.0.1", "/chat");
  assert(server_id > 0);
  assert(websocket_server_listen(server_id) == 0);
  pthread_t thread;
  assert(pthread_create(&thread, NULL, ws_shutdown_signaller, NULL) == 0);
  websocket_keep_alive();
  assert(pthread_join(thread, NULL) == 0);
  // keep_alive stopped every server, so the handle is already released.
  assert(websocket_stop_server(server_id) == 0);
  // ...and the port is free again.
  int64_t reborn = websocket_create_server(WS_LIVE_PORT, "127.0.0.1", "/chat");
  assert(reborn > 0);
  assert(websocket_server_listen(reborn) == 0);
  assert(websocket_stop_server(reborn) == 0);
}

int main(void) {
  signal(SIGPIPE, SIG_IGN); // a closed peer must surface as a send error
  test_handle_validation();
  test_listen_bind_conflict();
  test_live_loopback_exchange();
  test_non_websocket_connections();
  test_client_connect_failures();
  test_keep_alive_until_signal(); // LAST: latches the shutdown flag
  printf("✅ WebSocket runtime tests passed\n");
  return 0;
}
