#include "http_shared.h"
#include <ctype.h>
#include <string.h>

// HTTP field lines, read as the RFCs define them, for every head the runtime
// reads [HTTP-FIELDS]. The head is the caller's, `length` bytes: no read goes past it.

const char *http_skip_ows(const char *start, const char *end) {
  while (start < end && (*start == ' ' || *start == '\t')) {
    start++;
  }
  return start;
}

const char *http_trim_ows(const char *start, const char *end) {
  while (end > start && (end[-1] == ' ' || end[-1] == '\t')) {
    end--;
  }
  return end;
}

bool http_text_is(const char *text, size_t length, const char *expected) {
  if (length != strlen(expected)) {
    return false;
  }
  for (size_t index = 0; index < length; index++) {
    if (tolower((unsigned char)text[index]) !=
        tolower((unsigned char)expected[index])) {
      return false;
    }
  }
  return true;
}

// The line from `at` to `stop`, its '\n' or the end of the head, as a field when
// it has a colon: the name before it, the value after it with its blanks trimmed.
static bool http_line_field(const char *head, size_t at, size_t stop, HttpField *field) {
  stop = stop > at && head[stop - 1] == '\r' ? stop - 1 : stop;
  const char *colon = memchr(head + at, ':', stop - at);
  if (!colon) {
    return false;
  }
  const char *value = http_skip_ows(colon + 1, head + stop);
  *field = (HttpField){head + at, (size_t)(colon - head - at), value,
                       (size_t)(http_trim_ows(value, head + stop) - value)};
  return true;
}

bool http_next_field(const char *head, size_t length, size_t *cursor, HttpField *field) {
  while (*cursor < length) {
    size_t at = *cursor;
    const char *eol = memchr(head + at, '\n', length - at);
    size_t stop = eol ? (size_t)(eol - head) : length;
    *cursor = eol ? stop + 1 : length;
    if (http_line_field(head, at, stop, field)) {
      return true;
    }
    if (stop == at || (stop == at + 1 && head[at] == '\r')) {
      return false; // the blank line: the field section is over
    }
  }
  return false;
}

bool http_value_lists(const char *value, size_t length, const char *token) {
  const char *end = value + length;
  for (const char *at = value;;) {
    const char *comma = memchr(at, ',', (size_t)(end - at));
    const char *stop = comma ? comma : end;
    const char *item = http_skip_ows(at, stop);
    bool listed = http_text_is(item, (size_t)(http_trim_ows(item, stop) - item), token);
    if (listed || !comma) {
      return listed;
    }
    at = comma + 1;
  }
}
