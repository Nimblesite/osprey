// The JSON string and number grammars: everything they accept and everything
// they must refuse [BUILTIN-JSON-STRING] [BUILTIN-JSON-NUMBER]
// (docs/specs/runtime/0012-Built-InFunctions.md). Second translation unit of the json
// suite; see json_tests_shared.h for why it is separate.
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "json_tests_shared.h"

// [BUILTIN-JSON-STRING]. The string grammar is a closed set: every spelling
// outside it is the SAME rejection -- ERR_MALFORMED, no handle, nothing
// retained -- and every boundary is pinned from both sides, the last spelling
// the grammar refuses and the first it takes.
#define JSON_FIRST_LITERAL_CHAR 0x20
#define JSON_DEL_CHAR 0x7f
#define JSON_CONTROL_ESCAPE_MAX 32

// The accept-decode-release line that pins every string boundary: parse `doc`,
// demand the string at `path` reads back as exactly `want`, free the handle.
static void expect_string(const char *doc, const char *path, const char *want) {
  int64_t h = parse_ok(doc);
  expect_get(h, path, want);
  CHECK(json_free(h) == 0);
}

// Inside a container a number the scanner cannot finish leaves the whole
// container unparseable: wrap `src` with `fmt` (one %s), demand the snprintf
// fits, then demand the rejection.
static void reject_wrapped(const char *fmt, const char *src) {
  char wrapped[32];
  int n = snprintf(wrapped, sizeof(wrapped), fmt, src);
  CHECK(n > 0 && (size_t)n < sizeof(wrapped));
  CHECK(json_parse(wrapped) == ERR_MALFORMED);
}

