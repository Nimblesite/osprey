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

#include "collection_runtime.h"
#include "io_error.h"
#include "memory_hooks.h"
#include "osprey_path.h"
#include "string_runtime.h"
#if !defined(__wasm__) && !defined(OSP_SYMBOLIC) && !defined(_WIN32)
#include <dirent.h>
#include <sys/stat.h>
#endif
#ifdef OSP_SYMBOLIC
#include "klee/hunt_mock.h"
#endif

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
#elif defined(_GNU_SOURCE) && (defined(__GLIBC__) || \
    (defined(__ANDROID_API__) && __ANDROID_API__ >= 23))
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

void osp_io_error_text(const char *op, const char *subject, const char *reason) {
  (void)snprintf(io_error_text, sizeof(io_error_text), "%s: %s: %s",
                 op != NULL ? op : "io",
                 subject != NULL ? subject : "<null path>", reason);
  io_error_present = 1;
}

void osp_io_error_set(const char *op, const char *subject, int err) {
  char reason[OSP_IO_REASON_MAX];
  if (err != 0) {
    describe_errno(err, reason, sizeof(reason));
  } else {
    (void)snprintf(reason, sizeof(reason), "unspecified failure");
  }
  osp_io_error_text(op, subject, reason);
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
#ifdef OSP_SYMBOLIC
  // [SYMEX-MOCK-READ-BUFFER] A symbolic write into a 64 KiB allocation sends
  // the whole array to the solver. No modeled file exceeds its stream cap.
  size_t cap = osp_hunt_file_buffer_capacity();
#else
  size_t cap = FILE_READ_CHUNK_BYTES;
#endif
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
  // [PORT-STRING-NUL] Text cannot represent embedded NUL; never return its prefix.
  if (memchr(content, '\0', len) != NULL) {
    free(content);
    osp_io_error_text("readFile", filename, "text contains NUL; use a binary reader");
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

// ===== Central path joining, directory listing and path kinds ==============
//
// osp_path_child is THE child-path builder [BUILTIN-FILE-LISTDIR]: listDir, the
// media render's folder sweep, the profiler's socket sweep and the file
// server's static route all ask it instead of spelling "%s/%s", so the
// separator, trailing-separator collapse and truncation refusal have exactly
// one definition. Root stays "/x", never "//x".
size_t osp_path_child(char *out, size_t size, const char *dir, const char *name) {
  if (out == NULL || size == 0 || dir == NULL || name == NULL || name[0] == '\0') {
    return 0;
  }
  size_t end = strlen(dir);
  while (end > 0 && dir[end - 1] == OSP_PATH_SEP) {
    end--;
  }
  int wrote = snprintf(out, size, "%.*s%c%s", (int)end, dir, OSP_PATH_SEP, name);
  if (wrote <= 0 || (size_t)wrote >= size) {
    out[0] = '\0';
    return 0;
  }
  return (size_t)wrote;
}

static const char OP_LIST_DIR[] = "listDir";
static const char OP_KIND_OF[] = "kindOf";
static const char OP_MAKE_DIR[] = "makeDir";

#if !defined(__wasm__) && !defined(OSP_SYMBOLIC) && !defined(_WIN32)
// One directory entry's full path, with OSP_PATH_SEP appended when the entry
// is a directory. A symlink is never a directory here (lstat, not stat), so a
// link to an ancestor can never make a walker recurse forever. Returns 0 on
// success or the errno that refuses the entry.
static int entry_path(char *out, size_t size, const char *dir, const struct dirent *entry) {
  size_t length = osp_path_child(out, size, dir, entry->d_name);
  if (length == 0 || length + 1 >= size) {
    return ENAMETOOLONG;
  }
  int is_dir = entry->d_type == DT_DIR;
  struct stat info;
  if (entry->d_type == DT_UNKNOWN) {
    if (lstat(out, &info) != 0) {
      return errno;
    }
    is_dir = S_ISDIR(info.st_mode);
  }
  if (is_dir) {
    out[length] = OSP_PATH_SEP;
    out[length + 1] = '\0';
  }
  return 0;
}

static int is_dot_entry(const char *name) {
  return strcmp(name, ".") == 0 || strcmp(name, "..") == 0;
}

// Pushes every entry of `dir` onto `builder`; 0, or the errno that stopped
// the read. errno is cleared before each readdir so an I/O error at the end
// of the stream is never mistaken for end-of-directory.
static int push_entries(DIR *dir, const char *path, OspreyListBuilder *builder) {
  char joined[OSP_PATH_MAX];
  for (;;) {
    errno = 0;
    struct dirent *entry = readdir(dir);
    if (entry == NULL) {
      return errno;
    }
    if (is_dot_entry(entry->d_name)) {
      continue;
    }
    int failure = entry_path(joined, sizeof joined, path, entry);
    char *text = failure == 0 ? osp_host_string_copy(joined) : NULL;
    if (text == NULL) {
      return failure != 0 ? failure : ENOMEM;
    }
    osprey_list_builder_push_of(builder, (int64_t)(intptr_t)text, 1);
  }
}

// [BUILTIN-FILE-LISTDIR] The entries of one directory as a List<string> of
// full child paths, directories marked with a trailing separator so a walker
// recurses without a second syscall per entry; "." and ".." never appear. A
// failure anywhere is NULL with the reason recorded, never a partial list.
OspreyList *osprey_list_dir(const char *path) {
  if (path == NULL) {
    osp_io_error_text(OP_LIST_DIR, NULL, "no path");
    return NULL;
  }
  DIR *dir = opendir(path);
  if (dir == NULL) {
    osp_io_error_set(OP_LIST_DIR, path, errno);
    return NULL;
  }
  OspreyListBuilder *builder = osprey_list_builder_new_of(1);
  int failure = builder == NULL ? ENOMEM : push_entries(dir, path, builder);
  (void)closedir(dir);
  OspreyList *list = builder == NULL ? NULL : osprey_list_builder_seal(builder);
  if (failure != 0) {
    osp_release(list);
    osp_io_error_set(OP_LIST_DIR, path, failure);
    return NULL;
  }
  return list;
}

// [BUILTIN-FILE-KIND] "dir", "file" or "other" for one path, from stat — the
// one classification every walker shares. A path that cannot be stated is
// NULL with the reason recorded, never a guessed kind.
char *osprey_kind_of(const char *path) {
  struct stat info;
  if (path == NULL || stat(path, &info) != 0) {
    osp_io_error_set(OP_KIND_OF, path, path == NULL ? EINVAL : errno);
    return NULL;
  }
  const char *kind = S_ISDIR(info.st_mode) ? "dir" : S_ISREG(info.st_mode) ? "file" : "other";
  char *copy = osp_host_string_copy(kind);
  if (copy == NULL) {
    osp_io_error_set(OP_KIND_OF, path, ENOMEM);
  }
  return copy;
}

// [BUILTIN-FILE-MAKEDIR] One directory, created idempotently: an existing
// directory at `path` is Success. A failure (a file in the way, permission
// denied) is -1 with the reason recorded, never silence.
int64_t osprey_make_dir(const char *path) {
  if (path == NULL) {
    osp_io_error_set(OP_MAKE_DIR, NULL, EINVAL);
    return -1;
  }
  struct stat info;
  if (mkdir(path, 0777) == 0) {
    return 0;
  }
  int failure = errno;
  if (failure == EEXIST && stat(path, &info) == 0 && S_ISDIR(info.st_mode)) {
    return 0;
  }
  osp_io_error_set(OP_MAKE_DIR, path, failure == EEXIST ? ENOTDIR : failure);
  return -1;
}
#else
// wasm32 (no directory syscalls in this WASI profile), KLEE's bitcode libc
// (no dirent or mkdir: an unmodelled call) and Windows (no POSIX dirent yet)
// refuse every directory operation with the reason, never a fake answer.
static const char NO_DIRECTORIES[] = "no directory operations on this target";

OspreyList *osprey_list_dir(const char *path) {
  osp_io_error_text(OP_LIST_DIR, path, NO_DIRECTORIES);
  return NULL;
}

char *osprey_kind_of(const char *path) {
  osp_io_error_text(OP_KIND_OF, path, NO_DIRECTORIES);
  return NULL;
}

int64_t osprey_make_dir(const char *path) {
  osp_io_error_text(OP_MAKE_DIR, path, NO_DIRECTORIES);
  return -1;
}
#endif
