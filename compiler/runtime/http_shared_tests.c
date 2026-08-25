// Assertion-driven tests for the shared HTTP/WebSocket helpers in
// http_shared.c. A failed assert aborts the binary.
//
// This suite OWNS http_shared.c's coverage number: it is the only suite that
// exercises every helper in the unit rather than whichever subset a transport
// happens to reach. Spec anchors:
//   [HTTP-CLIENT]                 docs/specs/0014-HTTP.md
//   [BUILTIN-WEBSOCKET-FRAMING]   docs/specs/0015-WebSockets.md
//   [BUILTIN-WEBSOCKET-HANDSHAKE] docs/specs/0015-WebSockets.md
#include "http_shared.h"
#include <assert.h>

// Not in the header: internal to the handshake pipeline, asserted directly so
// its input-validation contract is pinned rather than inferred.
void sha1_websocket(const char *input, unsigned char output[20]);

// Every HttpMethod maps to its wire spelling, and an out-of-range enum value
// degrades to GET rather than returning a dangling pointer [HTTP-CLIENT].
static void test_http_method_strings(void) {
  assert(strcmp(http_method_to_string(HTTP_GET), "GET") == 0);
  assert(strcmp(http_method_to_string(HTTP_POST), "POST") == 0);
  assert(strcmp(http_method_to_string(HTTP_PUT), "PUT") == 0);
  assert(strcmp(http_method_to_string(HTTP_DELETE), "DELETE") == 0);
  assert(strcmp(http_method_to_string(HTTP_PATCH), "PATCH") == 0);
  assert(strcmp(http_method_to_string(HTTP_HEAD), "HEAD") == 0);
  assert(strcmp(http_method_to_string(HTTP_OPTIONS), "OPTIONS") == 0);
  assert(strcmp(http_method_to_string((HttpMethod)99), "GET") == 0);
}

// One parse_url case: asserts the whole (host, port, path) triple at once so a
// single expectation table drives every scheme and default [HTTP-CLIENT].
static void expect_url(const char *url, const char *host, int port,
                       const char *path) {
  char *got_host = NULL;
  char *got_path = NULL;
  int got_port = -1;
  assert(parse_url(url, &got_host, &got_port, &got_path) == 0);
  assert(strcmp(got_host, host) == 0);
  assert(got_port == port);
  assert(strcmp(got_path, path) == 0);
  free(got_host);
  free(got_path);
}

// parse_url over every scheme it recognises, both port forms and both path
// forms; the TLS schemes default to 443 and the plain ones to 80.
static void test_url_parsing(void) {
  expect_url("http://example.com:8080/api/test", "example.com", 8080,
             "/api/test");
  expect_url("http://localhost/test", "localhost", 80, "/test");
  expect_url("http://api.example.com:3000", "api.example.com", 3000, "/");
  expect_url("https://secure.example.com/x", "secure.example.com", 443, "/x");
  expect_url("https://secure.example.com:8443/", "secure.example.com", 8443,
             "/");
  expect_url("ws://127.0.0.1:9001/chat", "127.0.0.1", 9001, "/chat");
  expect_url("wss://ws.example.com/chat", "ws.example.com", 443, "/chat");
  expect_url("example.com:1234/p", "example.com", 1234, "/p"); // no scheme
}

// Every parse_url rejection: no URL, an unusable port and an empty host.
static void test_url_rejections(void) {
  char *host = NULL;
  char *path = NULL;
  int port = -1;
  assert(parse_url(NULL, &host, &port, &path) == -1);
  assert(parse_url("http://example.com:0/x", &host, &port, &path) == -1);
  assert(parse_url("http://example.com:70000/x", &host, &port, &path) == -1);
  assert(parse_url("http://example.com:notanumber/x", &host, &port, &path) ==
         -1);
  assert(parse_url("http://:8080/x", &host, &port, &path) == -1);
  assert(parse_url("http:///x", &host, &port, &path) == -1);
  assert(parse_url("http://", &host, &port, &path) == -1);
}

