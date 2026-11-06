// Whole-response framing against a scripted origin, split from
// http_origin_tests.c so each test TU stays under the repo size budget: what a
// response's status, repeated headers and NUL bytes do to the body the caller
// sees, and chunked bodies far past the transport's buffer [HTTP-RESPONSE-FRAMING],
// [HTTP-CHUNKED-DECODE].
#include "http_client_internal.h"
#include "http_runtime_tests.h"
#include "http_shared.h"
#include "io_error.h"

#include <assert.h>
#include <signal.h>
#include <stdio.h>
#include <unistd.h>

static const char TOO_BIG_REFUSED[] =
    "http response: chunked body: chunk size exceeds the remaining body";
static const char HEAD_CUT[] = "http response: head: truncated";
static const char HEAD_BAD[] = "http response: head: malformed";
static const char BODY_CUT[] = "http response: body: truncated";
static const char LENGTH_BAD[] = "http response: body: invalid Content-Length";

// 204 and 304 end at the blank line whatever their headers say (RFC 9112 6.3).
static void check_statuses_without_a_body(void) {
  EXPECT_RESPONSE("HTTP/1.1 304 Not Modified\r\nTransfer-Encoding: chunked\r\n\r\n", 304, "");
  EXPECT_RESPONSE("HTTP/1.1 204 No Content\r\nTransfer-Encoding: chunked\r\n\r\n", 204, "");
  EXPECT_RESPONSE("HTTP/1.1 304 Not Modified\r\n\r\n", 304, "");
  EXPECT_RESPONSE("HTTP/1.1 204 No Content\r\n\r\n", 204, "");
  EXPECT_RESPONSE("HTTP/1.1 304 Not Modified\r\nETag: \"a\"\r\nContent-Length: 5\r\n\r\n", 304, "");
  EXPECT_RESPONSE("HTTP/1.1 304 Not Modified\r\nTransfer-Encoding: chunked\r\n\r\n7FFFFFFF\r\nhi\r\n", 304, "");
  EXPECT_RESPONSE("HTTP/1.1 204 No Content\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n", 204, "");
  EXPECT_RESPONSE("HTTP/1.1 204 No Content\r\n\r\nstray", 204, "");
  // A body is still required of every neighbouring status.
  expect_refused_n("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n", 47,
                   "http response: chunked body: truncated");
  expect_refused_n("HTTP/1.1 203 OK\r\nTransfer-Encoding: chunked\r\n\r\n", 47,
                   "http response: chunked body: truncated");
  expect_refused_n("HTTP/1.1 305 OK\r\nTransfer-Encoding: chunked\r\n\r\n", 47,
                   "http response: chunked body: truncated");
  EXPECT_RESPONSE("HTTP/1.1 500 Oops\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nno\r\n0\r\n\r\n", 500, "no");
}

// [PORT-STRING-NUL-HTTP] Payload bytes survive every framing mode; NUL in
// metadata is still a typed error, including interim headers.
static void check_nul_bytes(void) {
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhe\0lo\r\n0\r\n\r\n", 200, "he\0lo");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1\r\n\0\r\n0\r\n\r\n", 200, "\0");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\na\0\r\n3\r\nb\0c\r\n0\r\n\r\n", 200, "a\0b\0c");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhe\0lo", 200, "he\0lo");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\n\r\n\0", 200, "\0");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\n\r\nhello\0", 200, "hello\0");
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\nX\0: 1\r\n\r\n",
                 "http response: chunked body: invalid trailer");
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\0\r\nhello\r\n0\r\n\r\n",
                 "http response: chunked body: invalid chunk size");
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nX-A: \0\r\nTransfer-Encoding: chunked\r\n\r\n7FFFFFFF\r\nhi\r\n", "http response: head: contains NUL");
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\n\0\r\n\r\nhello", "http response: head: contains NUL");
  EXPECT_REFUSED("HTTP/1.1 2\0 OK\r\n\r\nhello", "http response: head: contains NUL");
  EXPECT_REFUSED("\0HTTP/1.1 200 OK\r\n\r\nhello", "http response: head: contains NUL");
  EXPECT_REFUSED("HTTP/1.1 100 Continue\r\nX-A: \0\r\n\r\nHTTP/1.1 200 OK\r\n\r\nok", "http response: head: contains NUL");
  // Past the end of the framing, it isn't part of the response.
  EXPECT_RESPONSE("HTTP/1.1 304 Not Modified\r\n\r\n\0", 304, "");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok\0", 200, "ok");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n\0", 200, "");
}

