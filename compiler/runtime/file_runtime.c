// Portable file I/O for `readFile` / `writeFile`, and the thread-local failure
// channel every runtime entry point reports through (io_error.h).
//
// Split out of system_runtime.c, whose other half is the fork/exec process
// runtime that wasm32-wasip1 cannot have. This unit is portable — WASI supplies
// fopen/fread/fwrite — so it is in every archive, native and wasm.
//
// Two rules govern the code below, and both were learned from defects this file
// exists to close:
//
//   1. A LENGTH COMES FROM WHAT WAS READ, NEVER FROM A SEEK. fseek/ftell are
//      entitled to fail on any non-seekable stream (a FIFO, a socket, a
//      character device), and ftell reports -1 when they do.
//   2. A WRITE IS NOT DONE UNTIL THE FLUSH SUCCEEDS. stdio hands bytes to the
//      OS at fclose, so that is where ENOSPC, EPIPE and EIO appear.
//
// Implements [BUILTIN-FILE], [BUILTIN-FILE-ERRMSG].

#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "io_error.h"

#ifdef _WIN32
#include "osprey_win_compat.h"
#endif

// --- the failure channel ----------------------------------------------------

// Long enough for an operation, a filesystem path and a strerror sentence.
#define OSP_IO_ERROR_MAX 512
#define OSP_IO_REASON_MAX 128

// Thread-local, exactly like errno: a fiber, a coroutine body and the HTTP
// server thread each keep their own, so one thread's failure can never be
// read as another's.
static __thread char io_error_text[OSP_IO_ERROR_MAX];
static __thread int io_error_present;

void osp_io_error_clear(void) {
  io_error_present = 0;
  io_error_text[0] = '\0';
}

// strerror() may hand back a shared static buffer, and this runtime calls it
// from several threads at once, so every platform uses its re-entrant form —
// except wasm32-wasip1, whose libc has no `strerror_r` at all and whose
// programs have no second thread to race with. Copying the message out
// immediately keeps even that branch's result private to `out`.
static void describe_errno(int err, char *out, size_t out_size) {
#if defined(_WIN32)
  if (strerror_s(out, out_size, err) != 0) {
    (void)snprintf(out, out_size, "errno %d", err);
  }
#elif defined(__wasm__)
  (void)snprintf(out, out_size, "%s", strerror(err));
#elif defined(__GLIBC__) && defined(_GNU_SOURCE)
  // The GNU form returns the message, which may or may not be `out`.
  const char *message = strerror_r(err, out, out_size);
  if (message != out) {
    (void)snprintf(out, out_size, "%s", message);
  }
#else
  if (strerror_r(err, out, out_size) != 0) {
    (void)snprintf(out, out_size, "errno %d", err);
  }
#endif
}

void osp_io_error_set(const char *op, const char *subject, int err) {
  char reason[OSP_IO_REASON_MAX];
  if (err != 0) {
    describe_errno(err, reason, sizeof(reason));
  } else {
    (void)snprintf(reason, sizeof(reason), "unspecified failure");
  }
  (void)snprintf(io_error_text, sizeof(io_error_text), "%s: %s: %s",
                 op != NULL ? op : "io",
                 subject != NULL ? subject : "<null path>", reason);
  io_error_present = 1;
}

const char *osp_io_error(void) {
  return io_error_present ? io_error_text : NULL;
}

char *osp_io_error_take(void) {
  return io_error_present ? strdup(io_error_text) : NULL;
}

// --- reading ----------------------------------------------------------------

// Starting capacity, then doubling. Sized so ordinary source and config files
// are read in a single fread with no reallocation.
#define FILE_READ_CHUNK_BYTES 65536

// Double `*cap`, or report failure. The NUL terminator lives in the capacity,
// so the caller's usable room is always `*cap - 1`.
static char *grow_buffer(char *buf, size_t *cap) {
  if (*cap > SIZE_MAX / 2) {
    return NULL;
  }
  size_t bigger = *cap * 2;
  char *grown = realloc(buf, bigger);
  if (grown != NULL) {
    *cap = bigger;
  }
  return grown;
}

// Drain `file` to a NUL-terminated heap buffer, or NULL with the channel set.
// The loop stops on a short fread — end of stream or error, told apart by
// ferror afterwards — so the byte count is whatever the stream actually
// produced and never a seek's opinion of it.
static char *read_stream(FILE *file, const char *filename) {
  size_t cap = FILE_READ_CHUNK_BYTES;
  size_t len = 0;
  char *content = malloc(cap);
  if (content == NULL) {
    osp_io_error_set("readFile", filename, ENOMEM);
    return NULL;
  }
  for (;;) {
    if (len + 1 == cap) {
      char *grown = grow_buffer(content, &cap);
      if (grown == NULL) {
        free(content);
        osp_io_error_set("readFile", filename, ENOMEM);
        return NULL;
      }
      content = grown;
    }
    size_t room = cap - 1 - len;
    size_t got = fread(content + len, 1, room, file);
    len += got;
    if (got < room) {
      break;
    }
  }
  if (ferror(file)) {
    int err = errno;
    free(content);
    osp_io_error_set("readFile", filename, err);
    return NULL;
  }
  content[len] = '\0';
  return content;
}

// Read a whole file. Returns a heap buffer the caller owns, or NULL with the
// reason on the channel. Implements [BUILTIN-FILE].
char *read_file(char *filename) {
  osp_io_error_clear();
  if (filename == NULL) {
    osp_io_error_set("readFile", NULL, EINVAL);
    return NULL;
  }
  FILE *file = fopen(filename, "r");
  if (file == NULL) {
    osp_io_error_set("readFile", filename, errno);
    return NULL;
  }
  char *content = read_stream(file, filename);
  (void)fclose(file);
  return content;
}

// --- writing ----------------------------------------------------------------

// Write `content` over `filename`, returning the byte count written or a
// negative status with the reason on the channel. Every failure point is
// checked: a partial fwrite and a failed flush both mean the data is not on
// disk, and reporting either as success is silent data loss.
// Implements [BUILTIN-FILE].
int64_t write_file(char *filename, char *content) {
  osp_io_error_clear();
  if (filename == NULL || content == NULL) {
    osp_io_error_set("writeFile", filename, EINVAL);
    return -1;
  }
  FILE *file = fopen(filename, "w");
  if (file == NULL) {
    osp_io_error_set("writeFile", filename, errno);
    return -2;
  }
  size_t want = strlen(content);
  size_t written = fwrite(content, 1, want, file);
  if (written != want) {
    int err = ferror(file) && errno != 0 ? errno : EIO;
    (void)fclose(file);
    osp_io_error_set("writeFile", filename, err);
    return -3;
  }
  // The bytes reach the OS here, not at fwrite, so this is where a full disk
  // or a hung-up pipe reports itself. Dropping this status announces a
  // successful write of data that was never stored.
  if (fclose(file) != 0) {
    osp_io_error_set("writeFile", filename, errno);
    return -4;
  }
  return (int64_t)written;
}