void t_string_grammar_rejects_everything_outside_it(void) {
  static const char *const malformed[] = {
      // \u spellings that are not four hex digits.
      "{\"k\":\"\\uZZZZ\"}",        // no hex at all
      "{\"k\":\"\\u00g0\"}",        // one bad digit, third position
      "{\"k\":\"\\u000/\"}",        // '/' is adjacent to '0' in ASCII
      "{\"k\":\"\\u12\"}",          // too few digits, closed by the quote
      "{\"k\":\"\\u\"}",            // no digits
      "{\"k\":\"\\u001\"}",         // three digits then end of document
      "{\"k\":\"\\ud83d\\uZZZZ\"}", // bad LOW surrogate
      "{\"k\":\"\\ud83d\\ude0\"}",  // truncated low surrogate at end of text
      // Escapes outside the alphabet. The backslash is never dropped and the
      // character after it is never taken literally.
      "{\"k\":\"\\q\"}",
      "{\"k\":\"\\x41\"}",
      "{\"k\":\"\\U0041\"}", // capital U is not the escape; lowercase is
      "{\"k\":\"\\a\"}",
      "{\"k\":\"\\v\"}",
      "{\"k\":\"\\0\"}",
      "{\"k\":\"\\ \"}",
      "{\"k\":\"\\'\"}",
      "{\"k\":\"v\\\"}", // the backslash eats the closing quote
      // Surrogate halves are not code points, at either end of either range.
      "{\"k\":\"\\ud800\"}",              // lone high, first of its range
      "{\"k\":\"\\udbff\"}",              // lone high, last of its range
      "{\"k\":\"\\udc00\"}",              // lone low, first of its range
      "{\"k\":\"\\udfff\"}",              // lone low, last of its range
      "{\"k\":\"\\ud800x\"}",             // high half, then a plain character
      "{\"k\":\"\\ud800\\n\"}",           // high half, then a different escape
      "{\"k\":\"\\ud800\\u0041\"}",       // high half, then a non-surrogate
      "{\"k\":\"\\ud800\\ud800\"}",       // high half, then another HIGH half
      "{\"k\":\"\\ud800\\udbff\"}",       // ... to the top of the high range
      "{\"k\":\"\\ud800\\ud7ff\"}",       // ... and just below it
      "{\"k\":\"\\udbff\\ue000\"}",       // high half, then just past the low
      "{\"k\":\"\\ud800\\udbff\\udc00\"}" // a valid pair does not rescue it
  };
  for (size_t i = 0; i < sizeof(malformed) / sizeof(malformed[0]); i++) {
    CHECK(json_parse((char *)(uintptr_t)malformed[i]) == ERR_MALFORMED);
  }

  // Every character below U+0020 must arrive escaped: it is the ESCAPING that
  // the grammar requires, not the absence of the character, so the same byte
  // spelled \u00XX is accepted. U+0000 cannot be written into a C string at
  // all, which is why the sweep starts at one.
  for (unsigned ch = 1; ch < JSON_FIRST_LITERAL_CHAR; ch++) {
    char raw[] = "{\"k\":\"?\"}";
    raw[6] = (char)ch;
    CHECK(json_parse(raw) == ERR_MALFORMED);
    char escaped[JSON_CONTROL_ESCAPE_MAX];
    int n = snprintf(escaped, sizeof(escaped), "{\"k\":\"\\u%04x\"}", ch);
    CHECK(n > 0 && (size_t)n < sizeof(escaped));
    char want[] = "?";
    want[0] = (char)ch;
    expect_string(escaped, "k", want);
  }
  // The first character taken literally is the space immediately past them,
  // and DEL is not a control character to JSON at all.
  const char literal[] = {(char)JSON_FIRST_LITERAL_CHAR, (char)JSON_DEL_CHAR};
  for (size_t i = 0; i < sizeof(literal); i++) {
    char raw[] = "{\"k\":\"?\"}";
    raw[6] = literal[i];
    char want[] = "?";
    want[0] = literal[i];
    expect_string(raw, "k", want);
  }

  // Just outside the surrogate block on both sides: ordinary characters.
  expect_string("\"\\ud7ff\"", "", "\xed\x9f\xbf"); // U+D7FF
  expect_string("\"\\ue000\"", "", "\xee\x80\x80"); // U+E000
  // Both extremes of the pair range decode to both extremes of the
  // supplementary planes -- the arithmetic, not just the acceptance.
  expect_string("\"\\ud800\\udc00\"", "", "\xf0\x90\x80\x80"); // U+10000
  expect_string("\"\\udbff\\udfff\"", "", "\xf4\x8f\xbf\xbf"); // U+10FFFF

  // The whole escape alphabet, and nothing but: each one decodes to its own
  // byte, in a KEY as well as a value.
  expect_string("{\"a\\\"b\\\\c\\/d\\bE\\fF\\nG\\rH\\tI\":1}",
                "a\"b\\c/d\bE\fF\nG\rH\tI", "1");

  // The LITERAL path holds the same line as the escape path. JSON text is
  // UTF-8, so a raw byte sequence that is not valid UTF-8 is not a character
  // the document contains -- and copying it through would let an overlong
  // form, a UTF-8-spelled surrogate or a truncated sequence into a value while
  // "\\ud800" two lines away is refused.
  static const char *const literal_utf8[] = {
      "{\"k\":\"\xc3\xa9\"}",         // two bytes, U+00E9
      "{\"k\":\"\xe2\x82\xac\"}",     // three, U+20AC
      "{\"k\":\"\xf0\x9f\x98\x80\"}", // four, U+1F600
      "{\"k\":\"\xf4\x8f\xbf\xbf\"}", // four, U+10FFFF: the last one
      "{\"\xc3\xa9\":1}"             // and in a key
  };
  static const char *const literal_want[] = {"\xc3\xa9", "\xe2\x82\xac",
                                             "\xf0\x9f\x98\x80",
                                             "\xf4\x8f\xbf\xbf"};
  for (size_t i = 0; i < sizeof(literal_want) / sizeof(literal_want[0]); i++) {
    expect_string(literal_utf8[i], "k", literal_want[i]);
  }
  expect_string(literal_utf8[4], "\xc3\xa9", "1");

  static const char *const literal_garbage[] = {
      "{\"k\":\"\x80\"}",             // a continuation byte with no lead
      "{\"k\":\"\xbf\"}",             // the last continuation byte
      "{\"k\":\"\xc3\"}",             // a lead byte with nothing after it
      "{\"k\":\"\xe2\x82\"}",         // three-byte form, one byte short
      "{\"k\":\"\xf0\x9f\x98\"}",     // four-byte form, one byte short
      "{\"k\":\"\xc3\x41\"}",         // continuation slot holds an ASCII 'A'
      "{\"k\":\"\xc0\xaf\"}",         // overlong '/': 2 bytes for U+002F
      "{\"k\":\"\xe0\x80\xaf\"}",     // overlong again, 3 bytes
      "{\"k\":\"\xf0\x80\x80\xaf\"}", // and 4
      "{\"k\":\"\xed\xa0\x80\"}",     // U+D800 spelled in UTF-8
      "{\"k\":\"\xed\xbf\xbf\"}",     // U+DFFF, the other end
      "{\"k\":\"\xf4\x90\x80\x80\"}", // U+110000: past the last code point
      "{\"k\":\"\xf8\x88\x80\x80\x80\"}", // a five-byte form
      "{\"k\":\"\xff\"}",             // never a UTF-8 byte at all
      "{\"\x80\":1}"                  // and none of it is allowed in a key
  };
  for (size_t i = 0; i < sizeof(literal_garbage) / sizeof(literal_garbage[0]);
       i++) {
    CHECK(json_parse((char *)(uintptr_t)literal_garbage[i]) == ERR_MALFORMED);
  }
  // The boundary the other way: U+D7FF and U+E000 spelled literally are
  // ordinary characters, one byte either side of the surrogate block.
  expect_string("{\"k\":\"\xed\x9f\xbf\"}", "k", "\xed\x9f\xbf");
  expect_string("{\"k\":\"\xee\x80\x80\"}", "k", "\xee\x80\x80");

  // U+0000 is the one escape in range that is REFUSED rather than decoded.
  // Every scalar leaves this unit as a NUL-terminated C string, so accepting
  // it would not embed a NUL, it would truncate the value: "a\\u0000b" would
  // read back as "a" and nothing anywhere would say so.
  static const char *const embedded_nul[] = {
      "\"\\u0000\"",         // as the whole document
      "{\"k\":\"a\\u0000b\"}", // in the middle of a value
      "{\"\\u0000\":1}",      // and in a key
      "\"\\ud800\\udc00\"" // (the pair spelling of U+10000 is still fine)
  };
  for (size_t i = 0; i + 1 < sizeof(embedded_nul) / sizeof(embedded_nul[0]);
       i++) {
    CHECK(json_parse((char *)(uintptr_t)embedded_nul[i]) == ERR_MALFORMED);
  }
  expect_string(embedded_nul[3], "", "\xf0\x90\x80\x80");

  // ...and every well-formed spelling still decodes to exact UTF-8: one byte,
  // two, three, and a surrogate pair to four.
  expect_string("{\"k\":\"\\u0041\\u00e9\\u20ac\\ud83d\\ude00\"}", "k",
                "A\xc3\xa9\xe2\x82\xac\xf0\x9f\x98\x80");
}

