// RFC 6455 framing against a real loopback server [BUILTIN-WEBSOCKET-FRAMING]:
// every length form, fragmented messages with control frames between their
// fragments, ping and pong, the closing handshake, frames split across and packed
// into TCP segments, and every frame a server must refuse, each with the close
// code it is refused with. Client frames are built by hand here, so no test
// trusts the encoder or parser it checks.
#include "http_shared.h"
#include "test_alloc.h"
#include "websocket_test_helpers.h"

#include <assert.h>

enum { FRAMES_PORT = 18097, OVER_THE_LIMIT = 16385, PACKED_FRAMES = 3, CODEC_LIMIT = 65536, ODD_SIZE = 333 };
enum { OP_CONT = 0x0, OP_TEXT = 0x1, OP_BINARY = 0x2, OP_CLOSE = 0x8, OP_PING = 0x9, OP_PONG = 0xA };
enum { FIN = 0x80, RSV1 = 0x40, MASKED = 0x80 };

static const unsigned char KEY[4] = {0x37, 0xfa, 0x21, 0x3d};
static char *const ADDRESS = (char *)(uintptr_t) "127.0.0.1";
static char *const PATH = (char *)(uintptr_t) "/frames";

// The length field in the form RFC 6455 requires for `len`, after the mask bit.
static size_t put_length(unsigned char *out, size_t len, unsigned char mask) {
  size_t width = len < 126 ? 0 : len <= 0xFFFF ? 2 : 8;
  out[0] = (unsigned char)(mask | (width == 0 ? len : width == 2 ? 126 : 127));
  for (size_t i = 0; i < width; i++) {
    out[1 + i] = (unsigned char)((uint64_t)len >> (8 * (width - 1 - i)));
  }
  return 1 + width;
}

// One whole frame into `out` (room for 14 + len bytes); its size.
static size_t build_frame(unsigned char *out, unsigned char first, const char *payload, size_t len,
                          bool masked) {
  out[0] = first;
  size_t at = 1 + put_length(out + 1, len, masked ? MASKED : 0);
  if (masked) {
    memcpy(out + at, KEY, sizeof KEY);
    at += sizeof KEY;
  }
  for (size_t i = 0; i < len; i++) {
    out[at + i] = (unsigned char)((unsigned char)payload[i] ^ (masked ? KEY[i % 4] : 0));
  }
  return at + len;
}

static void send_frame(int fd, unsigned char first, const char *payload, size_t len, bool masked) {
  unsigned char *frame = malloc(14 + len);
  assert(frame != NULL);
  ws_send_all(fd, frame, build_frame(frame, first, payload, len, masked));
  free(frame);
}

static void send_text(int fd, const char *text) {
  send_frame(fd, FIN | OP_TEXT, text, strlen(text), true);
}

// The next server frame is `opcode` carrying exactly `expected` (`len` bytes).
static void expect_frame(int fd, unsigned char opcode, const char *expected, size_t len) {
  unsigned char got = 0;
  size_t length = 0;
  char *payload = ws_read_frame(fd, &got, &length);
  assert(got == opcode && length == len && memcmp(payload, expected, len) == 0);
  free(payload);
}

// The next server frame is the echo of `original`, the whole message.
static void expect_echo(int fd, const char *original) {
  char *echo = ws_read_text_frame(fd);
  char needle[256];
  snprintf(needle, sizeof needle, "{\"type\":\"echo\",\"original\":\"%s\"", original);
  assert(strncmp(echo, needle, strlen(needle)) == 0);
  free(echo);
}

// The server answers with a close frame carrying `code` (0: no status at all),
// then ends the connection.
static void expect_close(int fd, int code) {
  unsigned char body[2] = {(unsigned char)(code >> 8), (unsigned char)(code & 0xFF)};
  expect_frame(fd, OP_CLOSE, (const char *)body, code ? 2 : 0);
  ws_expect_peer_close(fd);
  close(fd);
}

