// Hostile-input sweeps of the chunked decoder, split from http_origin_tests.c so
// each test TU stays under the repo size budget. The socket-level cases live
// there; these reach inputs a sweep needs in the thousands, and bytes a C string
// reply cannot carry. Every input is copied into a heap block of EXACTLY its
// length with no terminator, so a sanitizer build traps any read past the end
// [HTTP-CHUNKED-DECODE].
#include "http_client_internal.h"
#include "http_runtime_tests.h"

#include <assert.h>
#include <stdio.h>

static const char TOO_BIG[] = "chunk size exceeds the remaining body";
static const char BAD_SIZE[] = "invalid chunk size";
static const char NO_CRLF[] = "chunk data not followed by CRLF";
static const char CUT_SHORT[] = "truncated";

static char *decode(const char *bytes, size_t len, const char **reason) {
  char *exact = malloc(len ? len : 1);
  assert(exact != NULL);
  memcpy(exact, bytes, len);
  *reason = NULL;
  char *out = http_decode_chunked(exact, len, reason);
  free(exact);
  return out;
}

static void expect_body_n(const char *bytes, size_t len, const char *body,
                          size_t body_len) {
  const char *reason = NULL;
  char *out = decode(bytes, len, &reason);
  assert(out != NULL && reason == NULL);
  assert(memcmp(out, body, body_len) == 0 && out[body_len] == '\0');
  free(out);
}

static void expect_refused_n(const char *bytes, size_t len, const char *why) {
  const char *reason = NULL;
  char *out = decode(bytes, len, &reason);
  assert(out == NULL);
  assert(reason != NULL && strcmp(reason, why) == 0);
}

// String-literal forms; the _n forms take the literals that hold a NUL.
#define EXPECT_BODY(bytes, body)                                               \
  expect_body_n(bytes, sizeof(bytes) - 1, body, sizeof(body) - 1)
#define EXPECT_REFUSED(bytes, why)                                             \
  expect_refused_n(bytes, sizeof(bytes) - 1, why)