// Repeated Transfer-Encoding lines are one list, so the last line's last coding
// decides, wherever other headers fall between them.
static void check_repeated_transfer_encoding(void) {
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n", 200, "hello");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\nX-A: 1\r\ntransfer-encoding: CHUNKED\r\n\r\n5\r\nhello\r\n0\r\n\r\n", 200, "hello");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nTransfer-Encoding: gzip\r\n\r\n2\r\nhi\r\n", 200, "2\r\nhi\r\n");
  static const char lying[] =
      "HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\nTransfer-Encoding: chunked\r\n\r\n7FFFFFFF\r\nhi\r\n";
  expect_refused_n(lying, sizeof(lying) - 1, TOO_BIG_REFUSED);
}

// A list may hold empty elements (RFC 9110 5.6.1): they name no coding, so they
// can't hide chunked as the last one, nor a later empty line undo it.
static void check_empty_list_elements(void) {
  static const char *const chunked[] = {
      "chunked,", "chunked, ", "chunked ,\t", ",chunked", " , chunked , ,",
      "gzip,,chunked,,", "chunked\r\nTransfer-Encoding: ,",
      "chunked\r\nTransfer-Encoding:", ",\r\nTransfer-Encoding: chunked,",
  };
  static const char *const plain[] = {"notchunked,", "chunked, gzip,", ",gzip", "chunked\r\nTransfer-Encoding: gzip,"};
  char reply[200];
  for (size_t i = 0; i < sizeof(chunked) / sizeof(chunked[0]); i++) {
    int len = snprintf(reply, sizeof(reply), "HTTP/1.1 200 OK\r\nTransfer-Encoding: %s\r\n\r\n5\r\nhello\r\n0\r\n\r\n", chunked[i]);
    expect_response_n(reply, (size_t)len, 200, "hello");
    len = snprintf(reply, sizeof(reply), "HTTP/1.1 200 OK\r\nTransfer-Encoding: %s\r\n\r\n7FFFFFFF\r\nhi\r\n", chunked[i]);
    expect_refused_n(reply, (size_t)len, TOO_BIG_REFUSED);
  }
  for (size_t i = 0; i < sizeof(plain) / sizeof(plain[0]); i++) {
    int len = snprintf(reply, sizeof(reply), "HTTP/1.1 200 OK\r\nTransfer-Encoding: %s\r\n\r\n2\r\nhi\r\n", plain[i]);
    expect_response_n(reply, (size_t)len, 200, "2\r\nhi\r\n");
  }
}

// Transfer-Encoding overrides Content-Length, whatever its coding (RFC 9112 6.3);
// an empty one names no coding, so the length still stands.
static void check_coding_overrides_length(void) {
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\nContent-Length: 2\r\n\r\nhello", 200, "hello");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nContent-Length: 99\r\nTransfer-Encoding: gzip\r\n\r\nhello", 200, "hello");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\nContent-Length: abc\r\n\r\nhello", 200, "hello");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 1\r\n\r\n5\r\nhello\r\n0\r\n\r\n", 200, "hello");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nTransfer-Encoding: ,\r\nContent-Length: 2\r\n\r\nhello", 200, "he");
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nTransfer-Encoding:\r\nContent-Length: 9\r\n\r\nhello", BODY_CUT);
}

// A skipped interim head is held to the same rules as the final one. Otherwise its
// blank line goes unnoticed, and the final head's is taken for it, making the body
// the response.
static void check_interim_heads_are_checked(void) {
  EXPECT_REFUSED("HTTP/1.1 100 X\n\nHTTP/1.1 200 OK\r\n\r\nHTTP/1.1 404 Forged\r\n\r\nno", HEAD_BAD);
  EXPECT_REFUSED("HTTP/1.1 100 X\r\nno-colon\r\n\r\nHTTP/1.1 200 OK\r\n\r\nok", HEAD_BAD);
  EXPECT_REFUSED("HTTP/1.1 103 X\r\nLink: a\r\n b: c\r\n\r\nHTTP/1.1 200 OK\r\n\r\nok", HEAD_BAD);
  EXPECT_RESPONSE("HTTP/1.1 103 X\r\nLink: a\r\n\r\nHTTP/1.1 200 OK\r\n\r\nok", 200, "ok");
}