static int frames_client(void) {
  int fd = ws_raw_connect(FRAMES_PORT);
  ws_send_upgrade(fd, "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n");
  char headers[1024];
  ws_read_headers(fd, headers, sizeof headers);
  assert(strstr(headers, "HTTP/1.1 101") == headers);
  char *welcome = ws_read_text_frame(fd);
  assert(strstr(welcome, "\"type\":\"welcome\"") != NULL);
  free(welcome);
  return fd;
}

// Pings answered with their own payload, empty and at the 125-byte control
// limit; an unsolicited pong answered with nothing.
static void check_ping_pong(int fd) {
  send_frame(fd, FIN | OP_PING, "are-you-there", 13, true);
  expect_frame(fd, OP_PONG, "are-you-there", 13);
  send_frame(fd, FIN | OP_PING, "", 0, true);
  expect_frame(fd, OP_PONG, "", 0);
  char widest[125];
  memset(widest, 'p', sizeof widest);
  send_frame(fd, FIN | OP_PING, widest, sizeof widest, true);
  expect_frame(fd, OP_PONG, widest, sizeof widest);
  send_frame(fd, FIN | OP_PONG, "unasked", 7, true);
  send_text(fd, "after-pong");
  expect_echo(fd, "after-pong");
}

// A message in fragments is delivered once, whole; a ping between its fragments
// is answered at once, before the message completes.
static void check_fragments(int fd) {
  send_frame(fd, OP_TEXT, "frag-", 5, true);
  send_frame(fd, FIN | OP_PING, "mid", 3, true);
  send_frame(fd, OP_CONT, "men", 3, true);
  send_frame(fd, FIN | OP_CONT, "ted", 3, true);
  expect_frame(fd, OP_PONG, "mid", 3);
  expect_echo(fd, "frag-mented");
  send_frame(fd, OP_TEXT, "", 0, true);
  send_frame(fd, FIN | OP_CONT, "late-start", 10, true);
  expect_echo(fd, "late-start");
}

// Text at each side of the one-byte and 16-bit length boundaries arrives whole.
static void check_length_forms(int fd) {
  const size_t sizes[] = {1, 125, 126, 127, 200};
  for (size_t i = 0; i < sizeof sizes / sizeof sizes[0]; i++) {
    char text[201];
    memset(text, 'a' + (int)i, sizes[i]);
    text[sizes[i]] = '\0';
    send_text(fd, text);
    char *echo = ws_read_text_frame(fd);
    assert(strstr(echo, text) != NULL);
    free(echo);
  }
}

// One frame dribbled a byte per segment, then several frames in one segment.
static void check_segments(int fd) {
  unsigned char frame[64];
  size_t size = build_frame(frame, FIN | OP_TEXT, "dribbled", 8, true);
  for (size_t i = 0; i < size; i++) {
    ws_send_all(fd, frame + i, 1);
    usleep(2000);
  }
  expect_echo(fd, "dribbled");
  unsigned char packed[3 * 64];
  size_t at = 0;
  const char *words[PACKED_FRAMES] = {"one", "two", "three"};
  for (int i = 0; i < PACKED_FRAMES; i++) {
    at += build_frame(packed + at, FIN | OP_TEXT, words[i], strlen(words[i]), true);
  }
  ws_send_all(fd, packed, at);
  for (int i = 0; i < PACKED_FRAMES; i++) {
    expect_echo(fd, words[i]);
  }
}

// The closing handshake: the server echoes the status it was sent and ends.
static void check_closing(void) {
  const int codes[] = {1000, 1001, 3000, 4999};
  for (size_t i = 0; i < sizeof codes / sizeof codes[0]; i++) {
    int fd = frames_client();
    unsigned char body[5] = {(unsigned char)(codes[i] >> 8), (unsigned char)(codes[i] & 0xFF), 'b', 'y', 'e'};
    send_frame(fd, FIN | OP_CLOSE, (const char *)body, sizeof body, true);
    expect_close(fd, codes[i]);
  }
  int fd = frames_client();
  send_frame(fd, FIN | OP_CLOSE, "", 0, true);
  expect_close(fd, 0);
}

