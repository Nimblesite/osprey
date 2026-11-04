// How long and how much the client reads, against a scripted origin, split from
// http_framing_tests.c so each test TU stays under the repo size budget: the size
// limit, the whole-read bound, and stopping at the end of the framing, however long
// the server holds the socket open [HTTP-RESPONSE-FRAMING].
#include "http_client_internal.h"
#include "http_runtime_tests.h"
#include "http_shared.h"
#include "io_error.h"

#include <assert.h>
#include <stdio.h>
#include <unistd.h>

static const char OVER_SIZE[] = "http response: size: over the limit";
static const char OVER_TIME[] = "http response: read: over the time limit";
static const char BODY_CUT[] = "http response: body: truncated";
static const char FAULTY_CODING[] = "http response: head: Transfer-Encoding on HTTP/1.0";

static int64_t elapsed_ms_since(uint64_t started) {
  return (int64_t)(osp_monotonic_ms() - started);
}

static void use_client_with_timeout(int64_t timeout_ms) {
  shared_origin_client =
      http_create_client((char *)(uintptr_t) "http://127.0.0.1:18088", timeout_ms);
  assert(shared_origin_client > 0);
}

// Answered as framed, with the socket still open, in far less than the timeout.
static void expect_prompt(const char *reply, size_t len, int64_t status, const char *body) {
  uint64_t started = osp_monotonic_ms();
  expect_response_n(reply, len, status, body);
  assert(elapsed_ms_since(started) < 1500); // the timeout is 20 s
}

#define EXPECT_PROMPT(reply, status, body)                                     \
  expect_prompt(reply, sizeof(reply) - 1, status, body)

// Framing that is complete ends the read, so a server that keeps the connection
// alive costs nothing. Bytes past the framing aren't part of the response.
static void check_stops_once_framed(void) {
  use_client_with_timeout(20000);
  origin_holds_connections = true;
  EXPECT_PROMPT("HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello", 200, "hello");
  EXPECT_PROMPT("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n", 200, "");
  EXPECT_PROMPT("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nokEXTRA", 200, "ok");
  EXPECT_PROMPT("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok\0NUL past the framing", 200, "ok");
  EXPECT_PROMPT("HTTP/1.1 500 Boom\r\nContent-Length: 2\r\n\r\nno", 500, "no");
  EXPECT_PROMPT("HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\nok", 200, "ok");
  EXPECT_PROMPT("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\nok", 200, "ok");
  EXPECT_PROMPT("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n", 200, "");
  EXPECT_PROMPT("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nok\r\n0\r\n\r\n", 200, "ok");
  EXPECT_PROMPT("HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\n2;a=b\r\nok\r\n0;c\r\nX-T: 1\r\nX-U: 2\r\n\r\nEXTRA", 200, "ok");
  EXPECT_PROMPT("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 1\r\n\r\n2\r\nok\r\n0\r\n\r\n", 200, "ok");
  EXPECT_PROMPT("HTTP/1.1 204 No Content\r\nContent-Length: 9\r\n\r\n", 204, "");
  EXPECT_PROMPT("HTTP/1.1 304 Not Modified\r\nTransfer-Encoding: chunked\r\n\r\n", 304, "");
  EXPECT_PROMPT("HTTP/1.1 101 Switching\r\n\r\n", 101, "");
  EXPECT_PROMPT("HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok", 200, "ok");
  EXPECT_PROMPT("HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 103 Hints\r\nLink: </a>\r\n\r\nHTTP/1.1 204 No\r\n\r\n", 204, "");
  http_close_client(shared_origin_client);
}

// What is refused for its framing needs nothing further from the server either.
static void check_stops_at_the_fault(void) {
  use_client_with_timeout(20000);
  origin_holds_connections = true;
  static const struct { const char *reply; int64_t code; const char *message; } cases[] = {
      {"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\nok\r\n", -14, "http response: chunked body: invalid chunk size"},
      {"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nokXX", -14, "http response: chunked body: chunk data not followed by CRLF"},
      {"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nok\rX", -14, "http response: chunked body: chunk data not followed by CRLF"},
      {"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\nX\001: 1\r\n\r\n", -14, "http response: chunked body: invalid trailer"},
      {"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\nX\001: 1\r\n", -14, "http response: chunked body: invalid trailer"},
      {"HTTP/1.1 200 OK\r\nContent-Length: x\r\n\r\nok", -14, "http response: body: invalid Content-Length"},
      {"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nok", -14, "http response: body: invalid Content-Length"},
      {"HTTP/1.1 200 OK\r\nno colon\r\n\r\nok", -14, "http response: head: malformed"},
      {"HTTP/1.1 100 Continue\r\nno colon\r\n\r\nok", -14, "http response: head: malformed"},
      {"HTTP/9 200 OK\r\n\r\nok", -8, "http response: status line: not HTTP/1.x"},
      {"HTTP/1.0 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n", -14, FAULTY_CODING},
  };
  for (size_t i = 0; i < sizeof(cases) / sizeof(cases[0]); i++) {
    uint64_t started = osp_monotonic_ms();
    expect_failure_n(cases[i].reply, strlen(cases[i].reply), cases[i].code, cases[i].message);
    assert(elapsed_ms_since(started) < 1500);
  }
  http_close_client(shared_origin_client);
}