// A read that fails or times out is not the close that ends an unframed body:
// reporting what arrived so far as the whole body is how a cut goes unnoticed.
// Framing that is complete needs no close, and framing that isn't stays refused.
static void check_reads_that_never_saw_the_close(void) {
  int64_t patient = shared_origin_client;
  shared_origin_client = http_create_client((char *)(uintptr_t) "http://127.0.0.1:18088", 150);
  assert(shared_origin_client > 0);
  origin_holds_connections = true;
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\n\r\nhalf a bo", BODY_CUT);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\n\r\nhalf a bo", BODY_CUT);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nhalf", BODY_CUT);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nContent-Le", HEAD_CUT);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhel", TOO_BIG_REFUSED);
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nhalf", 200, "half");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nok\r\n0\r\n\r\n", 200, "ok");
  EXPECT_RESPONSE("HTTP/1.1 204 No Content\r\n\r\n", 204, "");
  EXPECT_RESPONSE("HTTP/1.1 304 Not Modified\r\n\r\n", 304, "");
  origin_holds_connections = false;
  http_close_client(shared_origin_client);
  shared_origin_client = patient;
}

static void ignore_signal(int signo) {
  (void)signo;
}

static void *interrupt_soon(void *target) {
  usleep(50000);
  assert(pthread_kill(*(pthread_t *)target, SIGUSR1) == 0);
  return NULL;
}

// One exchange with a signal landing while the client waits for the reply.
static int64_t exchange_interrupted(void) {
  pthread_t self = pthread_self();
  pthread_t interrupter;
  assert(pthread_create(&interrupter, NULL, interrupt_soon, &self) == 0);
  ScriptedOrigin origin;
  origin_start(&origin, "HTTP/1.1 200 OK\r\n\r\nok");
  origin.reply_delay_us = 200000; // read only once the request arrives
  int64_t handle = http_get_response(shared_origin_client, (char *)(uintptr_t) "/f",
                                     (char *)(uintptr_t) "");
  origin_stop(&origin);
  assert(pthread_join(interrupter, NULL) == 0);
  return handle;
}

// A signal landing mid-read ends neither the read nor the response.
static void check_interrupted_read(void) {
  struct sigaction action;
  struct sigaction previous;
  memset(&action, 0, sizeof(action));
  action.sa_handler = ignore_signal; // no SA_RESTART: recv fails with EINTR
  assert(sigaction(SIGUSR1, &action, &previous) == 0);
  int64_t handle = exchange_interrupted();
  assert(sigaction(SIGUSR1, &previous, NULL) == 0);
  assert(handle >= 1);
  char *got = http_response_body(handle);
  assert(got != NULL && strcmp(got, "ok") == 0);
  osp_release(got);
  assert(http_response_free(handle) == 0);
}

// Running out of memory copying the headers is -9, never a chunked body handed
// back raw because no headers were left to name its framing. The padding gives
// the header block a size no other allocation in the exchange asks for.
static void check_header_allocation_failure(void) {
  char head[600];
  char pad[401];
  memset(pad, 'p', sizeof(pad) - 1);
  pad[sizeof(pad) - 1] = '\0';
  int block = snprintf(head, sizeof(head), "X-Pad: %s\r\nTransfer-Encoding: chunked", pad);
  char reply[700];
  int len = snprintf(reply, sizeof(reply), "HTTP/1.1 200 OK\r\n%s\r\n\r\n2\r\nhi\r\n0\r\n\r\n", head);
  long live = test_alloc_live();
  test_alloc_fail_size((size_t)block + 1);
  assert(origin_exchange(reply, (size_t)len) == -9);
  test_alloc_fail_off();
  assert(test_alloc_live() == live);
  int64_t handle = origin_exchange(reply, (size_t)len);
  assert(handle >= 1);
  char *body = http_response_body(handle);
  assert(body != NULL && strcmp(body, "hi") == 0);
  osp_release(body);
  assert(http_response_free(handle) == 0);
}

static size_t put_chunks(char *out, const char *payload, size_t payload_len,
                         size_t step) {
  size_t at = (size_t)sprintf(out, "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n");
  for (size_t done = 0; done < payload_len; done += step) {
    size_t take = payload_len - done < step ? payload_len - done : step;
    at += (size_t)sprintf(out + at, "%zx\r\n", take);
    memcpy(out + at, payload + done, take);
    at += take;
    memcpy(out + at, "\r\n", 2);
    at += 2;
  }
  return at;
}