// One frame a server must refuse, on a fresh connection, refused with `code`.
static void expect_refusal(unsigned char first, const char *payload, size_t len, bool masked, int code) {
  int fd = frames_client();
  send_frame(fd, first, payload, len, masked);
  expect_close(fd, code);
}

// The header of a masked text frame whose length field says `len` in a `width`
// byte field (2 or 8): refused with `code`, never treated as a frame.
static void expect_length_refusal(size_t len, size_t width, int code) {
  unsigned char header[10] = {FIN | OP_TEXT, (unsigned char)(MASKED | (width == 2 ? 126 : 127))};
  for (size_t i = 0; i < width; i++) {
    header[2 + i] = (unsigned char)((uint64_t)len >> (8 * (width - 1 - i)));
  }
  int fd = frames_client();
  ws_send_all(fd, header, 2 + width);
  expect_close(fd, code);
}

static void check_protocol_refusals(void) {
  expect_refusal(FIN | OP_TEXT, "bare", 4, false, 1002); // clients MUST mask
  expect_refusal(FIN | OP_CONT, "orphan", 6, true, 1002); // nothing to continue
  expect_refusal(FIN | RSV1 | OP_TEXT, "rsv", 3, true, 1002);
  expect_refusal(FIN | 0x3, "op3", 3, true, 1002);
  expect_refusal(FIN | 0xB, "opB", 3, true, 1002);
  expect_refusal(OP_PING, "split", 5, true, 1002); // control frames are never fragmented
  char wide[126];
  memset(wide, 'w', sizeof wide);
  expect_refusal(FIN | OP_PING, wide, sizeof wide, true, 1002);
  expect_refusal(FIN | OP_CLOSE, "x", 1, true, 1002);
  const unsigned char reserved[][2] = {{0x03, 0xE7}, {0x03, 0xED}, {0x03, 0xEE}, {0x0B, 0xB7}, {0x13, 0x88}};
  for (size_t i = 0; i < sizeof reserved / sizeof reserved[0]; i++) {
    expect_refusal(FIN | OP_CLOSE, (const char *)reserved[i], 2, true, 1002);
  }
  expect_length_refusal(5, 2, 1002); // not the minimal form
  expect_length_refusal((size_t)0x8000000000000001, 8, 1002); // the 64-bit top bit
}

// A new message while one is still in fragments.
static void check_interleaved_message(void) {
  int fd = frames_client();
  send_frame(fd, OP_TEXT, "first", 5, true);
  send_frame(fd, FIN | OP_TEXT, "second", 6, true);
  expect_close(fd, 1002);
}

// Data the server does not take: binary (1003), and anything over its limit as
// one frame, as a 64-bit length, or as fragments that add up past it (1009).
static void check_unacceptable_data(void) {
  expect_refusal(FIN | OP_BINARY, "\x01\x02", 2, true, 1003);
  char *big = malloc(OVER_THE_LIMIT);
  assert(big != NULL);
  memset(big, 'z', OVER_THE_LIMIT);
  expect_length_refusal(OVER_THE_LIMIT, 2, 1009);
  expect_length_refusal(0x11170, 8, 1009);
  int fd = frames_client();
  send_frame(fd, OP_TEXT, big, OVER_THE_LIMIT / 2, true);
  send_frame(fd, FIN | OP_CONT, big, OVER_THE_LIMIT / 2 + 1, true);
  expect_close(fd, 1009);
  free(big);
}

// `frame` (`size` bytes) parses to `want` and allocates nothing.
static void expect_parse_result(const unsigned char *frame, size_t size, int64_t want) {
  WsFrame parsed = {0};
  assert(parse_websocket_frame((const char *)frame, size, CODEC_LIMIT, &parsed) == want);
  assert(parsed.payload == NULL);
}

