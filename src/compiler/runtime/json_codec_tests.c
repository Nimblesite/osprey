// The JSON codec: nesting and duplicate-member limits, the reasons a
// rejection records, the decode slots and the single-buffer
// writer [BUILTIN-JSON-LIMITS] [BUILTIN-JSON-TYPED]. Third translation unit of
// the json suite; see json_tests_shared.h.
#include <math.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "json_tests_shared.h"

enum { K_NULL, K_FALSE, K_TRUE, K_INT, K_FLOAT, K_STRING, K_ARRAY, K_OBJECT };

static void expect_reason(const char *needle) {
  const char *reason = osp_io_error();
  CHECK(reason != NULL && strstr(reason, needle) != NULL);
}

static void expect_text(char *got, const char *want) {
  CHECK(got != NULL && strcmp(got, want) == 0);
  free(got);
}

// `depth` nested arrays around a zero.
static char *nested(size_t depth) {
  char *text = malloc(depth * 2 + 2);
  CHECK(text != NULL);
  memset(text, '[', depth);
  text[depth] = '0';
  memset(text + depth + 1, ']', depth);
  text[depth * 2 + 1] = '\0';
  return text;
}

static void t_depth_limit(void) {
  char *deepest = nested(128);
  int64_t h = json_parse(deepest);
  CHECK(h > 0 && json_free(h) == 0);
  free(deepest);
  char *deeper = nested(129);
  CHECK(json_parse(deeper) == ERR_MALFORMED);
  expect_reason("jsonParse: byte 128: nesting deeper than 128 levels");
  free(deeper);
  // Objects count the same as arrays, and depth is per path, not per total.
  CHECK(json_free(json_parse("[{\"a\":[{\"b\":[]}]},{\"c\":[]}]")) == 0);
}

static void t_duplicate_members(void) {
  CHECK(json_parse("{\"a\":1,\"a\":2}") == ERR_MALFORMED);
  expect_reason("jsonParse: byte 0: duplicate member name");
  // The reason points at the object that repeats a name, however deep.
  CHECK(json_parse("[{\"x\":{\"k\":1,\"j\":2,\"k\":3}}]") == ERR_MALFORMED);
  expect_reason("jsonParse: byte 6: duplicate member name");
  // A large object sorts its names to find the repeat; same answer.
  CHECK(json_parse("{\"a\":1,\"b\":2,\"c\":3,\"d\":4,\"e\":5,\"f\":6,"
                   "\"g\":7,\"h\":8,\"i\":9,\"b\":10}") == ERR_MALFORMED);
  expect_reason("jsonParse: byte 0: duplicate member name");
  int64_t wide = json_parse("{\"a\":1,\"b\":2,\"c\":3,\"d\":4,\"e\":5,\"f\":6,"
                            "\"g\":7,\"h\":8,\"i\":9,\"j\":10}");
  CHECK(wide > 0 && json_length(wide, "") == 10 && json_free(wide) == 0);
  // Distinct names, including ones that differ only past a shared prefix or
  // by an escape that decodes to a different scalar, all stay.
  int64_t h = json_parse("{\"ab\":1,\"a\":2,\"b\":3,\"a\\u0062c\":4}");
  CHECK(h > 0 && json_length(h, "") == 4 && json_free(h) == 0);
  // An escape that decodes to the SAME name is the same name.
  CHECK(json_parse("{\"ab\":1,\"a\\u0062\":2}") == ERR_MALFORMED);
}

static void t_reasons(void) {
  CHECK(json_parse("[1,") == ERR_MALFORMED);
  expect_reason("jsonParse: byte 3: unexpected end of text");
  CHECK(json_parse("[1 2]") == ERR_MALFORMED);
  expect_reason("byte 3: expected ',' or ']' in array");
  CHECK(json_parse("{\"a\" 1}") == ERR_MALFORMED);
  expect_reason("expected ':' after a member name");
  CHECK(json_parse("{\"a\":1 \"b\":2}") == ERR_MALFORMED);
  expect_reason("expected ',' or '}' in object");
  CHECK(json_parse("{1:2}") == ERR_MALFORMED);
  expect_reason("byte 1: expected a string");
  CHECK(json_parse("\"a\\qb\"") == ERR_MALFORMED);
  expect_reason("invalid character or escape in string");
  CHECK(json_parse("\"open") == ERR_MALFORMED);
  expect_reason("unterminated string");
  CHECK(json_parse("1.") == ERR_MALFORMED);
  expect_reason("byte 0: invalid number");
  CHECK(json_parse("nul") == ERR_MALFORMED);
  expect_reason("unexpected character");
  CHECK(json_parse("  ") == ERR_MALFORMED);
  expect_reason("byte 2: unexpected end of text");
  CHECK(json_parse("1 2") == ERR_TRAILING);
  expect_reason("byte 2: trailing text after the value");
  CHECK(json_parse(NULL) == ERR_NULL);
  expect_reason("jsonParse: document: no text");
}