// Bodies many reads long reach the decoder whole, in one chunk or thousands, and
// a lie told only at the very end is still caught.
static void check_bodies_past_the_transport_buffer(void) {
  size_t payload_len = MAX_HTTP_BUFFER * 8 + 123;
  char *payload = malloc(payload_len + 1);
  char *reply = malloc(payload_len * 7 + 256);
  assert(payload != NULL && reply != NULL);
  for (size_t i = 0; i < payload_len; i++) {
    payload[i] = (char)('a' + i % 26);
  }
  payload[payload_len] = '\0';
  static const size_t steps[] = {1, 7, 4096, MAX_HTTP_BUFFER, MAX_HTTP_BUFFER + 1, 1 << 30};
  for (size_t s = 0; s < sizeof(steps) / sizeof(steps[0]); s++) {
    size_t at = put_chunks(reply, payload, payload_len, steps[s]);
    memcpy(reply + at, "0\r\n\r\n", 5);
    expect_response_n(reply, at + 5, 200, payload);
    memcpy(reply + at, "7FFFFFFF\r\nhi\r\n", 14);
    expect_refused_n(reply, at + 14, TOO_BIG_REFUSED);
  }
  free(reply);
  free(payload);
}

// A status is exactly three digits, so a hostile one can't overflow, or pose as
// one of this API's own negative codes.
static void check_status_lines(void) {
  static const char not_http[] = "http response: status line: not HTTP/1.x";
  static const char *const bad[] = {
      "HTTP/1.1 -14 X\r\n\r\nb",       "HTTP/1.1 99999999999999999999 X\r\n\r\nb",
      "HTTP/1.1 20 OK\r\n\r\nb",        "HTTP/1.1 2000 OK\r\n\r\nb",
      "HTTP/1.1 304abc\r\n\r\nb",       "HTTP/1.1 \r\n\r\nb",
      "HTTP/1.1 OK\r\n\r\nb",           "HTTP/1.1 +20 OK\r\n\r\nb",
      "HTTP/1.1  200 OK\r\n\r\nb",      "HTTP/1.1 020 OK\r\n\r\nb",
      "HTTP/1.1 2x0 OK\r\n\r\nb",       "HTTP/2 200\r\n\r\nb",
      "HTTP/1.1\r\n\r\nb",              "NOT-HTTP\r\n\r\nb",
      "http/1.1 200 OK\r\n\r\nb",
      "NOT-HTTP\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nhi\r\n0\r\n\r\n",
      "HTTP/1.1 100 Continue\r\n\r\n",  "HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 -5 X\r\n\r\nb",
  };
  for (size_t i = 0; i < sizeof(bad) / sizeof(bad[0]); i++) {
    expect_failure_n(bad[i], strlen(bad[i]), -8, not_http);
  }
  EXPECT_RESPONSE("HTTP/1.1 200\r\n\r\nb", 200, "b");
  EXPECT_RESPONSE("HTTP/1.1 200 \r\n\r\nb", 200, "b");
  EXPECT_RESPONSE("HTTP/1.0 100 x\r\n\r\nHTTP/1.0 599 y\r\n\r\nb", 599, "b");
  EXPECT_RESPONSE("HTTP/1.1 999 z\r\n\r\nb", 999, "b");
}