static void check_table(void) {
  EXPECT_BODY("0\r\n\r\n", "");
  EXPECT_BODY("00000\r\n\r\n", "");
  EXPECT_BODY("5\r\nhello\r\n0\r\n\r\n", "hello");
  EXPECT_BODY("5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n", "hello world");
  EXPECT_BODY("a\r\n0123456789\r\n0\r\n\r\n", "0123456789");
  EXPECT_BODY("A\r\n0123456789\r\n0\r\n\r\n", "0123456789");
  EXPECT_BODY("00000000000000000000000000000005\r\nhello\r\n0\r\n\r\n", "hello");
  EXPECT_BODY("5;a=b\r\nhello\r\n0;c\r\n\r\n", "hello");
  EXPECT_BODY("5 ;a\r\nhello\r\n0\t;c\r\n\r\n", "hello");
  EXPECT_BODY("5 \t \r\nhello\r\n0 \r\n\r\n", "hello");
  EXPECT_BODY("3\r\nabc\r\n0\r\nX-A: 1\r\nX-B: 2\r\n\r\n", "abc");
  EXPECT_BODY("8\r\na\r\n0\r\n\r\n\r\n0\r\n\r\n", "a\r\n0\r\n\r\n");
  EXPECT_BODY("2\r\n\r\n\r\n0\r\n\r\n", "\r\n");
  EXPECT_BODY("3\r\nabc\r\n0\r\n\r\nleft over", "abc");
  EXPECT_BODY("5\r\nhe\0lo\r\n0\r\n\r\n", "he\0lo");
  EXPECT_BODY("0\r\nX\0Y: 1\r\n\r\n", "");

  EXPECT_REFUSED("10\r\nhi\r\n0\r\n\r\n", TOO_BIG);
  EXPECT_REFUSED("6\r\nhello", TOO_BIG);
  EXPECT_REFUSED("1\r\n", TOO_BIG);
  EXPECT_REFUSED("7FFFFFFF\r\nhi\r\n0\r\n\r\n", TOO_BIG);
  EXPECT_REFUSED("80000000\r\nhi\r\n0\r\n\r\n", TOO_BIG);
  EXPECT_REFUSED("FFFFFFFF\r\nhi\r\n0\r\n\r\n", TOO_BIG);
  EXPECT_REFUSED("7FFFFFFFFFFFFFFF\r\nhi\r\n0\r\n\r\n", TOO_BIG);
  EXPECT_REFUSED("8000000000000000\r\nhi\r\n0\r\n\r\n", TOO_BIG);
  EXPECT_REFUSED("FFFFFFFFFFFFFFFF\r\nhi\r\n0\r\n\r\n", TOO_BIG);
  EXPECT_REFUSED("3\r\nabc\r\n100\r\nhi\r\n0\r\n\r\n", TOO_BIG);

  EXPECT_REFUSED("10000000000000000\r\n\r\n", BAD_SIZE);
  EXPECT_REFUSED("10000000000000005\r\nhello\r\n0\r\n\r\n", BAD_SIZE);
  EXPECT_REFUSED("FFFFFFFFFFFFFFFFF\r\nhi\r\n0\r\n\r\n", BAD_SIZE);
  EXPECT_REFUSED("-1\r\nhi\r\n0\r\n\r\n", BAD_SIZE);
  EXPECT_REFUSED("-0\r\n\r\n", BAD_SIZE);
  EXPECT_REFUSED("+5\r\nhello\r\n0\r\n\r\n", BAD_SIZE);
  EXPECT_REFUSED("0x5\r\nhello\r\n0\r\n\r\n", BAD_SIZE);
  EXPECT_REFUSED(" 5\r\nhello\r\n0\r\n\r\n", BAD_SIZE);
  EXPECT_REFUSED("\t5\r\nhello\r\n0\r\n\r\n", BAD_SIZE);
  EXPECT_REFUSED("zz\r\nhello\r\n0\r\n\r\n", BAD_SIZE);
  EXPECT_REFUSED("5z\r\nhello\r\n0\r\n\r\n", BAD_SIZE);
  EXPECT_REFUSED("5 z\r\nhello\r\n0\r\n\r\n", BAD_SIZE);
  EXPECT_REFUSED("\r\n", BAD_SIZE);
  EXPECT_REFUSED(";a\r\nhi\r\n0\r\n\r\n", BAD_SIZE);
  EXPECT_REFUSED("5\nhello\n0\n\n", BAD_SIZE);
  EXPECT_REFUSED("3\r\nabc\r\nzz\r\n", BAD_SIZE);
  EXPECT_REFUSED("5\r\nhello\r\n\r\n", BAD_SIZE);
  EXPECT_REFUSED("\0", BAD_SIZE);
  EXPECT_REFUSED("5\0\r\nhello\r\n0\r\n\r\n", BAD_SIZE);
  EXPECT_REFUSED("0\0\r\n\r\n", BAD_SIZE);

  EXPECT_REFUSED("5\r\nhelloXX0\r\n\r\n", NO_CRLF);
  EXPECT_REFUSED("5\r\nhello\n0\r\n\r\n", NO_CRLF);
  EXPECT_REFUSED("5\r\nhello\rX0\r\n\r\n", NO_CRLF);
  EXPECT_REFUSED("4\r\nhello\r\n0\r\n\r\n", NO_CRLF);
  EXPECT_REFUSED("6\r\nhello\r\n0\r\n\r\n", NO_CRLF);

  EXPECT_REFUSED("", CUT_SHORT);
  EXPECT_REFUSED("5", CUT_SHORT);
  EXPECT_REFUSED("5\r", CUT_SHORT);
  EXPECT_REFUSED("5 ", CUT_SHORT);
  EXPECT_REFUSED("5;a", CUT_SHORT);
  EXPECT_REFUSED("5\r\nhello", CUT_SHORT);
  EXPECT_REFUSED("5\r\nhello\r", CUT_SHORT);
  EXPECT_REFUSED("5\r\nhello\r\n", CUT_SHORT);
  EXPECT_REFUSED("0", CUT_SHORT);
  EXPECT_REFUSED("0\r\n", CUT_SHORT);
  EXPECT_REFUSED("0\r\n\r", CUT_SHORT);
  EXPECT_REFUSED("0\r\nX-A: 1", CUT_SHORT);
  EXPECT_REFUSED("0\r\nX-A: 1\r\n", CUT_SHORT);
}

// A body is only complete at its final CRLF, so every shorter cut is refused.
static void check_every_prefix(void) {
  static const char *const framings[] = {
      "0\r\n\r\n",
      "5\r\nhello\r\n0\r\n\r\n",
      "5;a=b\r\nhello\r\n6 ;c\r\n world\r\n000\r\n\r\n",
      "3\r\nabc\r\n0\r\nX-A: 1\r\nX-B: 2\r\n\r\n",
      "8\r\na\r\n0\r\n\r\n\r\n0\r\n\r\n",
  };
  for (size_t f = 0; f < sizeof(framings) / sizeof(framings[0]); f++) {
    size_t total = strlen(framings[f]);
    const char *reason = NULL;
    char *whole = decode(framings[f], total, &reason);
    assert(whole != NULL && reason == NULL);
    free(whole);
    for (size_t cut = 0; cut < total; cut++) {
      assert(decode(framings[f], cut, &reason) == NULL);
      assert(reason != NULL);
    }
  }
}