// Slot 0 is the root, and each container's children are one run of slots
// [first, end), laid down as the container closes: here `a`'s three
// elements, then `b`'s one member, then the root's four members.
static void t_decode_slots(void) {
  int64_t plain = json_parse("[1]");
  CHECK(json_node_kind(plain, 0) == -1); // a jsonParse handle is not a decode
  CHECK(json_free(plain) == 0);
  int64_t h = json_decode_open(
      "{\"a\":[1,-2.5e1,\"x\\n\"],\"b\":{\"c\":null},\"t\":true,\"f\":false}");
  CHECK(h > 0 && json_node_first(h, 0) == 5 && json_node_end(h, 0) == 9);
  const int64_t kinds[] = {K_OBJECT, K_INT,   K_FLOAT, K_STRING, K_NULL,
                           K_ARRAY,  K_OBJECT, K_TRUE,  K_FALSE};
  const char *keys[] = {"", "", "", "", "c", "a", "b", "t", "f"};
  for (int64_t slot = 0; slot < 9; slot++) {
    CHECK(json_node_kind(h, slot) == kinds[slot]);
    expect_text(json_node_key(h, slot), keys[slot]);
  }
  CHECK(json_node_first(h, 5) == 1 && json_node_end(h, 5) == 4);
  CHECK(json_node_first(h, 6) == 4 && json_node_end(h, 6) == 5);
  CHECK(json_node_int(h, 1) == 1 && json_node_float(h, 2) == -25.0);
  expect_text(json_node_text(h, 3), "x\n");
  // Each accessor answers only its own kind, and anything else is empty.
  expect_text(json_node_text(h, 1), "");
  CHECK(json_node_int(h, 2) == 0 && json_node_float(h, 1) == 0.0);
  CHECK(json_node_first(h, 1) == 0 && json_node_end(h, 1) == 0);
  // Every out-of-range question is a refusal, never a read.
  CHECK(json_node_kind(h, 9) == -1 && json_node_kind(h, -1) == -1);
  CHECK(json_node_int(h, 99) == 0 && json_node_float(h, -1) == 0.0);
  expect_text(json_node_key(h, 9), "");
  expect_text(json_node_text(h, -1), "");
  CHECK(json_node_end(h, 9) == 0 && json_node_kind(0, 0) == -1);
  CHECK(json_decode_close(h) == 0);
  CHECK(json_node_kind(h, 0) == -1 && json_node_end(h, 0) == 0);
  CHECK(json_decode_close(h) == -1); // closed twice
  expect_reason("jsonDecode: document: invalid document handle");
  CHECK(json_decode_close(0) == -1 && json_decode_close(64) == -1);
  // Malformed text is a jsonDecode failure with the parser's reason.
  CHECK(json_decode_open("[1,") == ERR_MALFORMED);
  expect_reason("jsonDecode: byte 3: unexpected end of text");
  CHECK(json_decode_open("{} x") == ERR_TRAILING);
  CHECK(json_decode_open(NULL) == ERR_NULL);
}

// A decode table belongs to its thread and holds a bounded number of open
// documents; a full one refuses rather than overwriting.
static void t_decode_capacity(void) {
  int64_t open[63];
  for (int i = 0; i < 63; i++) {
    open[i] = json_decode_open("[]");
    CHECK(open[i] == i + 1);
  }
  CHECK(json_decode_open("[]") == ERR_TABLE_FULL);
  expect_reason("jsonDecode: document: too many open documents");
  for (int i = 0; i < 63; i++) {
    CHECK(json_decode_close(open[i]) == 0);
  }
}

static void expect_unrepresentable(const char *text, const char *why) {
  CHECK(json_decode_open((char *)(uintptr_t)text) == ERR_MALFORMED);
  expect_reason(why);
  int64_t h = json_parse((char *)(uintptr_t)text); // the handle API keeps text
  CHECK(h > 0 && json_free(h) == 0);
}