// Interim responses aren't the answer: the final response's status, headers and
// framing are what the caller gets. 101 is final, and has no body.
static void check_interim_responses(void) {
  EXPECT_RESPONSE("HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhi", 200, "hi");
  EXPECT_RESPONSE("HTTP/1.1 103 Hints\r\nLink: </a>\r\n\r\nHTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nhi\r\n0\r\n\r\n", 200, "hi");
  EXPECT_RESPONSE("HTTP/1.1 100 a\r\n\r\nHTTP/1.1 102 b\r\n\r\nHTTP/1.1 103 c\r\n\r\nHTTP/1.1 404 d\r\n\r\nno", 404, "no");
  EXPECT_RESPONSE("HTTP/1.1 100 a\r\n\r\nHTTP/1.1 304 d\r\nTransfer-Encoding: chunked\r\n\r\n", 304, "");
  EXPECT_RESPONSE("HTTP/1.1 101 Switching\r\nUpgrade: x\r\n\r\nraw bytes", 101, "");
  // An interim response's headers say nothing about the final one's framing.
  EXPECT_RESPONSE("HTTP/1.1 100 a\r\nTransfer-Encoding: chunked\r\n\r\nHTTP/1.1 200 OK\r\n\r\n7FFFFFFF", 200, "7FFFFFFF");
  EXPECT_REFUSED("HTTP/1.1 100 a\r\n\r\nHTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n7FFFFFFF\r\nhi\r\n", TOO_BIG_REFUSED);
  EXPECT_REFUSED("HTTP/1.1 100 a\r\n\r\nHTTP/1.1 200 OK\r\nX: y", HEAD_CUT);
  // The status-only API reports the final status too.
  static const char interim[] = "HTTP/1.1 100 a\r\n\r\nHTTP/1.1 201 b\r\n\r\n";
  ScriptedOrigin origin;
  origin_start(&origin, interim);
  assert(http_get(shared_origin_client, (char *)(uintptr_t) "/s", (char *)(uintptr_t) "") == 201);
  origin_stop(&origin);
}

// A head that never reaches its blank line, or that hides framing from the header
// scan, is refused: its bytes must not come back as a body.
static void check_heads(void) {
  EXPECT_REFUSED("HTTP/1.1 200 OK", HEAD_CUT);
  EXPECT_REFUSED("HTTP/1.1 200", HEAD_CUT);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\n", HEAD_CUT);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n", HEAD_CUT);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nX: y\r\n\r", HEAD_CUT);
  EXPECT_REFUSED("HTTP/1.1 200 OK\nTransfer-Encoding: chunked\n\n7FFFFFFF\nhi\n", HEAD_CUT);
  EXPECT_REFUSED("HTTP/1.1 200 O\nK\r\n\r\nb", HEAD_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nX: a\nTransfer-Encoding: chunked\r\n\r\n7FFFFFFF\r\nhi\r\n", HEAD_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nX: a\rb\r\n\r\nb", HEAD_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nX: a\x01\r\n\r\nb", HEAD_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nTransfer-Encoding : chunked\r\n\r\n7FFFFFFF\r\nhi\r\n", HEAD_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nTransfer-Encoding\t: chunked\r\n\r\nb", HEAD_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip,\r\n chunked\r\n\r\n2\r\nhi\r\n0\r\n\r\n", HEAD_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nX: a\r\n\ty\r\n\r\nb", HEAD_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nno colon\r\n\r\nb", HEAD_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nZ\r\n\r\nb", HEAD_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\n: no name\r\n\r\nb", HEAD_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nX: y\r\nZ\r\n\r\nb", HEAD_BAD);
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\n\r\n", 200, "");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nX:\r\nY:\ta b:c\r\n\r\nb", 200, "b");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nX-Caf\xc3\xa9: \xc3\xa9\r\n\r\nb", 200, "b");
}

// A declared length is checked: fewer bytes than it promises is a body cut short,
// and more is not part of this response. Chunked framing and bodiless statuses
// take no notice of it.
static void check_content_length(void) {
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello", 200, "hello");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\ncontent-length:5  \r\n\r\nhello", 200, "hello");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n", 200, "");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nhello", 200, "he");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nCONTENT-LENGTH: 2\r\n\r\nhello", 200, "he");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nContent-Length: 0005\r\n\r\nhello", 200, "hello");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhello", 200, "he");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\nhello", 200, "");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Length: 5\r\n\r\nhello", 200, "hello");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nContent-Length: 3\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n", 200, "hello");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nContent-Length: x\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nhi\r\n0\r\n\r\n", 200, "hi");
  EXPECT_RESPONSE("HTTP/1.1 304 Not Modified\r\nContent-Length: 99\r\n\r\n", 304, "");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nX-Content-Length: 99\r\n\r\nhello", 200, "hello");
  EXPECT_RESPONSE("HTTP/1.1 200 OK\r\nContent-Lengths: 99\r\n\r\nhello", 200, "hello");
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nshort", BODY_CUT);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\n", BODY_CUT);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nCONTENT-LENGTH: 100\r\n\r\nshort", BODY_CUT);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nContent-Length: 18446744073709551615\r\n\r\nshort", BODY_CUT);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nContent-Length: 18446744073709551616\r\n\r\nshort", LENGTH_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nContent-Length: 99999999999999999999999\r\n\r\nshort", LENGTH_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nContent-Length: -1\r\n\r\nshort", LENGTH_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nContent-Length: +5\r\n\r\nhello", LENGTH_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nContent-Length: 0x5\r\n\r\nhello", LENGTH_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nContent-Length: 5 5\r\n\r\nhello", LENGTH_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nContent-Length: 5, 5\r\n\r\nhello", LENGTH_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nContent-Length:\r\n\r\nhello", LENGTH_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Length: 2\r\n\r\nhello", LENGTH_BAD);
  EXPECT_REFUSED("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Length: x\r\n\r\nhello", LENGTH_BAD);
}