// Neither can the body hide behind a server that never closes: what needs the close
// still gets the timeout, and is still refused.
static void check_what_framing_cannot_end(void) {
  use_client_with_timeout(100);
  origin_holds_connections = true;
  static const char *const replies[] = {
      "HTTP/1.1 200 OK\r\n\r\nhalf",
      "HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\nContent-Length: 4\r\n\r\nhalf",
      "HTTP/1.1 200 OK\r\nContent-Length: 18446744073709551615\r\n\r\nhalf",
  };
  for (size_t i = 0; i < sizeof(replies) / sizeof(replies[0]); i++) {
    expect_failure_n(replies[i], strlen(replies[i]), -14, BODY_CUT);
  }
  http_close_client(shared_origin_client);
}

// RFC 9112 6.1: Transfer-Encoding on HTTP/1.0 is faulty framing, even with a
// Content-Length, even on a status with no body.
static void check_transfer_encoding_on_http_1_0(void) {
  use_client_with_timeout(5000);
  origin_holds_connections = false;
  static const char *const refused[] = {
      "HTTP/1.0 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nok\r\n0\r\n\r\n",
      "HTTP/1.0 200 OK\r\nTransfer-Encoding: gzip\r\n\r\nok",
      "HTTP/1.0 200 OK\r\ntransfer-encoding: GZIP, CHUNKED\r\n\r\n2\r\nok\r\n0\r\n\r\n",
      "HTTP/1.0 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 2\r\n\r\nok",
      "HTTP/1.0 200 OK\r\nContent-Length: 2\r\nTransfer-Encoding: gzip\r\n\r\nok",
      "HTTP/1.0 200 OK\r\nX-A: 1\r\nTransfer-Encoding: gzip\r\nX-B: 2\r\n\r\nok",
      "HTTP/1.0 204 No Content\r\nTransfer-Encoding: chunked\r\n\r\n",
      "HTTP/1.0 304 Not Modified\r\nTransfer-Encoding: chunked\r\n\r\n",
      "HTTP/1.1 100 Continue\r\n\r\nHTTP/1.0 200 OK\r\nTransfer-Encoding: gzip\r\n\r\nok",
  };
  for (size_t i = 0; i < sizeof(refused) / sizeof(refused[0]); i++) {
    expect_failure_n(refused[i], strlen(refused[i]), -14, FAULTY_CODING);
  }
  EXPECT_RESPONSE("HTTP/1.0 200 OK\r\n\r\nok", 200, "ok");
  EXPECT_RESPONSE("HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\nokEXTRA", 200, "ok");
  EXPECT_RESPONSE("HTTP/1.0 200 OK\r\nX-Transfer-Encoding: chunked\r\n\r\nok", 200, "ok");
  EXPECT_RESPONSE("HTTP/1.0 200 OK\r\nTransfer-Encoding: ,\r\n\r\nok", 200, "ok");
  EXPECT_RESPONSE("HTTP/1.0 100 Continue\r\nTransfer-Encoding: gzip\r\n\r\nHTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nok\r\n0\r\n\r\n", 200, "ok");
  http_close_client(shared_origin_client);
}

// A whole response of `total` bytes: a head, then filler, framed as `head_format`
// says with the size of the rest.
static char *filled_response(const char *head, size_t total, char filler, size_t *head_len) {
  char *reply = malloc(total + 1);
  assert(reply != NULL);
  *head_len = strlen(head);
  memcpy(reply, head, *head_len);
  memset(reply + *head_len, filler, total - *head_len);
  reply[total] = '\0';
  return reply;
}

