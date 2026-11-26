// Assertion-driven tests for the WebSocket transport
// (docs/specs/runtime/0015-WebSockets.md). A failed assert aborts the binary.
//
// This suite OWNS websocket_client_runtime.c and websocket_server_runtime.c.
// Everything runs against a REAL loopback server in this process — a real
// accept loop, a real upgrade handshake, real frames on a real socket — so the
// transport is asserted end to end rather than one validation branch at a time.
// Spec anchors:
//   [BUILTIN-WEBSOCKET-CLIENT]    client handles, ws:// only, wss:// rejected
//   [BUILTIN-WEBSOCKET-SERVER]    upgrade, welcome frame, echo, broadcast
//   [BUILTIN-WEBSOCKET-LIFECYCLE] keep-alive blocks until SIGINT/SIGTERM
#include "test_loopback.h"
#include "http_shared.h"
#include "websocket_test_helpers.h"
#include <assert.h>
#include <signal.h>
#include <sys/wait.h>

int64_t websocket_connect(char *url);
int64_t websocket_send(int64_t ws_id, char *message);
int64_t websocket_close(int64_t ws_id);
void websocket_keep_alive(void);

#define WS_LIVE_PORT test_port(1)
#define WS_DEAD_PORT test_port(2) // accepts, then closes without a handshake response

enum {
  WS_TRIES = 100,
  WS_POLL_US = 20000,
  WS_IO_TIMEOUT_S = 5
};

#define WS_SAMPLE_KEY "dGhlIHNhbXBsZSBub25jZQ=="
#define WS_SAMPLE_ACCEPT "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
#define WS_SAMPLE_KEY_LINE "Sec-WebSocket-Key: " WS_SAMPLE_KEY "\r\n"

// ---------------------------------------------------------------- raw client

