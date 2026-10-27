#ifndef HTTP_CLIENT_INTERNAL_H
#define HTTP_CLIENT_INTERNAL_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

// Private interface shared by the client transport TU
// (http_client_runtime.c) and the generic response-handle TU
// (http_client_response.c). Not part of the Osprey-visible builtin surface.

// Performs the request over http:// or https:// and returns the ENTIRE raw
// response in a freshly allocated, NUL-terminated buffer (caller frees
// *out_raw). Returns 0 on success or a negative error code.
int64_t http_perform(int64_t client_id, int64_t method, char *path,
                     char *headers, char *body, char **out_raw,
                     size_t *out_len);

// Returns the numeric status from an HTTP status line, or -8.
int64_t http_parse_status_line(const char *raw);

// Reports whether the response header block declares
// Transfer-Encoding: chunked (case-insensitive header name).
bool http_header_is_chunked(const char *headers);

// Decodes an HTTP/1.1 chunked body: returns a freshly allocated,
// NUL-terminated decoded body (caller frees), or NULL.
char *http_decode_chunked(const char *body);

#endif // HTTP_CLIENT_INTERNAL_H