// Out of memory is a close with 1011 or a refusal, never a crash or a leak: the
// payload copy, the reassembly buffer, and a connection's stream, each failed
// by its exact size so no other allocation is touched.
static void check_out_of_memory(void) {
  char text[ODD_SIZE] = {0};
  unsigned char frame[ODD_SIZE + WS_HEADER_MAX];
  size_t size = build_frame(frame, FIN | OP_TEXT, text, ODD_SIZE, true);
  osp_alloc_fail_size(ODD_SIZE + 1);
  expect_parse_result(frame, size, WS_FRAME_NO_MEMORY);
  WsStream *stream = calloc(1, sizeof *stream);
  assert(stream != NULL);
  stream->held = build_frame((unsigned char *)stream->data, FIN | OP_TEXT, text, ODD_SIZE, true);
  osp_alloc_fail_size(ODD_SIZE + 1);
  WsEvent event = ws_stream_next(stream);
  assert(event.kind == WS_EVENT_CLOSE && event.close_code == WS_CLOSE_INTERNAL && event.payload == NULL);
  stream->held = build_frame((unsigned char *)stream->data, OP_TEXT, "abc", 3, true);
  assert(ws_stream_next(stream).kind == WS_EVENT_NONE && stream->in_message);
  stream->held = build_frame((unsigned char *)stream->data, FIN | OP_CONT, "defg", 4, true);
  osp_alloc_fail_size(3 + 4 + 1);
  event = ws_stream_next(stream);
  assert(event.kind == WS_EVENT_CLOSE && event.close_code == WS_CLOSE_INTERNAL);
  ws_stream_release(stream);
  osp_alloc_fail_off();
}

// A server that cannot hold a connection's stream says so with 1011 and ends it.
static void check_connection_out_of_memory(void) {
  int fd = frames_client();
  send_text(fd, "served-first"); // its stream exists before the failure is armed
  expect_echo(fd, "served-first");
  osp_alloc_fail_size(sizeof(WsStream));
  int other = frames_client();
  expect_close(other, WS_CLOSE_INTERNAL);
  osp_alloc_fail_off();
  send_text(fd, "still-served");
  expect_echo(fd, "still-served");
  close(fd);
}

static void check_live_protocol(void) {
  int64_t server = websocket_create_server(FRAMES_PORT, ADDRESS, PATH);
  assert(server > 0 && websocket_server_listen(server) == 0);
  int fd = frames_client();
  check_ping_pong(fd);
  check_fragments(fd);
  check_length_forms(fd);
  check_segments(fd);
  close(fd);
  check_closing();
  check_protocol_refusals();
  check_interleaved_message();
  check_unacceptable_data();
  check_connection_out_of_memory();
  assert(websocket_stop_server(server) == 0);
}

// ------------------------------------------------------------ frame codec

// `frame` (`size` bytes) parses whole: the bytes it took and what it found.
static void expect_parsed(const unsigned char *frame, size_t size, unsigned char first, const char *payload,
                          size_t len, bool masked) {
  WsFrame parsed = {0};
  assert(parse_websocket_frame((const char *)frame, size, CODEC_LIMIT, &parsed) == (int64_t)size);
  assert(parsed.fin == ((first & FIN) != 0) && parsed.opcode == (first & 0x0F) && parsed.masked == masked);
  assert(parsed.payload_len == len && memcmp(parsed.payload, payload, len) == 0 && parsed.payload[len] == '\0');
  free(parsed.payload);
}

// Every proper prefix of a frame is part of one: nothing taken, nothing allocated.
static void expect_every_prefix_partial(const unsigned char *frame, size_t size) {
  size_t step = size > 512 ? 251 : 1; // sample big frames, cover small ones byte by byte
  for (size_t cut = 0; cut < size; cut += step) {
    expect_parse_result(frame, cut, WS_FRAME_PARTIAL);
  }
  expect_parse_result(frame, size - 1, WS_FRAME_PARTIAL); // the last byte is what completes it
}