static void t_decode_numbers(void) {
  int64_t h = json_decode_open(
      "[9223372036854775807,-9223372036854775808,-0,1e3,1.0,"
      "123456789012345678,-999999999999999999]");
  CHECK(h > 0 && json_node_first(h, 0) == 1 && json_node_end(h, 0) == 8);
  const int64_t kinds[] = {K_INT, K_INT, K_INT, K_FLOAT, K_FLOAT, K_INT, K_INT};
  for (int64_t i = 0; i < 7; i++) {
    CHECK(json_node_kind(h, i + 1) == kinds[i]);
  }
  CHECK(json_node_int(h, 1) == INT64_MAX && json_node_int(h, 2) == INT64_MIN);
  CHECK(json_node_int(h, 3) == 0 && json_node_float(h, 4) == 1000.0);
  CHECK(json_node_float(h, 5) == 1.0);
  CHECK(json_node_int(h, 6) == 123456789012345678);
  CHECK(json_node_int(h, 7) == -999999999999999999);
  CHECK(json_decode_close(h) == 0);
  expect_unrepresentable("[9223372036854775808]",
                         "jsonDecode: byte 1: integer outside the 64-bit range");
  expect_unrepresentable("{\"n\":-9223372036854775809}",
                         "byte 5: integer outside the 64-bit range");
  expect_unrepresentable("[1e999]", "byte 1: number outside the float range");
  expect_unrepresentable("-1e400", "byte 0: number outside the float range");
}

static void t_writer(void) {
  int64_t w = json_writer_open();
  CHECK(w > 0);
  CHECK(json_write_raw(w, "[") == 0);
  CHECK(json_write_string(w, "q\"b\\s/\b\f\n\r\t\x01\x1f\x7f") == 0);
  CHECK(json_write_string(w, "\xc3\xa9\xf0\x9f\x98\x80") == 0);
  CHECK(json_write_int(w, INT64_MIN) == 0);
  CHECK(json_write_float(w, 1.0) == 0);
  CHECK(json_write_float(w, 0.1) == 0);
  CHECK(json_write_float(w, 1e300) == 0);
  CHECK(json_write_string(w, "") == 0);
  CHECK(json_write_raw(w, "]") == 0);
  expect_text(json_writer_close(w),
              "[\"q\\\"b\\\\s/\\b\\f\\n\\r\\t\\u0001\\u001f\x7f\",\"\xc3\xa9\xf0\x9f\x98\x80\","
              "-9223372036854775808,1.0,0.1,1e+300,\"\"]");
  // Separators are the writer's: a value after a value gets one, and an
  // opening bracket or a ':' resets that, at every depth.
  w = json_writer_open();
  const char *tokens[] = {"{", "a", ":", "[", "[", "]", "null", "{", "}", "]", "b", ":", "true", "}"};
  for (size_t i = 0; i < sizeof(tokens) / sizeof(tokens[0]); i++) {
    bool key = strcmp(tokens[i], "a") == 0 || strcmp(tokens[i], "b") == 0;
    CHECK((key ? json_write_string(w, (char *)(uintptr_t)tokens[i])
               : json_write_raw(w, (char *)(uintptr_t)tokens[i])) == 0);
  }
  expect_text(json_writer_close(w), "{\"a\":[[],null,{}],\"b\":true}");
  // The writer's output is a document the parser accepts unchanged.
  w = json_writer_open();
  CHECK(json_write_string(w, "a\x01\"\\\xe2\x82\xac") == 0);
  char *quoted = json_writer_close(w);
  int64_t h = json_parse(quoted);
  CHECK(h > 0);
  expect_get(h, "", "a\x01\"\\\xe2\x82\xac");
  CHECK(json_free(h) == 0);
  free(quoted);
  CHECK(json_writer_close(w) == NULL); // closed twice
  expect_reason("jsonEncode: value: invalid writer handle");
  CHECK(json_write_raw(w, "x") == -1 && json_write_int(0, 1) == -1);
  expect_text(json_writer_close(json_writer_open()), ""); // an empty document
}

static void expect_writer_refusal(const char *why, int which) {
  int64_t w = json_writer_open();
  CHECK(json_write_raw(w, "[") == 0);
  volatile double zero = 0.0;
  int64_t status = which == 0   ? json_write_string(w, "ok\xc0\xaf")
                   : which == 1 ? json_write_string(w, "\xed\xa0\x80")
                   : which == 2 ? json_write_float(w, zero / zero)
                                : json_write_float(w, 1e308 * (10.0 + zero));
  CHECK(status == -1);
  // Sticky: later writes refuse too, and close reports the FIRST reason.
  CHECK(json_write_raw(w, "]") == -1 && json_write_int(w, 1) == -1);
  CHECK(json_writer_close(w) == NULL);
  expect_reason(why);
}

static void t_writer_refusals(void) {
  expect_writer_refusal("jsonEncode: value: string is not valid UTF-8", 0); // overlong '/'
  expect_writer_refusal("jsonEncode: value: string is not valid UTF-8", 1); // surrogate
  expect_writer_refusal("jsonEncode: value: float is not finite", 2);       // NaN
  expect_writer_refusal("jsonEncode: value: float is not finite", 3);       // +inf
}

void t_codec(void) {
  t_depth_limit();
  t_duplicate_members();
  t_reasons();
  t_decode_slots();
  t_decode_capacity();
  t_decode_numbers();
  t_writer();
  t_writer_refusals();
}