// Declares every size around a five-byte payload: only the true one decodes,
// and a declared zero ends the body, leaving the rest as trailer lines.
static void check_every_declared_size(void) {
  static const char rest[] = "hello\r\n0\r\n\r\n";
  size_t rest_len = sizeof(rest) - 1;
  for (size_t size = 0; size < 4096; size++) {
    char framing[64];
    int n = snprintf(framing, sizeof(framing), "%zx\r\n%s", size, rest);
    assert(n > 0);
    const char *reason = NULL;
    char *out = decode(framing, (size_t)n, &reason);
    if (size == 0 || size == 5) {
      assert(out != NULL && reason == NULL);
      assert(strcmp(out, size == 5 ? "hello" : "") == 0);
      free(out);
    } else {
      assert(out == NULL && reason != NULL);
      if (size > rest_len) {
        assert(strcmp(reason, TOO_BIG) == 0);
      }
    }
  }
}

// Walks a set bit and an all-ones run through every width of a size_t, then past
// it: what fits is refused by the bound, what does not is refused as a size.
static void check_every_bit_width(void) {
  for (unsigned bit = 8; bit < 64; bit++) {
    uint64_t value = (uint64_t)1 << bit;
    const uint64_t forms[] = {value, value - 1, value + 5};
    for (size_t form = 0; form < 3; form++) {
      char framing[64];
      int n = snprintf(framing, sizeof(framing), "%llx\r\nhello\r\n0\r\n\r\n",
                       (unsigned long long)forms[form]);
      assert(n > 0);
      expect_refused_n(framing, (size_t)n, TOO_BIG);
      n = snprintf(framing, sizeof(framing), "%llX;x\r\nhello\r\n0\r\n\r\n",
                   (unsigned long long)forms[form]);
      assert(n > 0);
      expect_refused_n(framing, (size_t)n, TOO_BIG);
    }
  }
  for (size_t digits = 17; digits <= 40; digits++) {
    char framing[80];
    memset(framing, 'F', digits);
    memcpy(framing + digits, "\r\nhi\r\n0\r\n\r\n", 11);
    expect_refused_n(framing, digits + 11, BAD_SIZE);
    framing[0] = '1';
    memset(framing + 1, '0', digits - 1);
    expect_refused_n(framing, digits + 11, BAD_SIZE);
  }
}

// Size formats a real origin may use, each applied to every chunk.
static int put_size(char *at, size_t size, unsigned style) {
  switch (style) {
  case 0:
    return sprintf(at, "%zx\r\n", size);
  case 1:
    return sprintf(at, "%zX\r\n", size);
  case 2:
    return sprintf(at, "%08zx\r\n", size);
  case 3:
    return sprintf(at, "%zx;name=value;flag\r\n", size);
  default:
    return sprintf(at, "%zx \t;name\r\n", size);
  }
}

// Reframes one payload at every chunk length, in every size format, with and
// without trailers, and requires the payload back.
static void check_round_trips(void) {
  char payload[512];
  for (size_t i = 0; i < sizeof(payload) - 1; i++) {
    payload[i] = "0a\r\n;Fz \t-"[i % 10];
  }
  size_t payload_len = sizeof(payload) - 1;
  payload[payload_len] = '\0';
  char *framing = malloc(payload_len * 40 + 128);
  assert(framing != NULL);
  for (size_t split = 1; split <= payload_len; split++) {
    for (unsigned style = 0; style < 5; style++) {
      for (unsigned trailers = 0; trailers < 2; trailers++) {
        size_t at = 0;
        for (size_t done = 0; done < payload_len; done += split) {
          size_t take = payload_len - done < split ? payload_len - done : split;
          at += (size_t)put_size(framing + at, take, style);
          memcpy(framing + at, payload + done, take);
          at += take;
          memcpy(framing + at, "\r\n", 2);
          at += 2;
        }
        at += (size_t)put_size(framing + at, 0, style);
        at += (size_t)sprintf(framing + at, "%s\r\n",
                              trailers ? "X-A: 1\r\nX-B: 2\r\n" : "");
        expect_body_n(framing, at, payload, payload_len);
      }
    }
  }
  free(framing);
}

static void expect_either_outcome(const char *bytes, size_t len) {
  const char *reason = NULL;
  char *out = decode(bytes, len, &reason);
  if (out) {
    assert(reason == NULL);
    assert(strlen(out) <= len);
    free(out);
  } else {
    assert(reason != NULL);
  }
}