// One byte past the limit is refused, and exactly the limit is not, however the
// response is framed, and however far the server would have gone on.
static void check_size_limit(void) {
  use_client_with_timeout(20000);
  origin_holds_connections = false;
  size_t head_len = 0;
  static const char plain[] = "HTTP/1.1 200 OK\r\n\r\n";
  char *at_limit = filled_response(plain, HTTP_MAX_RESPONSE_BYTES, 'x', &head_len);
  int64_t handle = origin_exchange(at_limit, HTTP_MAX_RESPONSE_BYTES);
  assert(handle >= 1);
  char *body = http_response_body(handle);
  assert(body != NULL && strlen(body) == HTTP_MAX_RESPONSE_BYTES - head_len);
  free(body);
  assert(http_response_free(handle) == 0);
  free(at_limit);

  static const char *const heads[] = {
      plain,
      "HTTP/1.1 200 OK\r\nContent-Length: 999999999\r\n\r\n",
      "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n7FFFFFFF\r\n",
      "HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\n\r\n",
      "HTTP/1.1 200 OK\r\nX-Endless: ",
      "",
  };
  size_t over = HTTP_MAX_RESPONSE_BYTES + 1;
  for (size_t i = 0; i < sizeof(heads) / sizeof(heads[0]); i++) {
    char *reply = filled_response(heads[i], over, 'x', &head_len);
    expect_failure_n(reply, over, -15, OVER_SIZE);
    free(reply);
  }
  http_close_client(shared_origin_client);
}

static void expect_cut_off(const char *reply, int64_t timeout_ms) {
  long live = test_alloc_live();
  uint64_t started = osp_monotonic_ms();
  osp_io_error_clear();
  int64_t code = origin_exchange(reply, strlen(reply));
  assert(code == -16);
  int64_t took = elapsed_ms_since(started);
  assert(took >= timeout_ms * HTTP_READ_BOUND_FACTOR - 100);
  assert(took < timeout_ms * (HTTP_READ_BOUND_FACTOR + 5));
  assert(strcmp(osp_io_error(), OVER_TIME) == 0);
  assert(test_alloc_live() == live);
  osp_io_error_clear();
}

// A server sending just often enough to beat the timeout is cut off at the bound,
// wherever in the response it is dripping. Within the bound, it's only a slow server.
static void check_whole_read_bound(void) {
  enum { TIMEOUT_MS = 100, DRIP_MS = 40 };
  use_client_with_timeout(TIMEOUT_MS);
  origin_holds_connections = false;
  origin_drip_us = DRIP_MS * 1000;
  char filler[201];
  memset(filler, 'x', sizeof(filler) - 1);
  filler[sizeof(filler) - 1] = '\0';
  char reply[512];
  static const char *const heads[] = {
      "X-Long: ", // still in the head
      "\r\n\r\n", // a body only the close can end
      "Content-Length: 999\r\n\r\n",
      "Transfer-Encoding: chunked\r\n\r\n999\r\n",
      "Transfer-Encoding: chunked\r\n\r\n1;",
      "Transfer-Encoding: chunked\r\n\r\n0\r\nX-T: ",
  };
  for (size_t i = 0; i < sizeof(heads) / sizeof(heads[0]); i++) {
    snprintf(reply, sizeof(reply), "HTTP/1.1 200 OK\r\n%s%s", heads[i], filler);
    expect_cut_off(reply, TIMEOUT_MS);
  }
  static const char slow[] = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok"; // 1.6 s of 1 s
  http_close_client(shared_origin_client);
  use_client_with_timeout(200);
  EXPECT_RESPONSE(slow, 200, "ok");
  // Falling silent near the end is still over the limit, at the limit: the last wait
  // is the time left, not a whole timeout, which would call it a timed-out read.
  origin_drip_us = 150000;
  origin_holds_connections = true;
  http_close_client(shared_origin_client);
  use_client_with_timeout(200);
  expect_cut_off("HTTP/1.1 200 O", 200); // the last of these is sent at 1.95 s of 2 s
  origin_holds_connections = false;
  origin_drip_us = 0;
  http_close_client(shared_origin_client);
}

// A timeout of 0 is no limit on a single read, and framing ends the read as ever.
static void check_no_timeout(void) {
  use_client_with_timeout(0);
  origin_holds_connections = true;
  EXPECT_PROMPT("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok", 200, "ok");
  origin_holds_connections = false;
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\n\r\nclosed", 200, "closed");
  http_close_client(shared_origin_client);
}