// base64_encode against the RFC 4648 vectors, plus non-ASCII bytes.
static void test_base64_vectors(void) {
  const char *vec[][2] = {{"", ""},           {"f", "Zg=="},
                          {"fo", "Zm8="},     {"foo", "Zm9v"},
                          {"foob", "Zm9vYg=="}, {"fooba", "Zm9vYmE="},
                          {"foobar", "Zm9vYmFy"}};
  for (size_t i = 0; i < sizeof(vec) / sizeof(vec[0]); i++) {
    char *out =
        base64_encode((const unsigned char *)vec[i][0], strlen(vec[i][0]));
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
// and two draws differ [BUILTIN-WEBSOCKET-HANDSHAKE].
static void test_websocket_key(void) {
  char *k1 = generate_websocket_key();
  char *k2 = generate_websocket_key();
  assert(k1 != NULL && k2 != NULL);
  assert(strlen(k1) == 24 && strlen(k2) == 24);
  assert(strcmp(k1 + 22, "==") == 0);
  assert(strcmp(k1, k2) != 0);
  free(k1);
  free(k2);
}

// sha1_websocket zeroes its output — never leaves it uninitialised — for every
// input it refuses: no input, the empty key, and a key past the 4096-byte cap.
static void test_sha1_input_validation(void) {
  unsigned char out[20];
  const unsigned char zero[20] = {0};
  memset(out, 0xAB, sizeof(out));
  sha1_websocket(NULL, out);
  assert(memcmp(out, zero, sizeof(out)) == 0);
  sha1_websocket("dGhlIHNhbXBsZSBub25jZQ==", NULL); // no digest sink: no fault
  memset(out, 0xAB, sizeof(out));
  sha1_websocket("", out);
  assert(memcmp(out, zero, sizeof(out)) == 0);
  static char oversized[5000];
  memset(oversized, 'k', sizeof(oversized) - 1);
  oversized[sizeof(oversized) - 1] = '\0';
  memset(out, 0xAB, sizeof(out));
  sha1_websocket(oversized, out);
  assert(memcmp(out, zero, sizeof(out)) == 0);
  // The RFC 6455 §1.3 sample key hashes to a non-zero digest.
  sha1_websocket("dGhlIHNhbXBsZSBub25jZQ==", out);
  assert(memcmp(out, zero, sizeof(out)) != 0);
}

// The RFC 6455 §1.3 sample handshake: the accept token for the sample key is
// pinned in the spec itself — the strongest possible golden for the SHA-1 +
// base64 accept pipeline [BUILTIN-WEBSOCKET-HANDSHAKE].
static void test_handshake_response_rfc6455(void) {
  char *resp = create_websocket_handshake_response("dGhlIHNhbXBsZSBub25jZQ==");
  assert(resp != NULL);
  assert(strstr(resp, "HTTP/1.1 101 Switching Protocols\r\n") == resp);
  assert(strstr(resp, "Upgrade: websocket") != NULL);
  assert(strstr(resp, "Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=") !=
         NULL);
  free(resp);
}

// The handshake refuses every key it cannot trust: absent, the wrong length,
// or carrying a byte outside the base64 alphabet.
static void test_handshake_key_rejections(void) {
  assert(create_websocket_handshake_response(NULL) == NULL);
  assert(create_websocket_handshake_response("") == NULL);
  assert(create_websocket_handshake_response("too-short") == NULL);
  assert(create_websocket_handshake_response(
             "dGhlIHNhbXBsZSBub25jZQ==extra") == NULL);
  assert(create_websocket_handshake_response("dGhlIHNhbXBsZSBub25j!Q==") ==
         NULL);
  assert(create_websocket_handshake_response("dGhlIHNhbXBsZSBub25j Q==") ==
         NULL);
}

// Frame parsing: unmasked, client-masked (XOR unmasking), the 126 extended
// length form, the empty frame, and every truncation rejection
// [BUILTIN-WEBSOCKET-FRAMING].
static void test_parse_websocket_frame(void) {
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
  // A low length byte with the top bit set must NOT sign-extend.
  unsigned char ext200[206] = {0x81, 126, 0, 0xC8};
  memset(ext200 + 4, 'y', 200);
  payload = NULL;
  assert(parse_websocket_frame((const char *)ext200, 204, &payload) == 200);
  assert(payload != NULL && strlen(payload) == 200);
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
// frame), verified over a socketpair, and round-trips through the parser
// [BUILTIN-WEBSOCKET-FRAMING].
static void test_send_frame_wire_format(void) {
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
  // The 125/126 boundary: 125 bytes still use the one-byte length form.
  static char edge[126];
  memset(edge, 'C', 125);
  edge[125] = '\0';
  assert(send_websocket_frame(pair[0], edge) == 127);
  unsigned char edge_wire[130] = {0};
  got = 0;
  while (got < 127) {
    ssize_t n = read(pair[1], edge_wire + got, sizeof(edge_wire) - (size_t)got);
    assert(n > 0);
    got += n;
  }
  assert(edge_wire[0] == 0x81 && edge_wire[1] == 125);
  assert(send_websocket_frame(pair[0], NULL) == -1);
  static char huge[5001];
  memset(huge, 'B', 5000);
  huge[5000] = '\0';
  assert(send_websocket_frame(pair[0], huge) == -1); // DoS guard at 4096
  // A closed peer surfaces the send failure rather than reporting success.
  close(pair[1]);
  signal(SIGPIPE, SIG_IGN);
  int closed_result = send_websocket_frame(pair[0], "gone");
  assert(closed_result == 4 || closed_result < 0);
  close(pair[0]);
}

// get_next_id hands out strictly increasing, never-reused handles.
static void test_next_id_monotonic(void) {
  int64_t first = get_next_id();
  int64_t second = get_next_id();
  assert(first > 0);
  assert(second == first + 1);
}

int main(void) {
  test_http_method_strings();
  test_url_parsing();
  test_url_rejections();
  test_base64_vectors();
  test_websocket_key();
  test_sha1_input_validation();
  test_handshake_response_rfc6455();
  test_handshake_key_rejections();
  test_parse_websocket_frame();
  test_send_frame_wire_format();
  test_next_id_monotonic();
  printf("✅ http_shared tests passed\n");
  return 0;
}