// Every byte value at every position, every deleted byte and every inserted
// byte: however the framing is damaged, the decoder answers and stays in bounds.
static void check_every_mutation(void) {
  static const char framing[] =
      "5;a=b\r\nhello\r\n1A\r\nabcdefghijklmnopqrstuvwxyz\r\n"
      "0;end\r\nX-A: 1\r\n\r\n";
  size_t len = sizeof(framing) - 1;
  char mutated[sizeof(framing) + 1];
  for (size_t at = 0; at < len; at++) {
    for (int byte = 0; byte < 256; byte++) {
      memcpy(mutated, framing, len);
      mutated[at] = (char)byte;
      expect_either_outcome(mutated, len);
      memcpy(mutated, framing, at);
      mutated[at] = (char)byte;
      memcpy(mutated + at + 1, framing + at, len - at);
      expect_either_outcome(mutated, len + 1);
    }
    memcpy(mutated, framing, at);
    memcpy(mutated + at, framing + at + 1, len - at - 1);
    expect_either_outcome(mutated, len - 1);
  }
}

// Bodies past the 8 KiB transport buffer, in one chunk and in a hundred thousand.
static void check_large_bodies(void) {
  size_t payload_len = 4 * 1024 * 1024;
  char *payload = malloc(payload_len + 1);
  char *framing = malloc(payload_len * 7 + 64);
  assert(payload != NULL && framing != NULL);
  memset(payload, 'x', payload_len);
  payload[payload_len] = '\0';

  int head = sprintf(framing, "%zx\r\n", payload_len);
  memcpy(framing + head, payload, payload_len);
  memcpy(framing + (size_t)head + payload_len, "\r\n0\r\n\r\n", 7);
  size_t whole = (size_t)head + payload_len + 7;
  expect_body_n(framing, whole, payload, payload_len);
  expect_refused_n(framing, whole - 7 - 1, TOO_BIG);

  size_t at = 0;
  for (size_t i = 0; i < 100000; i++) {
    memcpy(framing + at, "1\r\nx\r\n", 6);
    at += 6;
  }
  memcpy(framing + at, "0\r\n\r\n", 5);
  expect_body_n(framing, at + 5, payload, 100000);
  free(framing);
  free(payload);
}

// No reason with no result is how the caller tells out-of-memory from a bad body.
static void check_allocation_failure(void) {
  static const char framing[] = "5\r\nhello\r\n0\r\n\r\n";
  size_t len = sizeof(framing) - 1;
  const char *reason = "untouched";
  test_alloc_fail_size(len + 1);
  assert(http_decode_chunked(framing, len, &reason) == NULL);
  test_alloc_fail_off();
  assert(reason == NULL);
}

static void check_which_headers_mean_chunked(void) {
  static const char *const chunked[] = {
      "Transfer-Encoding: chunked",
      "Transfer-Encoding: chunked\r\n",
      "Transfer-Encoding:chunked\r\n",
      "transfer-encoding: chunked\r\n",
      "TRANSFER-ENCODING: CHUNKED\r\n",
      "Transfer-Encoding: ChUnKeD\r\n",
      "Transfer-Encoding:   chunked \t\r\n",
      "Transfer-Encoding: gzip, chunked\r\n",
      "Transfer-Encoding: gzip,chunked\r\n",
      "Transfer-Encoding: gzip,\tchunked\r\n",
      "Server: x\r\nTransfer-Encoding: chunked\r\nX-Other: y\r\n",
  };
  static const char *const other[] = {
      "",
      "\r\n",
      "Server: x\r\n",
      "Transfer-Encoding:",
      "Transfer-Encoding: \r\n",
      "Transfer-Encoding: gzip\r\n",
      "Transfer-Encoding: gzip\r\nX-Note: chunked\r\n",
      "Transfer-Encoding: chunked, gzip\r\n",
      "Transfer-Encoding: notchunked\r\n",
      "Transfer-Encoding: chunke\r\n",
      "X-Transfer-Encoding: chunked\r\n",
      "X-Note: Transfer-Encoding: chunked\r\n",
      "Transfer-Encodings: chunked\r\n",
      "Transfer-Encoding",
  };
  for (size_t i = 0; i < sizeof(chunked) / sizeof(chunked[0]); i++) {
    assert(http_header_is_chunked(chunked[i]));
  }
  for (size_t i = 0; i < sizeof(other) / sizeof(other[0]); i++) {
    assert(!http_header_is_chunked(other[i]));
  }
  assert(!http_header_is_chunked(NULL));
}

void test_chunked_decoder(void) {
  check_table();
  check_every_prefix();
  check_every_declared_size();
  check_every_bit_width();
  check_round_trips();
  check_every_mutation();
  check_large_bodies();
  check_allocation_failure();
  check_which_headers_mean_chunked();
}