// Each length at the edges of RFC 6455's three forms, masked and not: parsed
// whole, partial at every cut, and encoded byte for byte as built by hand here.
static void check_length_boundaries(void) {
  const size_t sizes[] = {0, 1, 125, 126, 127, 65535, 65536};
  char *payload = malloc(65536);
  unsigned char *built = malloc(65536 + 14), *encoded = malloc(65536 + 14);
  assert(payload && built && encoded);
  for (size_t i = 0; i < 65536; i++) {
    payload[i] = (char)(i * 7);
  }
  for (size_t i = 0; i < sizeof sizes / sizeof sizes[0]; i++) {
    for (int masked = 0; masked <= 1; masked++) {
      size_t size = build_frame(built, FIN | OP_TEXT, payload, sizes[i], masked);
      expect_parsed(built, size, FIN | OP_TEXT, payload, sizes[i], masked);
      expect_every_prefix_partial(built, size);
      assert(ws_encode_frame(encoded, OP_TEXT, payload, sizes[i], masked ? KEY : NULL) == size);
      assert(memcmp(encoded, built, size) == 0);
    }
  }
  free(payload);
  free(built);
  free(encoded);
}

// Opcodes and FIN are reported, not ignored; one frame is taken from several.
static void check_frame_kinds(void) {
  const unsigned char firsts[] = {FIN | OP_TEXT, OP_TEXT, OP_CONT, FIN | OP_CONT, FIN | OP_BINARY,
                                  FIN | OP_CLOSE, FIN | OP_PING, FIN | OP_PONG};
  unsigned char frame[64];
  for (size_t i = 0; i < sizeof firsts; i++) {
    expect_parsed(frame, build_frame(frame, firsts[i], "kind", 4, i % 2), firsts[i], "kind", 4, i % 2);
  }
  unsigned char two[64];
  size_t first = build_frame(two, FIN | OP_TEXT, "ab", 2, true);
  size_t both = first + build_frame(two + first, FIN | OP_PING, "cd", 2, true);
  WsFrame parsed = {0};
  assert(parse_websocket_frame((const char *)two, both, 100, &parsed) == (int64_t)first);
  assert(strcmp(parsed.payload, "ab") == 0);
  free(parsed.payload);
  // The extended length's low byte read unsigned: 0xC8 is 200, not a sign-extended size.
  unsigned char ext200[204] = {FIN | OP_TEXT, 126, 0, 0xC8};
  memset(ext200 + 4, 'y', 200);
  expect_parsed(ext200, sizeof ext200, FIN | OP_TEXT, (const char *)ext200 + 4, 200, false);
}

typedef struct {
  unsigned char bytes[10];
  size_t len;
  int64_t result;
} HeaderCase;

// Headers RFC 6455 forbids, and lengths past the caller's limit.
static const HeaderCase REFUSED[] = {
    {{FIN | RSV1 | OP_TEXT, 0}, 2, WS_FRAME_INVALID},
    {{FIN | 0x20 | OP_TEXT, 0}, 2, WS_FRAME_INVALID},
    {{FIN | 0x10 | OP_TEXT, 0}, 2, WS_FRAME_INVALID},
    {{FIN | 0x3, 0}, 2, WS_FRAME_INVALID},
    {{FIN | 0x7, 0}, 2, WS_FRAME_INVALID},
    {{FIN | 0xB, 0}, 2, WS_FRAME_INVALID},
    {{FIN | 0xF, 0}, 2, WS_FRAME_INVALID},
    {{OP_PING, 0}, 2, WS_FRAME_INVALID},
    {{OP_CLOSE, 0}, 2, WS_FRAME_INVALID},
    {{FIN | OP_PONG, 126, 0, 126}, 4, WS_FRAME_INVALID},
    {{FIN | OP_TEXT, 126, 0, 125}, 4, WS_FRAME_INVALID},
    {{FIN | OP_TEXT, 127, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF}, 10, WS_FRAME_INVALID},
    {{FIN | OP_TEXT, 127, 0x80, 0, 0, 0, 0, 0, 0, 0}, 10, WS_FRAME_INVALID},
    {{FIN | OP_TEXT, 127, 0, 0, 0, 0, 0, 1, 0, 1}, 10, WS_FRAME_TOO_BIG},
    {{FIN | OP_TEXT, 127, 0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF}, 10, WS_FRAME_TOO_BIG},
};