// [BUILTIN-JSON-NUMBER]. A number is stored as its SOURCE TEXT and handed
// straight back by jsonGet, so anything the scanner accepts is something a
// caller reads as a number. The grammar is RFC 8259's, and both sides of every
// boundary are pinned -- including the exact code, which differs at the root:
// a COMPLETE value followed by more text is trailing garbage, not malformed.
void t_number_grammar(void) {
  static const char *const accepted[] = {
      "0",      "-0",       "1",      "-1",     "42",    "-42",
      "1234567890", "0.0",  "-0.0",   "0.5",    "-0.5",  "1.25",
      "0e0",    "0E0",      "1e5",    "1E5",    "1e+5",  "1E+5",
      "1e-5",   "1E-5",     "-1.5e-10", "0.0e0", "10",   "100",
      "0e00",   "1.000",    "-0.0e-0"};
  for (size_t i = 0; i < sizeof(accepted) / sizeof(accepted[0]); i++) {
    // the exact source text, unrewritten
    expect_string(accepted[i], "", accepted[i]);
  }

  // Inside a container there is no trailing-garbage outcome: whatever the
  // number scanner refuses to finish leaves the container unparseable.
  static const char *const rejected[] = {
      "-",     "+1",  ".5",    "1.",   "1.e5", "1e",   "1E",
      "1e+",   "1e-", "--1",   "-.5",  "-",    "1..2", "1e5e5",
      "01",    "00",  "-01",   "1+2",  "0x10", "1_000"};
  for (size_t i = 0; i < sizeof(rejected) / sizeof(rejected[0]); i++) {
    reject_wrapped("[%s]", rejected[i]);
    reject_wrapped("{\"k\":%s}", rejected[i]);
  }

  // At the ROOT the answer separates two different faults. A number that
  // cannot be finished is malformed; a number that IS finished and is followed
  // by more text is trailing garbage -- which is what a leading zero produces,
  // because the zero is the whole integer part and the rest is not part of it.
  static const struct {
    const char *text;
    int64_t code;
  } at_root[] = {{"-", ERR_MALFORMED},   {"+1", ERR_MALFORMED},
                 {".5", ERR_MALFORMED},  {"1.", ERR_MALFORMED},
                 {"1e", ERR_MALFORMED},  {"1e+", ERR_MALFORMED},
                 {"--1", ERR_MALFORMED}, {"01", ERR_TRAILING},
                 {"00", ERR_TRAILING},   {"-01", ERR_TRAILING},
                 {"1+2", ERR_TRAILING},  {"0x10", ERR_TRAILING}};
  for (size_t i = 0; i < sizeof(at_root) / sizeof(at_root[0]); i++) {
    CHECK(json_parse((char *)(uintptr_t)at_root[i].text) == at_root[i].code);
  }

  // A rejection leaves nothing behind: the table is intact and the next parse
  // gets the lowest slot, exactly as it would have before.
  int64_t after = parse_ok("[0,-0.5,1e-5]");
  CHECK(json_length(after, "") == 3);
  expect_get(after, "[2]", "1e-5");
  CHECK(json_free(after) == 0);
}

