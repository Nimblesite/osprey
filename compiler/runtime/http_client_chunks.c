#include "http_client_internal.h"
#include "http_shared.h"

// Chunked framing, split from the rest of a response's framing
// (http_client_framing.c) so each TU stays under the repo size budget. Everything
// here treats the bytes as untrusted. Implements [HTTP-CHUNKED-DECODE].

static const char CHUNK_TOO_BIG[] = "chunk size exceeds the remaining body";
static const char CHUNK_BAD_SIZE[] = "invalid chunk size";
static const char CHUNK_NO_CRLF[] = "chunk data not followed by CRLF";
static const char CHUNK_CUT_SHORT[] = "truncated";
static const char CHUNK_BAD_TRAILER[] = "invalid trailer";

const char *http_find_crlf(const char *p, const char *end) {
  for (; end - p >= 2; p++) {
    if (p[0] == '\r' && p[1] == '\n') {
      return p;
    }
  }
  return NULL;
}

bool http_has_control_byte(const char *p, const char *end) {
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
  const char *line_end = http_find_crlf(p, end);
  if (!line_end) {
    return *p == ';' || end - p == 1 ? CHUNK_CUT_SHORT : CHUNK_BAD_SIZE;
  }
  if (http_has_control_byte(p, line_end)) {
    return CHUNK_BAD_SIZE; // a bare CR or LF here is how smuggled framing hides
  }
  *cursor = line_end + 2;
  return NULL;
}

// Decodes every chunk of [p, end) into out, which holds end - p + 1 bytes: the
// decoded bytes can't outnumber the framed ones. The body ends at the blank line
// after the zero chunk's trailers. Returns NULL, or why the framing is malformed.
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
    const char *line_end = http_find_crlf(p, end);
    if (!line_end) {
      return CHUNK_CUT_SHORT;
    }
    if (line_end == p) {
      return NULL;
    }
    if (http_has_control_byte(p, line_end)) {
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

// Finds the end of the line at w->pos, picking up where the last look stopped, so a
// line that arrives a little at a time costs one pass over it.
static const char *next_line(ChunkWalk *w, const char *raw, const char *end) {
  size_t from = w->searched > w->pos ? w->searched : w->pos;
  const char *eol = http_find_crlf(raw + from, end);
  if (!eol) {
    w->searched = (size_t)(end - raw) - 1; // a CR at the end may yet start a CRLF
  }
  return eol;
}

// One line of chunk size or trailer. True when it settles the end of the framing.
static bool walk_line(ChunkWalk *w, const char *raw, const char *end, const char *eol,
                      size_t *frame_end) {
  const char *p = raw + w->pos;
  if (w->in_trailers) {
    if (eol == p || http_has_control_byte(p, eol)) {
      *frame_end = eol == p ? (size_t)(eol + 2 - raw) : (size_t)(end - raw);
      return true;
    }
    w->pos = (size_t)(eol + 2 - raw);
    return false;
  }
  if (parse_chunk_size(&p, end, &w->size)) {
    *frame_end = (size_t)(end - raw);
    return true;
  }
  w->pos = (size_t)(p - raw);
  w->in_trailers = w->size == 0;
  return false;
}

// http_chunks_complete says whether the framing has ended in raw[0, len), moving on
// from where the last call stopped: read a chunk at a time, a response of many chunks
// would otherwise be walked from its start after every read. Framing the decoder would
// refuse ends here too, at len, as no more bytes can change its answer. What it would
// call truncated doesn't, as more bytes can.
bool http_chunks_complete(ChunkWalk *w, const char *raw, size_t len, size_t *frame_end) {
  const char *end = raw + len;
  for (;;) {
    if (w->size > 0) {
      size_t have = len - w->pos;
      if (have < 2 || w->size > have - 2) {
        return false;
      }
      const char *after = raw + w->pos + w->size;
      if (after[0] != '\r' || after[1] != '\n') {
        *frame_end = len;
        return true;
      }
      w->pos += w->size + 2;
      w->size = 0;
    }
    const char *eol = next_line(w, raw, end);
    if (!eol) {
      return false;
    }
    if (walk_line(w, raw, end, eol, frame_end)) {
      return true;
    }
  }
}