// One damaged response through the parsing a real exchange uses, in a heap block
// of exactly its length plus the terminator the transport adds, so a sanitizer
// build traps any read past the end. It may be refused or accepted, never anything
// else, and whatever is accepted obeys the rules callers rely on.
static void parse_damaged(const char *bytes, size_t len) {
  expect_scan_agrees(bytes, len);
  char *raw = malloc(len + 1);
  assert(raw != NULL);
  memcpy(raw, bytes, len);
  raw[len] = '\0';
  const char *sep = NULL;
  int64_t status = 0;
  int64_t rc = 0;
  const char *head = http_checked_head(raw, len, &sep, &status, &rc);
  if (!head) {
    assert(rc == -8 || rc == -14);
    osp_io_error_clear();
    free(raw);
    return;
  }
  assert(status >= 100 && status <= 999 && memchr(raw, '\0', (size_t)(sep - raw)) == NULL);
  char *headers = NULL;
  assert(http_copy_response_headers(head, sep, &headers));
  HttpBody body;
  const char *start = sep + 4;
  rc = http_response_body_of(status, headers, start, len - (size_t)(start - raw), true, &body);
  if (rc == 0) {
    assert(body.data != NULL && body.length <= len && body.data[body.length] == '\0');
    free(body.data);
  } else {
    assert(rc == -14 && body.data == NULL);
    osp_io_error_clear();
  }
  free(headers);
  free(raw);
}

// Every byte value at every position, every deletion, every insertion and every
// truncation of responses that between them use all the framing there is.
static void check_every_response_mutation(void) {
  static const char *const responses[] = {
      "HTTP/1.1 200 OK\r\nX-A: 1\r\nTransfer-Encoding: gzip, chunked\r\n\r\n"
      "5;a=b\r\nhello\r\n1A\r\nabcdefghijklmnopqrstuvwxyz\r\n0;end\r\nX-T: 1\r\n\r\n",
      "HTTP/1.0 200 OK\r\nContent-Length: 5\r\nX-B: 2\r\n\r\nhello",
      "HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 103 Hints\r\nLink: </a>\r\n\r\n"
      "HTTP/1.1 304 Not Modified\r\nTransfer-Encoding: chunked\r\n\r\n",
  };
  char mutated[256];
  for (size_t r = 0; r < sizeof(responses) / sizeof(responses[0]); r++) {
    const char *good = responses[r];
    size_t len = strlen(good);
    assert(len + 1 < sizeof(mutated));
    for (size_t at = 0; at < len; at++) {
      parse_damaged(good, at);
      memcpy(mutated, good, at);
      memcpy(mutated + at, good + at + 1, len - at - 1);
      parse_damaged(mutated, len - 1);
      for (int byte = 0; byte < 256; byte++) {
        memcpy(mutated, good, len);
        mutated[at] = (char)byte;
        parse_damaged(mutated, len);
        memcpy(mutated, good, at);
        mutated[at] = (char)byte;
        memcpy(mutated + at + 1, good + at, len - at);
        parse_damaged(mutated, len + 1);
      }
    }
    parse_damaged(good, len);
  }
}

void test_response_framing(void) {
  shared_origin_client = origin_client();
  check_every_response_mutation();
  check_status_lines();
  check_interim_responses();
  check_heads();
  check_content_length();
  check_statuses_without_a_body();
  check_nul_bytes();
  check_repeated_transfer_encoding();
  check_empty_list_elements();
  check_coding_overrides_length();
  check_interim_heads_are_checked();
  check_reads_that_never_saw_the_close();
  check_interrupted_read();
  check_header_allocation_failure();
  check_bodies_past_the_transport_buffer();
  http_close_client(shared_origin_client);
}
