#include "http_client_internal.h"
#include "http_shared.h"
#include "io_error.h"

#include <strings.h> // strncasecmp

// What a response's bytes mean, split from the response handles
// (http_client_response.c) so each TU stays under the repo size budget: its status
// line, its head and how its body is framed. Everything here treats the bytes as
// untrusted. Implements [HTTP-RESPONSE-FRAMING] and [HTTP-CHUNKED-DECODE].

// Whether chunked is the last coding of one Transfer-Encoding value, as a whole
// word. value - 1 is always the header's colon, so looking back stays in bounds.
static bool value_ends_chunked(const char *value, const char *end) {
  static const size_t coding_len = 7;
  while (end > value && (end[-1] == ' ' || end[-1] == '\t')) {
    end--;
  }
  if ((size_t)(end - value) < coding_len ||
      strncasecmp(end - coding_len, "chunked", coding_len) != 0) {
    return false;
  }
  char before = end[-(ptrdiff_t)coding_len - 1];
  return before == ':' || before == ' ' || before == '\t' || before == ',';
}

// http_header_is_chunked reports whether the response header block names chunked
// as the final coding of its Transfer-Encoding header (case-insensitive, per
// RFC 9112 6.1). Repeated lines are one list, so the last one decides. Any other
// header that merely mentions either word does not count.
bool http_header_is_chunked(const char *headers) {
  static const size_t name_len = 18;
  bool chunked = false;
  const char *line = headers;
  while (line && *line) {
    const char *eol = strstr(line, "\r\n");
    if (strncasecmp(line, "Transfer-Encoding:", name_len) == 0) {
      const char *value = line + name_len;
      chunked = value_ends_chunked(value, eol ? eol : value + strlen(value));
    }
    line = eol ? eol + 2 : NULL;
  }
  return chunked;
}

static const char CHUNK_TOO_BIG[] = "chunk size exceeds the remaining body";
static const char CHUNK_BAD_SIZE[] = "invalid chunk size";
static const char CHUNK_NO_CRLF[] = "chunk data not followed by CRLF";
static const char CHUNK_CUT_SHORT[] = "truncated";
static const char CHUNK_BAD_TRAILER[] = "invalid trailer";

static const char *find_crlf(const char *p, const char *end) {
  for (; end - p >= 2; p++) {
    if (p[0] == '\r' && p[1] == '\n') {
      return p;
    }
  }
  return NULL;
}

static bool has_control_byte(const char *p, const char *end) {
  for (; p < end; p++) {
    if (((unsigned char)*p < 0x20 && *p != '\t') || *p == 0x7f) {
      return true;
    }
  }
  return false;
}

static int hex_digit(char c) {
  if (c >= '0' && c <= '9') {
    return c - '0';
  }
  if (c >= 'a' && c <= 'f') {
    return c - 'a' + 10;
  }
  return c >= 'A' && c <= 'F' ? c - 'A' + 10 : -1;
}

// Parses "<hex-size>[ws][;ext]\r\n" at *cursor and moves past it. Returns NULL,
// or why the line is malformed. A size that overflows is malformed, so it can
// never wrap round to a small value that passes the caller's bound.
static const char *parse_chunk_size(const char **cursor, const char *end,
                                    size_t *size) {
  const char *p = *cursor;
  const char *digits = p;
  *size = 0;
  for (; p < end && hex_digit(*p) >= 0; p++) {
    if (*size > SIZE_MAX / 16) {
      return CHUNK_BAD_SIZE;
    }
    *size = *size * 16 + (size_t)hex_digit(*p);
  }
  if (p == end) {
    return CHUNK_CUT_SHORT;
  }
  if (p == digits) {
    return CHUNK_BAD_SIZE;
  }
  while (p < end && (*p == ' ' || *p == '\t')) {
    p++;
  }
  if (p == end) {
    return CHUNK_CUT_SHORT;
  }
  if (*p != ';' && *p != '\r') {
    return CHUNK_BAD_SIZE;
  }
  const char *line_end = find_crlf(p, end);
  if (!line_end) {
    return *p == ';' || end - p == 1 ? CHUNK_CUT_SHORT : CHUNK_BAD_SIZE;
  }
  if (has_control_byte(p, line_end)) {
    return CHUNK_BAD_SIZE; // a bare CR or LF here is how smuggled framing hides
  }
  *cursor = line_end + 2;
  return NULL;
}

