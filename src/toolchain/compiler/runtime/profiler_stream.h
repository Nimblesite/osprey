// Internal to the live-attach endpoint (docs/specs/tooling/0028-Profiler.md):
// profiler_endpoint.c owns the socket, its connections, the listener thread and
// the lifecycle [PROF-ATTACH-ENDPOINT]; profiler_stream.c owns the wire
// protocol and CPU sessions [PROF-ATTACH-PROTOCOL].
#ifndef OSPREY_PROFILER_STREAM_H
#define OSPREY_PROFILER_STREAM_H

#include "profiler_runtime.h"

#if defined(__APPLE__)
#include <TargetConditionals.h>
#endif

// Desktop POSIX only: a phone app cannot be attached to from a desktop
// language server, and Windows and Wasm have no profiler at all.
#if !defined(_WIN32) && !defined(__wasm__) && !defined(__ANDROID__) && \
    !(defined(__APPLE__) && TARGET_OS_IPHONE)
#define OSP_DIAG_ENDPOINT 1

enum { OSP_DIAG_LINE_MAX = 256 };

// One connection: its socket and the request bytes no newline has ended yet.
typedef struct DiagClient {
  int fd;
  size_t used;
  char line[OSP_DIAG_LINE_MAX];
} DiagClient;

// Run every complete line `c` holds; false when the connection must close.
bool osp_diag_take_lines(DiagClient *c);
void osp_diag_reply_error(int fd, const char *code, const char *message);
// Connection `fd` is going away: end the session it owned, sending nothing.
void osp_diag_forget(int fd);
// The endpoint is closing: end any session as `exit`, sending its last batch.
void osp_diag_finish(void);

#endif // OSP_DIAG_ENDPOINT

#endif // OSPREY_PROFILER_STREAM_H
