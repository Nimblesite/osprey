#ifndef HTTP_CLIENT_INTERNAL_H
#define HTTP_CLIENT_INTERNAL_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

// Private interface shared by the client transport TU
// (http_client_runtime.c) and the generic response-handle TU
// (http_client_response.c). Not part of the Osprey-visible builtin surface.

// What the client will read of one response. Implements [HTTP-RESPONSE-FRAMING].
#define HTTP_MAX_RESPONSE_BYTES ((size_t)64 * 1024 * 1024)
#define HTTP_READ_BOUND_FACTOR 10 // times the client's timeout: the whole read
#define HTTP_READ_BOUND_UNTIMED_MS ((int64_t)10 * 60 * 1000) // with a timeout of 0

// Where a walk over a chunked body has got to, within the whole response.
typedef struct {
  size_t pos;      // start of the line or chunk data awaited
  size_t searched; // how far the awaited line has been searched for its end
  size_t size;     // the chunk data awaited, when not 0
  bool in_trailers;
} ChunkWalk;

typedef enum { FRAME_HEAD, FRAME_CHUNKS, FRAME_LENGTH, FRAME_CLOSE } FramePhase;

// Where the search for the end of a response's framing has got to. Zero to start.
typedef struct {
  FramePhase phase;
  size_t head;      // start of the head awaited
  size_t searched;  // how far that head has been searched for its blank line
  size_t frame_end; // FRAME_LENGTH: where the response ends
  ChunkWalk chunks;
} HttpFrameScan;

bool http_frame_complete(HttpFrameScan *scan, const char *raw, size_t len, size_t *frame_end);
bool http_chunks_complete(ChunkWalk *walk, const char *raw, size_t len, size_t *frame_end);
const char *http_find_crlf(const char *p, const char *end);
bool http_has_control_byte(const char *p, const char *end);

// Performs the request over http:// or https:// and returns the raw response, up
// to the end of its framing, in a freshly allocated, NUL-terminated buffer (caller
// frees *out_raw). *out_closed says the peer closed in good order, not that a read
// failed or timed out, or that the framing ended first. Returns 0 on success or a
// negative error code.
int64_t http_perform(int64_t client_id, int64_t method, char *path,
                     char *headers, char *body, char **out_raw,
                     size_t *out_len, bool *out_closed);

// Returns the numeric status from an HTTP status line, or -8.
int64_t http_parse_status_line(const char *raw);
const char *http_final_response(const char *raw);
const char *http_checked_head(const char *raw, size_t len, const char **sep,
                              int64_t *status, int64_t *rc);
bool http_copy_response_headers(const char *head, const char *sep, char **out);
int64_t http_response_body_of(int64_t status, const char *headers, const char *body,
                              size_t body_len, bool closed, char **out);

// Reports whether the response header block declares
// Transfer-Encoding: chunked (case-insensitive header name).
bool http_header_is_chunked(const char *headers);

// Decodes the len bytes of an HTTP/1.1 chunked body: returns a freshly allocated,
// NUL-terminated decoded body (caller frees), or NULL with *malformed saying what
// is wrong with the framing, or NULL with *malformed NULL when out of memory.
char *http_decode_chunked(const char *body, size_t len, const char **malformed);

#endif // HTTP_CLIENT_INTERNAL_H