// Decodes every chunk of [p, end) into out, which holds end - p + 1 bytes: the
// decoded bytes can't outnumber the framed ones. The body ends at the blank line
// after the zero chunk's trailers, and bytes after it are ignored, as the
// connection closes there. Returns NULL, or why the framing is malformed.
static const char *decode_chunks(const char *p, const char *end, char *out) {
  size_t out_len = 0;
  for (;;) {
    size_t size = 0;
    const char *malformed = parse_chunk_size(&p, end, &size);
    if (malformed) {
      return malformed;
    }
    if (size == 0) {
      break;
    }
    if (size > (size_t)(end - p)) {
      return CHUNK_TOO_BIG;
    }
    memcpy(out + out_len, p, size);
    out_len += size;
    p += size;
    if (end - p < 2) {
      return CHUNK_CUT_SHORT;
    }
    if (p[0] != '\r' || p[1] != '\n') {
      return CHUNK_NO_CRLF;
    }
    p += 2;
  }
  out[out_len] = '\0';
  for (;;) {
    const char *line_end = find_crlf(p, end);
    if (!line_end) {
      return CHUNK_CUT_SHORT;
    }
    if (line_end == p) {
      return NULL;
    }
    if (has_control_byte(p, line_end)) {
      return CHUNK_BAD_TRAILER;
    }
    p = line_end + 2;
  }
}

// http_decode_chunked decodes the len bytes of an HTTP/1.1 chunked body. Returns
// a freshly allocated, NUL-terminated body (caller frees), or NULL with *malformed
// saying what is wrong with the framing, or NULL with *malformed NULL when out of
// memory. Nothing partial comes back. Implements [HTTP-CHUNKED-DECODE].
char *http_decode_chunked(const char *body, size_t len, const char **malformed) {
  *malformed = NULL;
  char *out = malloc(len + 1);
  if (!out) {
    return NULL;
  }
  *malformed = decode_chunks(body, body + len, out);
  if (*malformed) {
    free(out);
    return NULL;
  }
  return out;
}

// http_parse_status_line returns the status from an HTTP/1.x status line, or -8.
// A status is exactly three digits, so a hostile one can neither overflow nor pose
// as one of this API's own negative codes.
int64_t http_parse_status_line(const char *raw) {
  if (strncmp(raw, "HTTP/1.1 ", 9) != 0 && strncmp(raw, "HTTP/1.0 ", 9) != 0) {
    return -8;
  }
  const char *d = raw + 9;
  bool digits = d[0] >= '1' && d[0] <= '9' && d[1] >= '0' && d[1] <= '9' &&
                d[2] >= '0' && d[2] <= '9';
  if (!digits || (d[3] != ' ' && d[3] != '\r' && d[3] != '\0')) {
    return -8;
  }
  return (d[0] - '0') * 100 + (d[1] - '0') * 10 + (d[2] - '0');
}

// http_final_response skips interim 1xx responses, which aren't the answer. 101 is
// final: nothing after it is HTTP.
const char *http_final_response(const char *raw) {
  const char *head = raw;
  for (;;) {
    int64_t status = http_parse_status_line(head);
    const char *sep = strstr(head, "\r\n\r\n");
    if (status < 100 || status >= 200 || status == 101 || !sep) {
      return head;
    }
    head = sep + 4;
  }
}

// Whether the status line and header lines hold nothing that could hide framing
// from the header scan: no stray control byte, folded line or space in a name.
static bool head_is_well_formed(const char *head, const char *sep) {
  const char *eol = strstr(head, "\r\n");
  if (has_control_byte(head, eol)) {
    return false;
  }
  while (eol != sep) {
    const char *line = eol + 2;
    eol = strstr(line, "\r\n");
    const char *colon = memchr(line, ':', (size_t)(eol - line));
    if (!colon || colon == line || has_control_byte(line, eol) ||
        memchr(line, ' ', (size_t)(colon - line)) ||
        memchr(line, '\t', (size_t)(colon - line))) {
      return false;
    }
  }
  return true;
}