// A timeout too big for the transport is its largest, not whatever its low bits say:
// these would otherwise be 1 ms, far less than this reply's delay.
static void check_huge_timeout(void) {
  ScriptedOrigin origin;
  origin_start(&origin, "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
  origin.reply_delay_us = 100000;
  int64_t client = http_create_client((char *)(uintptr_t) "http://127.0.0.1:18088",
                                      ((int64_t)1 << 32) + 1);
  assert(client > 0);
  assert(http_get(client, (char *)(uintptr_t) "/", (char *)(uintptr_t) "") == 200);
  origin_stop(&origin);
  http_close_client(client);
}

// The status-only builtins read through the same bounds.
static void check_status_only_builtins(void) {
  ScriptedOrigin origin;
  origin_start(&origin, "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
  origin.hold_open = true;
  int64_t client = http_create_client((char *)(uintptr_t) "http://127.0.0.1:18088", 20000);
  uint64_t started = osp_monotonic_ms();
  assert(http_get(client, (char *)(uintptr_t) "/", (char *)(uintptr_t) "") == 200);
  assert(elapsed_ms_since(started) < 1500);
  origin_stop(&origin);
  http_close_client(client);
}

// The bytes as they would be seen a read at a time, `step` bytes to a read. Returns
// whether the framing ended, and where, in a block of exactly the size the transport
// would have, so a sanitizer build traps any read past the end.
static bool scan_in_steps(const char *bytes, size_t len, size_t step, size_t *end) {
  char *raw = malloc(len + 1);
  assert(raw != NULL);
  HttpFrameScan scan;
  memset(&scan, 0, sizeof(scan));
  bool done = false;
  for (size_t have = 0; have < len && !done;) {
    size_t next = len - have < step ? len : have + step;
    memcpy(raw + have, bytes + have, next - have);
    raw[next] = '\0';
    have = next;
    done = http_frame_complete(&scan, raw, have, end);
    assert(!done || *end <= have);
  }
  free(raw);
  return done;
}

// However a response is cut up the scan reaches one answer, and it never stops at a
// point the decoder would call truncated: stopping there would refuse a good response.
void expect_scan_agrees(const char *bytes, size_t len) {
  size_t whole_end = 0;
  size_t byte_end = 0;
  bool whole = scan_in_steps(bytes, len, len + 1, &whole_end);
  bool by_byte = scan_in_steps(bytes, len, 1, &byte_end);
  assert(whole == by_byte);
  if (!whole) {
    return;
  }
  assert(whole_end == byte_end || whole_end == len); // a fault ends the scan where it stands
  char *raw = malloc(byte_end + 1);
  assert(raw != NULL);
  memcpy(raw, bytes, byte_end);
  raw[byte_end] = '\0';
  const char *sep = NULL;
  int64_t status = 0;
  int64_t rc = 0;
  osp_io_error_clear();
  const char *head = http_checked_head(raw, byte_end, &sep, &status, &rc);
  if (head) {
    char *headers = NULL;
    assert(http_copy_response_headers(head, sep, &headers));
    char *body = NULL;
    rc = http_response_body_of(status, headers, sep + 4, byte_end - (size_t)(sep + 4 - raw), false, &body);
    free(body);
    free(headers);
  }
  assert(osp_io_error() == NULL || strstr(osp_io_error(), "truncated") == NULL);
  osp_io_error_clear();
  free(raw);
}

static void expect_frame_end(const char *bytes, size_t len, size_t expected) {
  for (size_t step = 1; step <= len + 1; step = step < 8 ? step + 1 : step * 2) {
    size_t end = 0;
    assert(scan_in_steps(bytes, len, step, &end));
    assert(end == expected);
  }
  size_t end = 0;
  assert(!scan_in_steps(bytes, expected - 1, 1, &end)); // one byte short is not the end
}

// Exactly where each kind of framing ends, past which nothing is read.
static void check_frame_ends(void) {
  static const struct { const char *frame; const char *past; } framed[] = {
      {"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok", "EXTRA"},
      {"HTTP/1.1 200 OK\r\nContent-Length: 00\r\n\r\n", "EXTRA"},
      {"HTTP/1.0 200 OK\r\nContent-Length:\t2 \r\n\r\nok", "\r\n\r\n"},
      {"HTTP/1.1 204 No\r\n\r\n", "EXTRA"},
      {"HTTP/1.1 304 No\r\nContent-Length: 9\r\n\r\n", "EXTRA"},
      {"HTTP/1.1 101 Up\r\nUpgrade: x\r\n\r\n", "not http"},
      {"HTTP/1.1 100 Go\r\n\r\nHTTP/1.1 102 Go\r\nA: b\r\n\r\nHTTP/1.1 204 No\r\n\r\n", "X"},
      {"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n", "0\r\n\r\n"},
      {"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1\r\n\r\r\n2\r\n\r\n\r\n0\r\n\r\n", "X"},
      {"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nA;x=y\r\n0123456789\r\n0;z\r\nA: b\r\nC: d\r\n\r\n", "X"},
      {"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\nTransfer-Encoding: chunked\r\nContent-Length: 1\r\n\r\n0\r\n\r\n", "X"},
  };
  char joined[256];
  for (size_t i = 0; i < sizeof(framed) / sizeof(framed[0]); i++) {
    size_t frame_len = strlen(framed[i].frame);
    int total = snprintf(joined, sizeof(joined), "%s%s", framed[i].frame, framed[i].past);
    assert(total > 0 && (size_t)total < sizeof(joined));
    expect_frame_end(joined, (size_t)total, frame_len);
    expect_frame_end(framed[i].frame, frame_len, frame_len);
  }
  static const char *const never[] = {
      "HTTP/1.1 200 OK\r\n\r\nonly the close ends this",
      "HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\nContent-Length: 1\r\n\r\nnor this",
      "HTTP/1.1 200 OK\r\nContent-Length: 18446744073709551615\r\n\r\ntoo long to arrive",
      "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nFFFFFFFFFFFFFFFF\r\nmore",
      "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\nA: unfinished trailer\r\n",
      "HTTP/1.1 200 OK\r\nno blank line yet\r\n",
      "HTTP/1.1 100 Go\r\n\r\n",
  };
  for (size_t i = 0; i < sizeof(never) / sizeof(never[0]); i++) {
    size_t end = 0;
    assert(!scan_in_steps(never[i], strlen(never[i]), 1, &end));
    assert(!scan_in_steps(never[i], strlen(never[i]), strlen(never[i]) + 1, &end));
  }
}

// Fills a limit's worth of `unit`, read in transport-sized steps. A scan that went
// back to the start after each read would take minutes over this, and this is what a
// hostile server would send to burn the client's CPU instead of its memory.
static void expect_linear_scan(const char *head, const char *unit, const char *tail) {
  size_t size = HTTP_MAX_RESPONSE_BYTES;
  char *bytes = malloc(size + 1);
  assert(bytes != NULL);
  size_t head_len = strlen(head);
  size_t unit_len = strlen(unit);
  size_t tail_len = strlen(tail);
  memcpy(bytes, head, head_len);
  size_t at = head_len;
  while (at + unit_len + tail_len < size) {
    memcpy(bytes + at, unit, unit_len);
    at += unit_len;
  }
  memcpy(bytes + at, tail, tail_len);
  at += tail_len;
  size_t end = 0;
  uint64_t started = osp_monotonic_ms();
  (void)scan_in_steps(bytes, at, CHUNK_SIZE, &end);
  assert(elapsed_ms_since(started) < 20000);
  free(bytes);
}

static void check_scan_costs_stay_linear(void) {
  static const char chunked[] = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
  expect_linear_scan(chunked, "1\r\nx\r\n", "0\r\n\r\n");
  expect_linear_scan("HTTP/1.1 200 OK\r\nX-Long: ", "x", "\r\n\r\n");
  expect_linear_scan("HTTP/1.1 200 OK\r\n", "X: y\r\n", "\r\n");
  expect_linear_scan("", "HTTP/1.1 100 Go\r\n\r\n", "HTTP/1.1 204 No\r\n\r\n");
  expect_linear_scan(chunked, "1;", "\r\nx\r\n0\r\n\r\n");
  expect_linear_scan(chunked, "0", "\r\n\r\n");
  expect_linear_scan("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n", "A: b\r\n", "\r\n");
  expect_linear_scan("HTTP/1.1 200 OK\r\n", "\r", "\n\r\n");
}

void test_response_read_bounds(void) {
  int64_t previous = shared_origin_client;
  check_frame_ends();
  check_scan_costs_stay_linear();
  check_stops_once_framed();
  check_stops_at_the_fault();
  check_what_framing_cannot_end();
  check_transfer_encoding_on_http_1_0();
  check_size_limit();
  check_whole_read_bound();
  check_no_timeout();
  check_huge_timeout();
  check_status_only_builtins();
  origin_holds_connections = false;
  osp_io_error_clear();
  shared_origin_client = previous;
}
