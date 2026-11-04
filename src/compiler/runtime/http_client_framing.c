#include "http_client_internal.h"
#include "http_shared.h"
#include "io_error.h"

#include <strings.h> // strncasecmp

// What a response's bytes mean, split from the response handles
// (http_client_response.c) so each TU stays under the repo size budget: its status
// line, its head and how its body is framed. Everything here treats the bytes as
// untrusted. Implements [HTTP-RESPONSE-FRAMING] and [HTTP-CHUNKED-DECODE].

// What the Transfer-Encoding lines of a header block leave as the final coding.
typedef enum { CODING_NONE, CODING_OTHER, CODING_CHUNKED } TransferCoding;

// The last coding of one Transfer-Encoding value, as a whole word. Empty list
// elements name no coding (RFC 9110 5.6.1), so a value of only those changes
// nothing. value - 1 is always the header's colon, so looking back stays in bounds.
static TransferCoding last_coding(const char *value, const char *end,
                                  TransferCoding previous) {
  static const size_t coding_len = 7;
  while (end > value && (end[-1] == ' ' || end[-1] == '\t' || end[-1] == ',')) {
    end--;
  }
  if (end == value) {
    return previous;
  }
  if ((size_t)(end - value) < coding_len ||
      strncasecmp(end - coding_len, "chunked", coding_len) != 0) {
    return CODING_OTHER;
  }
  char before = end[-(ptrdiff_t)coding_len - 1];
  bool whole_word = before == ':' || before == ' ' || before == '\t' || before == ',';
  return whole_word ? CODING_CHUNKED : CODING_OTHER;
}

// Repeated lines are one list, so the last coding on any of them decides. Any other
// header that merely mentions either word does not count. Lines end at CRLF or at
// stop, which is where the header block ends.
static TransferCoding final_coding(const char *lines, const char *stop) {
  static const size_t name_len = 18;
  TransferCoding coding = CODING_NONE;
  for (const char *line = lines; line && line < stop;) {
    const char *eol = strstr(line, "\r\n");
    if (strncasecmp(line, "Transfer-Encoding:", name_len) == 0) {
      coding = last_coding(line + name_len, eol ? eol : stop, coding);
    }
    line = eol ? eol + 2 : NULL;
  }
  return coding;
}

// The end of a NUL-terminated header block, which may be absent.
static const char *block_end(const char *headers) {
  return headers ? headers + strlen(headers) : NULL;
}