// Copies the header block between the status line and the blank line. Returns
// false when out of memory; a response with no headers has NULL ones.
bool http_copy_response_headers(const char *head, const char *sep, char **out) {
  const char *first_eol = strstr(head, "\r\n");
  *out = NULL;
  if (first_eol == sep) {
    return true;
  }
  size_t hlen = (size_t)(sep - (first_eol + 2));
  *out = malloc(hlen + 1);
  if (!*out) {
    return false;
  }
  memcpy(*out, first_eol + 2, hlen);
  (*out)[hlen] = '\0';
  return true;
}

// Parses one Content-Length value: digits, then optional padding. False when it is
// anything else, or too big to be a length.
static bool parse_content_length(const char *value, const char *end, size_t *out) {
  while (value < end && (*value == ' ' || *value == '\t')) {
    value++;
  }
  while (end > value && (end[-1] == ' ' || end[-1] == '\t')) {
    end--;
  }
  *out = 0;
  for (const char *p = value; p < end; p++) {
    size_t digit = (size_t)(*p - '0');
    if (*p < '0' || *p > '9' || *out > (SIZE_MAX - digit) / 10) {
      return false;
    }
    *out = *out * 10 + digit;
  }
  return value < end;
}

// Reads the Content-Length lines of a header block. False when one is malformed or
// they disagree; otherwise *declared says whether there was one, of *length.
static bool declared_length(const char *headers, bool *declared, size_t *length) {
  static const size_t name_len = 15;
  *declared = false;
  for (const char *line = headers; line && *line;) {
    const char *eol = strstr(line, "\r\n");
    if (strncasecmp(line, "Content-Length:", name_len) == 0) {
      const char *value = line + name_len;
      size_t parsed = 0;
      if (!parse_content_length(value, eol ? eol : value + strlen(value), &parsed) ||
          (*declared && parsed != *length)) {
        return false;
      }
      *declared = true;
      *length = parsed;
    }
    line = eol ? eol + 2 : NULL;
  }
  return true;
}

// Copies a body framed by its Content-Length or by the connection's close. Returns
// NULL when out of memory or, with *malformed set, when the framing is wrong.
static char *plain_body(const char *headers, const char *body, size_t body_len,
                        const char **malformed) {
  bool declared = false;
  size_t length = body_len;
  if (!declared_length(headers, &declared, &length)) {
    *malformed = "invalid Content-Length";
    return NULL;
  }
  if (declared && length > body_len) {
    *malformed = CHUNK_CUT_SHORT;
    return NULL;
  }
  char *out = malloc(length + 1);
  if (out) {
    memcpy(out, body, length);
    out[length] = '\0';
  }
  return out;
}

// 204, 304 and 101 end at their blank line, whatever their headers say (RFC 9112
// 6.3). Interim responses never get this far.
static bool status_has_no_body(int64_t status) {
  return status == 204 || status == 304 || status == 101;
}

// Produces the retained body, or a negative code with the reason on the channel.
int64_t http_response_body_of(int64_t status, const char *headers, const char *body,
                             size_t body_len, char **out) {
  const char *malformed = NULL;
  const char *subject = "body";
  if (status_has_no_body(status)) {
    *out = strdup("");
  } else if (http_header_is_chunked(headers)) {
    subject = "chunked body";
    *out = http_decode_chunked(body, body_len, &malformed);
  } else {
    *out = plain_body(headers, body, body_len, &malformed);
  }
  if (malformed) {
    osp_io_error_text("http response", subject, malformed);
    return -14; // broken framing is never shown to the caller as a body
  }
  return *out ? 0 : -9;
}

// Refuses a response whose bytes can't be trusted as far as its body, and returns
// where its final response's head is, with its blank line in *sep. NULL with *rc.
const char *http_checked_head(const char *raw, size_t len, const char **sep,
                                int64_t *status, int64_t *rc) {
  const char *reason = NULL;
  const char *subject = "head";
  const char *head = http_final_response(raw);
  *rc = -14;
  *status = http_parse_status_line(head);
  *sep = strstr(head, "\r\n\r\n");
  if (strlen(raw) != len) {
    subject = "NUL byte";
    reason = "a string cannot hold it";
  } else if (*status < 0) {
    *rc = -8;
    subject = "status line";
    reason = "not HTTP/1.x";
  } else if (!*sep) {
    reason = "truncated";
  } else if (!head_is_well_formed(head, *sep)) {
    reason = "malformed";
  }
  if (reason) {
    osp_io_error_text("http response", subject, reason);
    return NULL;
  }
  return head;
}