// Loopback TCP connect with a bounded retry: the server's accept thread may
// still be binding when the first attempt runs.
int ws_raw_connect(int port) {
  struct sockaddr_in addr = test_loopback_addr(port);
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

void ws_send_all(int fd, const void *buf, size_t len) {
  size_t sent = 0;
  while (sent < len) {
    ssize_t n = send(fd, (const char *)buf + sent, len - sent, 0);
    assert(n > 0);
    sent += (size_t)n;
  }
}

bool ws_read_exact(int fd, void *buf, size_t len) {
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

// Reads the reply head into `out` up to its blank line, a byte at a time so
// what follows it stays in the socket buffer: false when the server closed
// first. A timeout is neither, and fails here.
static bool ws_read_reply(int fd, char *out, size_t capacity) {
  size_t len = 0;
  bool whole = false;
  while (!whole && len + 1 < capacity) {
    char c = 0;
    ssize_t n = recv(fd, &c, 1, 0);
    assert(n == 1 || n == 0 || (n < 0 && errno == ECONNRESET));
    if (n <= 0) {
      break;
    }
    out[len++] = c;
    whole = len >= 4 && memcmp(out + len - 4, "\r\n\r\n", 4) == 0;
  }
  out[len] = '\0';
  return whole;
}

void ws_read_headers(int fd, char *out, size_t capacity) {
  assert(ws_read_reply(fd, out, capacity) && "the server closed without replying");
}

// The payload length a frame header announces, in whichever of RFC 6455's three
// forms it uses, decoded here by hand so the tests never trust the parser they test.
static size_t ws_read_length(int fd, unsigned char second) {
  size_t width = (second & 0x7F) == 126 ? 2 : (second & 0x7F) == 127 ? 8 : 0;
  unsigned char bytes[8] = {0};
  assert(width == 0 || ws_read_exact(fd, bytes, width));
  size_t length = width ? 0 : (size_t)(second & 0x7F);
  for (size_t i = 0; i < width; i++) {
    length = (length << 8) | bytes[i];
  }
  return length;
}

// Reads exactly one server frame off the wire: its opcode, FIN set (servers send
// whole frames), no mask, and its payload NUL-terminated. A frame split across
// TCP segments is reassembled instead of truncated.
char *ws_read_frame(int fd, unsigned char *opcode, size_t *length) {
  unsigned char head[2] = {0};
  assert(ws_read_exact(fd, head, 2));
  assert((head[0] & 0xF0) == 0x80 && (head[1] & 0x80) == 0);
  *opcode = head[0] & 0x0F;
  *length = ws_read_length(fd, head[1]);
  char *payload = malloc(*length + 1);
  assert(payload != NULL);
  assert(*length == 0 || ws_read_exact(fd, payload, *length));
  payload[*length] = '\0';
  return payload;
}

char *ws_read_text_frame(int fd) {
  unsigned char opcode = 0;
  size_t length = 0;
  char *payload = ws_read_frame(fd, &opcode, &length);
  assert(opcode == 0x1);
  return payload;
}

// Client frames MUST be masked (RFC 6455 §5.3); the server unmasks them. A
// text of 126 bytes or more takes the 16-bit extended length. The longest is
// the longest message the server accepts.
#define WS_MASKED_TEXT_MAX WS_MESSAGE_LIMIT
static void ws_send_masked_text(int fd, const char *text) {
  size_t len = strlen(text);
  assert(len <= WS_MASKED_TEXT_MAX);
  unsigned char frame[WS_MASKED_TEXT_MAX + 8];
  const unsigned char key[4] = {0x12, 0x34, 0x56, 0x78};
  size_t at = 2;
  frame[0] = 0x81;
  if (len < 126) {
    frame[1] = (unsigned char)(0x80 | len);
  } else {
    frame[1] = 0x80 | 126;
    frame[at++] = (unsigned char)(len >> 8);
    frame[at++] = (unsigned char)(len & 0xFF);
  }
  memcpy(frame + at, key, sizeof(key));
  at += sizeof(key);
  for (size_t i = 0; i < len; i++) {
    frame[at + i] = (unsigned char)text[i] ^ key[i % 4];
  }
  ws_send_all(fd, frame, at + len);
}

// An upgrade request whose fields are `fields`, each line CRLF-ended, after the
// request line and Host that every test sends the same.
static void ws_send_upgrade_fields(int fd, const char *fields) {
  char request[512];
  int n = snprintf(request, sizeof(request), "GET /chat HTTP/1.1\r\nHost: 127.0.0.1\r\n%s\r\n",
                   fields);
  assert(n > 0 && (size_t)n < sizeof(request));
  ws_send_all(fd, request, (size_t)n);
}

// The canonical handshake with `key_header`'s lines: the RFC 6455 4.2.1 names
// and values as the spec capitalises them, one of the spellings it allows.
void ws_send_upgrade(int fd, const char *key_header) {
  char fields[512];
  int n = snprintf(fields, sizeof(fields),
                   "Upgrade: websocket\r\nConnection: Upgrade\r\n%s"
                   "Sec-WebSocket-Version: 13\r\n",
                   key_header);
  assert(n > 0 && (size_t)n < sizeof(fields));
  ws_send_upgrade_fields(fd, fields);
}

// The peer closed: drained EOF or a TCP reset; a timeout is still a failure.
void ws_expect_peer_close(int fd) {
  char scratch[256];
  for (int i = 0; i < WS_TRIES; i++) {
    ssize_t n = recv(fd, scratch, sizeof(scratch), 0);
    if (n == 0 || (n < 0 && errno == ECONNRESET)) {
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

// Creates a server on WS_LIVE_PORT with `host`/`path`, asserting the handle
// is valid and that listen() returns `expected` (0 serves, -4 is refused).
static int64_t ws_serve(char *host, char *path, int expected) {
  int64_t server_id = websocket_create_server(WS_LIVE_PORT, host, path);
  assert(server_id > 0);
  assert(websocket_server_listen(server_id) == expected);
  return server_id;
}

// A port that is bound twice cannot listen twice.
static void test_listen_bind_conflict(void) {
  int64_t first = ws_serve("127.0.0.1", "/chat", 0);
  int64_t second = ws_serve("127.0.0.1", "/chat", -4); // bind() rejects the reuse
  assert(websocket_stop_server(second) == 0);
  assert(websocket_stop_server(first) == 0);
  // An address the host cannot bind fails the same way.
  int64_t unroutable = ws_serve("203.0.113.1", "/", -4);
  assert(websocket_stop_server(unroutable) == 0);
}

// ------------------------------------------------------------ live transport

// The full server path: upgrade, welcome frame, echo, broadcast, and teardown
// with a live connection still registered [BUILTIN-WEBSOCKET-SERVER].
// The server echoed `original` to this socket: the client's frame was masked, as
// RFC 6455 requires, or the server would have closed it with 1002. The welcome
// and broadcasts may come first, or the client's handshake read may have taken
// the welcome already.
static void ws_expect_echo_on(int fd, const char *original) {
  struct timeval tv = {WS_IO_TIMEOUT_S, 0};
  assert(setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv)) == 0);
  char needle[64];
  snprintf(needle, sizeof(needle), "\"original\":\"%s\"", original);
  for (int frames = 0; frames < 8; frames++) {
    char *text = ws_read_text_frame(fd);
    bool echoed = strstr(text, needle) != NULL;
    free(text);
    if (echoed) {
      return;
    }
  }
  assert(0 && "the server never echoed the client's message");
}

// Connects to the live server, upgrades with the sample key, and asserts the
// response is a valid upgrade acceptance; returns the upgraded socket.
static int ws_upgrade_handshake(void) {
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
  return fd;
}

// Reads a text frame, asserts it holds every non-NULL needle, and frees it.
static void ws_expect_text(int fd, const char *a, const char *b, const char *c) {
  char *text = ws_read_text_frame(fd);
  const char *const needles[] = {a, b, c};
  for (size_t i = 0; i < 3; i++) {
    assert(needles[i] == NULL || strstr(text, needles[i]) != NULL);
  }
  free(text);
}

// #414: the echo is valid JSON whatever the client sent. `escaped` is the text
// as a JSON string body; the reply is exact up to its timestamp.
static void ws_expect_exact_echo(int fd, const char *sent, const char *escaped) {
  ws_send_masked_text(fd, sent);
  char *text = ws_read_text_frame(fd);
  size_t room = 2 * strlen(escaped) + 128;
  char *want = malloc(room);
  assert(want != NULL);
  int n = snprintf(want, room,
                   "{\"type\":\"echo\",\"original\":\"%s\",\"echo\":\"Server received: %s\",\"timestamp\":\"",
                   escaped, escaped);
  assert(n > 0 && (size_t)n < room);
  assert(strncmp(text, want, (size_t)n) == 0);
  size_t length = strlen(text);
  assert(length > (size_t)n + 2 && strcmp(text + length - 2, "\"}") == 0);
  for (size_t i = (size_t)n; i < length - 2; i++) {
    assert(text[i] >= '0' && text[i] <= '9');
  }
  free(want);
  free(text);
}

// The widest escapes: every byte a \u00XX, so the reply is twelve times the
// text and takes the 64-bit extended length. Each is delivered whole, never
// dropped for being bigger than the text that asked for it.
#define WS_ECHO_CONTROL_TEXT 6000
static void test_echo_delivers_escaped_texts(int fd) {
  static char sent[WS_ECHO_CONTROL_TEXT + 1];
  static char escaped[6 * WS_ECHO_CONTROL_TEXT + 1];
  const size_t lengths[] = {1, 10, 100, 1000, WS_ECHO_CONTROL_TEXT};
  for (size_t at = 0; at < sizeof lengths / sizeof lengths[0]; at++) {
    size_t length = lengths[at];
    memset(sent, '\x01', length);
    sent[length] = '\0';
    for (size_t i = 0; i < length; i++) {
      memcpy(escaped + 6 * i, "\\u0001", 6);
    }
    escaped[6 * length] = '\0';
    ws_expect_exact_echo(fd, sent, escaped);
  }
  memset(sent, '"', WS_ECHO_CONTROL_TEXT);
  sent[WS_ECHO_CONTROL_TEXT] = '\0';
  for (size_t i = 0; i < WS_ECHO_CONTROL_TEXT; i++) {
    memcpy(escaped + 2 * i, "\\\"", 2);
  }
  escaped[2 * WS_ECHO_CONTROL_TEXT] = '\0';
  ws_expect_exact_echo(fd, sent, escaped);
}

static void test_echo_escapes_what_it_quotes(int fd) {
  ws_expect_exact_echo(fd, "a\"b", "a\\\"b");
  ws_expect_exact_echo(fd, "\",\"admin\":true,\"x\":\"", "\\\",\\\"admin\\\":true,\\\"x\\\":\\\"");
  ws_expect_exact_echo(fd, "back\\slash", "back\\\\slash");
  ws_expect_exact_echo(fd, "tab\tnew\nline\x01\x1b[31m", "tab\\tnew\\nline\\u0001\\u001b[31m");
  ws_expect_exact_echo(fd, "caf\xc3\xa9 / \xe2\x9c\x93", "caf\xc3\xa9 / \xe2\x9c\x93");
  char longest[WS_MASKED_TEXT_MAX + 1];
  memset(longest, 'x', WS_MASKED_TEXT_MAX);
  longest[WS_MASKED_TEXT_MAX] = '\0';
  ws_expect_exact_echo(fd, longest, longest); // past the old 1 KiB reply buffer and the 4 KiB frame
  test_echo_delivers_escaped_texts(fd);
}

// A text frame that is not UTF-8 cannot be quoted: the server closes rather
// than send a reply that is not JSON.
static void test_echo_refuses_what_it_cannot_quote(void) {
  int fd = ws_upgrade_handshake();
  ws_expect_text(fd, "\"type\":\"welcome\"", NULL, NULL);
  ws_send_masked_text(fd, "bad \xff byte");
  ws_expect_peer_close(fd);
  close(fd);
}

static void test_live_loopback_exchange(void) {
  int64_t server_id = ws_serve("127.0.0.1", "/chat", 0);

  int fd = ws_upgrade_handshake();

  ws_expect_text(fd, "\"type\":\"welcome\"", "Osprey WebSocket Server", NULL);

  ws_send_masked_text(fd, "hello");
  ws_expect_text(fd, "\"type\":\"echo\"", "\"original\":\"hello\"",
                 "Server received: hello");
  test_echo_escapes_what_it_quotes(fd);
  test_echo_refuses_what_it_cannot_quote();

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
  ws_expect_echo_on(websockets[ws_id]->socket_fd, "from-client");
  assert(websocket_close(ws_id) == 0);
  assert(websocket_send(ws_id, "after-close") == -2);

  // Stopping the server tears the surviving connection down and closes the fd.
  assert(websocket_stop_server(server_id) == 0);
  close(fd);
}

// The handshake fields as the RFCs allow them to be spelled (#434): RFC 9110
// 5.1 reads a field name in any case and 5.5 trims a value's blanks; RFC 6455
// 4.2.1 wants `Upgrade` to list `websocket` and `Connection` to list the token
// `Upgrade`, in any case and among any others.
static const char *const WS_UPGRADE_SPELLINGS[] = {"Upgrade: websocket", "upgrade: websocket",
                                                   "UPGRADE: WEBSOCKET", "uPgRaDe: wEbSoCkEt"};
static const char *const WS_CONNECTION_SPELLINGS[] = {
    "Connection: Upgrade",             "Connection: upgrade",
    "connection: keep-alive, Upgrade", "CONNECTION: KEEP-ALIVE, UPGRADE",
    "Connection:\tupgrade  ",          "Connection: keep-alive,upgrade"};
static const char *const WS_KEY_SPELLINGS[] = {
    WS_SAMPLE_KEY_LINE, "sec-websocket-key: " WS_SAMPLE_KEY "\r\n",
    "SEC-WEBSOCKET-KEY: " WS_SAMPLE_KEY "\r\n", "Sec-WebSocket-Key:   " WS_SAMPLE_KEY "\r\n"};
#define WS_SPELLINGS(list) (sizeof(list) / sizeof((list)[0]))

// Every combination of those spellings is a handshake, answered with the same
// accept token for the same key and then served.
static void test_handshake_spellings(void) {
  char fields[512], headers[1024];
  for (size_t u = 0; u < WS_SPELLINGS(WS_UPGRADE_SPELLINGS); u++) {
    for (size_t c = 0; c < WS_SPELLINGS(WS_CONNECTION_SPELLINGS); c++) {
      for (size_t k = 0; k < WS_SPELLINGS(WS_KEY_SPELLINGS); k++) {
        snprintf(fields, sizeof(fields), "%s\r\n%s\r\n%s", WS_UPGRADE_SPELLINGS[u],
                 WS_CONNECTION_SPELLINGS[c], WS_KEY_SPELLINGS[k]);
        int fd = ws_raw_connect(WS_LIVE_PORT);
        ws_send_upgrade_fields(fd, fields);
        assert(ws_read_reply(fd, headers, sizeof(headers)) && "a valid upgrade got no reply");
        assert(strstr(headers, "HTTP/1.1 101 Switching Protocols\r\n") == headers &&
               "a valid upgrade was refused");
        assert(strstr(headers, WS_SAMPLE_ACCEPT) != NULL);
        ws_expect_text(fd, "\"type\":\"welcome\"", NULL, NULL);
        close(fd);
      }
    }
  }
}

// A field value that says "Connection: Upgrade" is not a Connection field, a
// longer word is not the token, and a missing or malformed key is no handshake.
static const char *const WS_NOT_A_HANDSHAKE[] = {
    "X-Note: Connection: Upgrade\r\n" WS_SAMPLE_KEY_LINE,
    "x-note: connection: upgrade\r\n" WS_SAMPLE_KEY_LINE,
    "Referer: a; Connection: Upgrade; b\r\n" WS_SAMPLE_KEY_LINE,
    "Connection: keep-alive\r\n" WS_SAMPLE_KEY_LINE,
    "Connection: Upgradex\r\n" WS_SAMPLE_KEY_LINE,
    "Connection: \"Upgrade\"\r\n" WS_SAMPLE_KEY_LINE,
    "Connection: Upgrade\r\n",
    "Connection: Upgrade\r\nSec-WebSocket-Key: not a key\r\n"};

// An upgrade request that is not a handshake is answered 400, never a 101 and
// never silence, whatever the case of its Upgrade name.
static void test_malformed_upgrades_get_400(void) {
  char fields[512], headers[1024];
  for (size_t i = 0; i < WS_SPELLINGS(WS_NOT_A_HANDSHAKE); i++) {
    snprintf(fields, sizeof(fields), "%s\r\n%s", WS_UPGRADE_SPELLINGS[i % WS_SPELLINGS(WS_UPGRADE_SPELLINGS)],
             WS_NOT_A_HANDSHAKE[i]);
    int fd = ws_raw_connect(WS_LIVE_PORT);
    ws_send_upgrade_fields(fd, fields);
    assert(ws_read_reply(fd, headers, sizeof(headers)) && "a malformed upgrade got no reply");
    assert(strstr(headers, "HTTP/1.1 400 Bad Request\r\n") == headers &&
           "a malformed upgrade must get 400");
    assert(strstr(headers, WS_SAMPLE_ACCEPT) == NULL);
    ws_expect_peer_close(fd);
    close(fd);
  }
}

// A value that only says "Upgrade: websocket" asks for nothing, and neither
// does a list of other protocols: no 101, and the connection is not served.
static void test_field_values_are_not_upgrade(void) {
  static const char *const not_an_upgrade[] = {
      "X-Note: Upgrade: websocket\r\nConnection: Upgrade\r\n" WS_SAMPLE_KEY_LINE,
      "Upgrade: h2c\r\nConnection: Upgrade\r\n" WS_SAMPLE_KEY_LINE};
  char headers[1024];
  for (size_t i = 0; i < WS_SPELLINGS(not_an_upgrade); i++) {
    int fd = ws_raw_connect(WS_LIVE_PORT);
    ws_send_upgrade_fields(fd, not_an_upgrade[i]);
    bool replied = ws_read_reply(fd, headers, sizeof(headers));
    assert(!(replied && strstr(headers, "HTTP/1.1 101") == headers) &&
           "a field value is not an Upgrade field");
    ws_expect_peer_close(fd);
    close(fd);
  }
}

// Connections the server must refuse: one that says nothing, one that is not
// an upgrade, and one that upgrades without a key. Each is closed, not served.
static void test_non_websocket_connections(void) {
  int64_t server_id = ws_serve("127.0.0.1", "/chat", 0);

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
  int good = ws_upgrade_handshake();
  ws_expect_text(good, "welcome", NULL, NULL);
  close(good);
  // ...in every spelling, and refuses every look-alike on the same socket.
  test_handshake_spellings();
  test_malformed_upgrades_get_400();
  test_field_values_are_not_upgrade();
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
  // The dial is refused on a port this test itself just proved closed.
  char closed_url[64];
  snprintf(closed_url, sizeof(closed_url), "ws://127.0.0.1:%d/chat",
           test_loopback_closed_port());
  assert(websocket_connect(closed_url) == -6);

  // A name no resolver can answer: the label is past the DNS length limit.
  char long_host[400];
  memcpy(long_host, "ws://", 5);
  memset(long_host + 5, 'a', sizeof(long_host) - 8);
  memcpy(long_host + sizeof(long_host) - 3, "/x", 3);
  assert(websocket_connect(long_host) == -5);

  int listen_fd = test_loopback_listen(WS_DEAD_PORT);
  pthread_t thread;
  assert(pthread_create(&thread, NULL, ws_rude_listener, &listen_fd) == 0);
  char url[64];
  snprintf(url, sizeof(url), "ws://127.0.0.1:%d/chat", WS_DEAD_PORT);
  assert(websocket_connect(url) == -8); // handshake response never arrives
  assert(pthread_join(thread, NULL) == 0);
  close(listen_fd);
}

// A closed peer must return -3 even with the process's default SIGPIPE policy.
// The rest of this suite ignores SIGPIPE, which otherwise hides this defect.
static void ws_send_to_closed_peer(void) {
  assert(signal(SIGPIPE, SIG_DFL) != SIG_ERR);
  int sockets[2];
  assert(socketpair(AF_UNIX, SOCK_STREAM, 0, sockets) == 0);
  assert(close(sockets[1]) == 0);
  WebSocket connection = {.socket_fd = sockets[0], .is_connected = true};
  assert(pthread_mutex_init(&connection.mutex, NULL) == 0);
  websockets[1] = &connection;
  _exit(websocket_send(1, "closed peer") == -3 ? 0 : 1);
}

static void test_closed_peer_preserves_process(void) {
  pid_t child = fork();
  assert(child >= 0);
  if (child == 0) {
    ws_send_to_closed_peer();
  }
  int status = 0;
  assert(waitpid(child, &status, 0) == child);
  assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
}

// ------------------------------------------------------------- keep-alive

// The signaller waits until websocket_keep_alive has installed the process's
// SIGTERM handler, and only then sends the signal: one arriving before that
// meets the default disposition and kills the whole binary. The bound only
// names a keep_alive that stopped installing its handler; waiting for the
// installation is the mechanism, never the bound.
enum { WS_HANDLER_TRIES = 30000, WS_HANDLER_POLL_US = 1000 };

static void *ws_shutdown_signaller(void *arg) {
  (void)arg;
  for (int i = 0; i < WS_HANDLER_TRIES; i++) {
    struct sigaction installed;
    assert(sigaction(SIGTERM, NULL, &installed) == 0);
    if (installed.sa_handler != SIG_DFL && installed.sa_handler != SIG_IGN) {
      assert(kill(getpid(), SIGTERM) == 0);
      return NULL;
    }
    usleep(WS_HANDLER_POLL_US);
  }
  assert(0 && "keep_alive never installed its SIGTERM handler");
  return NULL;
}

// websocketKeepAlive blocks until SIGINT or SIGTERM, then stops every server
// [BUILTIN-WEBSOCKET-LIFECYCLE]. Runs LAST: it latches the shutdown flag.
static void test_keep_alive_until_signal(void) {
  int64_t server_id = ws_serve("127.0.0.1", "/chat", 0);
  pthread_t thread;
  assert(pthread_create(&thread, NULL, ws_shutdown_signaller, NULL) == 0);
  websocket_keep_alive();
  assert(pthread_join(thread, NULL) == 0);
  // keep_alive stopped every server, so the handle is already released.
  assert(websocket_stop_server(server_id) == 0);
  // ...and the port is free again.
  int64_t reborn = ws_serve("127.0.0.1", "/chat", 0);
  assert(websocket_stop_server(reborn) == 0);
}

int main(void) {
  signal(SIGPIPE, SIG_IGN); // a closed peer must surface as a send error
  test_closed_peer_preserves_process();
  test_handle_validation();
  test_listen_bind_conflict();
  test_live_loopback_exchange();
  test_non_websocket_connections();
  test_client_connect_failures();
  test_websocket_lifecycle();
  test_websocket_frames();
  test_keep_alive_until_signal(); // LAST: latches the shutdown flag
  printf("✅ WebSocket runtime tests passed\n");
  return 0;
}