// http_header_is_chunked reports whether the response header block names chunked
// as the final coding of its Transfer-Encoding header (case-insensitive, per
// RFC 9112 6.1).
bool http_header_is_chunked(const char *headers) {
  return final_coding(headers, block_end(headers)) == CODING_CHUNKED;
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

// Whether the status line and header lines hold nothing that could hide framing
// from the header scan: no stray control byte, folded line or space in a name.
static bool head_is_well_formed(const char *head, const char *sep) {
  const char *eol = strstr(head, "\r\n");
  if (http_has_control_byte(head, eol)) {
    return false;
  }
  while (eol != sep) {
    const char *line = eol + 2;
    eol = strstr(line, "\r\n");
    const char *colon = memchr(line, ':', (size_t)(eol - line));
    if (!colon || colon == line || http_has_control_byte(line, eol) ||
        memchr(line, ' ', (size_t)(colon - line)) ||
        memchr(line, '\t', (size_t)(colon - line))) {
      return false;
    }
  }
  return true;
}

// Interim statuses are skipped; 101 is final, as nothing after it is HTTP.
static bool status_is_interim(int64_t status) {
  return status >= 100 && status < 200 && status != 101;
}

// RFC 9112 6.1: a coding on HTTP/1.0 is faulty framing, even with a Content-Length.
static bool coding_on_http_1_0(const char *head, const char *sep) {
  return strncmp(head, "HTTP/1.0 ", 9) == 0 &&
         final_coding(strstr(head, "\r\n") + 2, sep) != CODING_NONE;
}

// http_final_response skips interim 1xx responses, which aren't the answer. 101 is
// final: nothing after it is HTTP. A malformed interim head is returned for the
// caller to refuse, as skipping it would take a later blank line for its end.
const char *http_final_response(const char *raw) {
  const char *head = raw;
  for (;;) {
    int64_t status = http_parse_status_line(head);
    const char *sep = strstr(head, "\r\n\r\n");
    if (!status_is_interim(status) || !sep || !head_is_well_formed(head, sep)) {
      return head;
    }
    head = sep + 4;
  }
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
static bool declared_length(const char *lines, const char *stop, bool *declared,
                            size_t *length) {
  static const size_t name_len = 15;
  *declared = false;
  for (const char *line = lines; line && line < stop;) {
    const char *eol = strstr(line, "\r\n");
    if (strncasecmp(line, "Content-Length:", name_len) == 0) {
      size_t parsed = 0;
      if (!parse_content_length(line + name_len, eol ? eol : stop, &parsed) ||
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

// Copies a body framed by its Content-Length or by the connection's close. Any
// coding overrides the length (RFC 9112 6.3). A body framed by the close needs to
// have seen it: a read that failed proves nothing but where the bytes stopped.
// Returns NULL when out of memory or, with *malformed set, when the framing is wrong.
static char *plain_body(const char *headers, const char *body, size_t body_len,
                        bool closed, const char **malformed) {
  bool declared = false;
  size_t length = body_len;
  const char *stop = block_end(headers);
  if (final_coding(headers, stop) == CODING_NONE &&
      !declared_length(headers, stop, &declared, &length)) {
    *malformed = "invalid Content-Length";
    return NULL;
  }
  if (declared ? length > body_len : !closed) {
    *malformed = "truncated";
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
                              size_t body_len, bool closed, char **out) {
  const char *malformed = NULL;
  const char *subject = "body";
  if (status_has_no_body(status)) {
    *out = strdup("");
  } else if (http_header_is_chunked(headers)) {
    subject = "chunked body";
    *out = http_decode_chunked(body, body_len, &malformed);
  } else {
    *out = plain_body(headers, body, body_len, closed, &malformed);
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
  } else if (coding_on_http_1_0(head, *sep)) {
    reason = "Transfer-Encoding on HTTP/1.0";
  }
  if (reason) {
    osp_io_error_text("http response", subject, reason);
    return NULL;
  }
  return head;
}

// head_is_well_formed scans with strstr, so a NUL must rule first.
static bool head_is_sound(const char *head, const char *sep) {
  return !memchr(head, '\0', (size_t)(sep - head)) && head_is_well_formed(head, sep);
}

static const char *find_blank_line(const char *p, const char *end) {
  for (p = http_find_crlf(p, end); p; p = http_find_crlf(p + 2, end)) {
    if (end - p >= 4 && p[2] == '\r' && p[3] == '\n') {
      return p;
    }
  }
  return NULL;
}

// How the final head at [head, sep) frames what follows it. Whatever the client is
// going to refuse ends the read where it stands, at len.
static void settle_framing(HttpFrameScan *scan, const char *raw, size_t len,
                           const char *head, const char *sep) {
  size_t body = (size_t)(sep + 4 - raw);
  bool declared = false;
  size_t length = 0;
  scan->phase = FRAME_LENGTH;
  scan->frame_end = len;
  if (http_parse_status_line(head) < 0 || !head_is_sound(head, sep) ||
      coding_on_http_1_0(head, sep)) {
    return;
  }
  const char *lines = strstr(head, "\r\n") + 2;
  TransferCoding coding = final_coding(lines, sep);
  if (status_has_no_body(http_parse_status_line(head))) {
    scan->frame_end = body;
  } else if (coding == CODING_CHUNKED) {
    scan->phase = FRAME_CHUNKS;
    scan->chunks.pos = body;
  } else if (coding == CODING_OTHER) {
    scan->phase = FRAME_CLOSE;
  } else if (declared_length(lines, sep, &declared, &length)) {
    bool fits = declared && length <= SIZE_MAX - body;
    scan->phase = fits ? FRAME_LENGTH : FRAME_CLOSE;
    scan->frame_end = fits ? body + length : len;
  }
}

// Moves past each head that has arrived, picking up where the last look stopped.
static void scan_heads(HttpFrameScan *scan, const char *raw, size_t len) {
  while (scan->phase == FRAME_HEAD) {
    const char *head = raw + scan->head;
    size_t from = scan->searched > scan->head ? scan->searched : scan->head;
    const char *sep = find_blank_line(raw + from, raw + len);
    if (!sep) {
      scan->searched = len > 3 ? len - 3 : 0; // a blank line may be arriving in pieces
      return;
    }
    if (status_is_interim(http_parse_status_line(head)) && head_is_sound(head, sep)) {
      scan->head = (size_t)(sep + 4 - raw);
    } else {
      settle_framing(scan, raw, len, head, sep);
    }
  }
}

// http_frame_complete says whether raw[0, len), NUL-terminated, holds the end of the
// response's framing, and where. Call it after every read with the same scan: it
// moves on from where it stopped, so the cost of a response is linear in its size
// however it is cut up. Implements [HTTP-RESPONSE-FRAMING].
bool http_frame_complete(HttpFrameScan *scan, const char *raw, size_t len, size_t *frame_end) {
  scan_heads(scan, raw, len);
  if (scan->phase == FRAME_CHUNKS) {
    return http_chunks_complete(&scan->chunks, raw, len, frame_end);
  }
  if (scan->phase == FRAME_LENGTH && len >= scan->frame_end) {
    *frame_end = scan->frame_end;
    return true;
  }
  return false;
}