static void check_header_refusals(void) {
  for (size_t i = 0; i < sizeof REFUSED / sizeof REFUSED[0]; i++) {
    expect_parse_result(REFUSED[i].bytes, REFUSED[i].len, REFUSED[i].result);
  }
  // The limit is inclusive: 100 bytes are taken at a limit of 100, 101 are not.
  char zeros[101] = {0};
  unsigned char at_limit[110];
  WsFrame parsed = {0};
  size_t size = build_frame(at_limit, FIN | OP_TEXT, zeros, 100, false);
  assert(parse_websocket_frame((const char *)at_limit, size, 100, &parsed) == (int64_t)size);
  free(parsed.payload);
  parsed = (WsFrame){0};
  size = build_frame(at_limit, FIN | OP_TEXT, zeros, 101, false);
  assert(parse_websocket_frame((const char *)at_limit, size, 100, &parsed) == WS_FRAME_TOO_BIG);
}

// The server's wire bytes, over a socketpair: unmasked text in the one-byte and
// 16-bit forms, a control frame, a masked client frame, and every refusal.
static void check_send_wire_format(void) {
  int pair[2];
  assert(socketpair(AF_UNIX, SOCK_STREAM, 0, pair) == 0);
  unsigned char wire[200];
  assert(send_websocket_frame(pair[0], "hi") == 4);
  assert(ws_read_exact(pair[1], wire, 4) && memcmp(wire, "\x81\x02hi", 4) == 0);
  char text[131];
  memset(text, 'A', 130);
  text[130] = '\0';
  assert(send_websocket_frame(pair[0], text) == 134);
  assert(ws_read_exact(pair[1], wire, 134) && memcmp(wire, "\x81\x7e\x00\x82", 4) == 0 && memcmp(wire + 4, text, 130) == 0);
  text[125] = '\0';
  assert(send_websocket_frame(pair[0], text) == 127);
  assert(ws_read_exact(pair[1], wire, 127) && wire[1] == 125);
  assert(send_websocket_message(pair[0], OP_PONG, "pp", 2, false) == 4);
  assert(ws_read_exact(pair[1], wire, 4) && memcmp(wire, "\x8a\x02pp", 4) == 0);
  assert(send_websocket_message(pair[0], OP_TEXT, "mask", 4, true) == 10);
  assert(ws_read_exact(pair[1], wire, 10) && wire[0] == 0x81 && wire[1] == (MASKED | 4));
  for (size_t i = 0; i < 4; i++) {
    assert((wire[6 + i] ^ wire[2 + i % 4]) == "mask"[i]);
  }
  close(pair[1]);
  int closed = send_websocket_frame(pair[0], "gone"); // a closed peer is a failure, never a crash
  assert(closed == 4 || closed < 0);
  close(pair[0]);
}

static void check_send_refusals(void) {
  assert(send_websocket_frame(3, NULL) == -1);
  assert(send_websocket_message(3, OP_TEXT, NULL, 1, false) == -1);
  char *huge = malloc(WS_SEND_LIMIT + 2);
  assert(huge != NULL);
  memset(huge, 'B', WS_SEND_LIMIT + 1);
  huge[WS_SEND_LIMIT + 1] = '\0';
  assert(send_websocket_frame(3, huge) == -1); // the outbound limit
  free(huge);
  assert(send_websocket_frame(-1, "invalid socket") < 0);
}

static void check_codec(void) {
  check_length_boundaries();
  check_frame_kinds();
  check_header_refusals();
  check_send_wire_format();
  check_send_refusals();
  check_out_of_memory();
}

void test_websocket_frames(void) {
  check_codec();
  check_live_protocol();
}
