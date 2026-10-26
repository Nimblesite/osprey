// Byte streams: the one stateful model of the mock libc [SYMEX-MOCK-STREAM].
// stdin and every file opened for reading hold symbolic bytes and a symbolic
// length (`stdin`, `stdin.len`, `file.data:<path>`, `file.len:<path>`),
// plus a cursor. Opening a file forks on `file.ok:<path>`, so the
// open-failure branch is explored too. stdout, stderr and files opened for
// writing forward their bytes to `osp_hunt_sink`.
#include "hunt_mock.h"

#include <errno.h>
#include <klee/klee.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int osp_hunt_errno;
unsigned osp_hunt_calls[OSP_HUNT_CALL_COUNT];
__attribute__((weak)) long osp_hunt_input_bytes = 64;

void osp_hunt_note(osp_hunt_call call) { osp_hunt_calls[call]++; }

static void discard(const char *data, size_t count) {
  (void)data;
  (void)count;
}
void (*osp_hunt_sink)(const char *data, size_t count) = discard;

static struct osp_hunt_file standard[3] = {
    {"stdin", NULL, -1, 0, 1, 0, 1, 0, 0},
    {"stdout", NULL, 0, 0, 0, 1, 1, 0, 0},
    {"stderr", NULL, 0, 0, 0, 1, 1, 0, 0},
};
static struct osp_hunt_file files[OSP_HUNT_FILE_SLOTS];
static int file_protocol_named;
static char *initial_input;
static size_t initial_capacity;

size_t osp_hunt_input_capacity(const char *text) {
  if (!text || text != initial_input) return 0;
  size_t physical = klee_get_obj_size(text);
  return physical < initial_capacity ? physical : initial_capacity;
}

FILE *osp_hunt_stdin = &standard[0];
FILE *osp_hunt_stdout = &standard[1];
FILE *osp_hunt_stderr = &standard[2];

static void forget(struct osp_hunt_file *stream) {
  if (stream->readable && stream->data != NULL) {
    free(stream->data);
  }
  stream->data = NULL;
  stream->len = stream->readable ? -1 : 0;
  stream->pos = 0;
  stream->in_use = stream == standard || stream == standard + 1 || stream == standard + 2;
  stream->error = 0;
  stream->eof = 0;
}

void osp_hunt_reset(void) {
  for (int i = 0; i < 3; i++) {
    forget(standard + i);
  }
  for (int i = 0; i < OSP_HUNT_FILE_SLOTS; i++) {
    forget(files + i);
  }
  memset(osp_hunt_calls, 0, sizeof(osp_hunt_calls));
  osp_hunt_errno = 0;
  file_protocol_named = 0;
  initial_input = NULL;
  initial_capacity = 0;
}

static long stream_cap(void) {
  long cap = osp_hunt_input_bytes;
  if (cap < 0) {
    return 0;
  }
  return cap > OSP_HUNT_STREAM_MAX ? OSP_HUNT_STREAM_MAX : cap;
}

// Implements [SYMEX-MOCK-READ-BUFFER]. Reuse the stream's clamped bound.
size_t osp_hunt_file_buffer_capacity(void) { return (size_t)stream_cap() + 2; }

// First use of a readable stream: its bytes and length become symbolic. The
// bytes are their own allocation because KLEE makes whole objects symbolic.
static void materialise(struct osp_hunt_file *stream) {
  if (stream->len >= 0) {
    return;
  }
  long cap = stream_cap();
  char name[OSP_HUNT_NAME_MAX];
  osp_hunt_name(name, stream == standard ? stream->name : "file.len:",
                stream == standard ? ".len" : stream->name);
  stream->data = cap > 0 ? malloc((size_t)cap) : NULL;
  if (stream->data != NULL) {
    char data_name[OSP_HUNT_NAME_MAX];
    osp_hunt_name(data_name, stream == standard ? "" : "file.data:", stream->name);
    klee_make_symbolic(stream->data, (size_t)cap, data_name);
  }
  stream->len = stream->data == NULL ? 0 : klee_range(0, (int)cap + 1, name);
}

int osp_hunt_valid_line_end(const unsigned char *data, long length, long end, long cap) {
  int valid = (end >= 0) & (end <= length) & (length >= 0) & (length <= cap);
  for (long i = 0; i < cap; i++) {
    valid &= ((i >= end) | (data[i] != '\n')) &
             ((i != end) | (end == length) | (data[i] == '\n'));
  }
  return valid;
}

// Each output byte depends only on its prefix, not a stream-wide end witness.
static long copy_initial_line(char *out, const struct osp_hunt_file *stream, long cap) {
  long end = stream->len;
  int stopped = 0;
  for (long i = 0; i < cap; i++) {
    int stop = !stopped & ((i == stream->len) | (stream->data[i] == '\n'));
    volatile uintptr_t mask = (uintptr_t)0 - (uintptr_t)stop;
    out[i] = (char)(stream->data[i] & ~(unsigned char)mask);
    end = (long)(((uintptr_t)i & mask) | ((uintptr_t)end & ~mask));
    stopped |= stop;
  }
  out[cap] = '\0';
  return end;
}

static char *initial_line(struct osp_hunt_file *stream, long cap) {
  char *out = malloc((size_t)cap + 1);
  if (out == NULL) {
    return NULL;
  }
  initial_input = out;
  initial_capacity = (size_t)cap + 1;
  long end = copy_initial_line(out, stream, cap);
  stream->pos = end + (end < stream->len);
  stream->eof = (unsigned char)(end == stream->len);
  osp_hunt_calls[OSP_HUNT_CALL_READ] += (unsigned)end + 1;
  (void)fflush(osp_hunt_stdout);
  return out;
}

// Later or interleaved reads keep the ordinary byte-stream implementation.
int osp_hunt_initial_input(char **out) {
  struct osp_hunt_file *stream = osp_hunt_stdin;
  if (stream->pos != 0 || !stream->readable) {
    return 0;
  }
  materialise(stream);
  *out = initial_line(stream, stream_cap());
  return 1;
}

// Copy up to `want` bytes from the cursor; a short copy marks end of stream.
static long take(struct osp_hunt_file *stream, unsigned char *out, long want) {
  if (!stream->readable) {
    stream->error = 1;
    errno = EBADF;
    return 0;
  }
  materialise(stream);
  long available = stream->len - stream->pos;
  long count = want < available ? want : available;
  if (count > 0) {
    memcpy(out, stream->data + stream->pos, (size_t)count);
  }
  stream->pos += count;
  if (count < want) {
    stream->eof = 1;
  }
  return count;
}

static size_t emit(struct osp_hunt_file *stream, const char *data, size_t count) {
  if (!stream->writable) {
    stream->error = 1;
    errno = EBADF;
    return 0;
  }
  osp_hunt_sink(data, count);
  return count;
}

ssize_t read(int fd, void *out, size_t count) {
  osp_hunt_note(OSP_HUNT_CALL_READ);
  if (fd != STDIN_FILENO) {
    errno = EBADF;
    return -1;
  }
  return take(osp_hunt_stdin, out, (long)count);
}

ssize_t write(int fd, const void *data, size_t count) {
  if (fd != STDOUT_FILENO && fd != STDERR_FILENO) {
    errno = EBADF;
    return -1;
  }
  return (ssize_t)emit(osp_hunt_stdout, data, count);
}

int close(int fd) {
  (void)fd;
  return 0;
}

int isatty(int fd) {
  (void)fd;
  return 0;
}

int fgetc(FILE *stream) {
  unsigned char byte;
  return take(stream, &byte, 1) == 1 ? byte : EOF;
}

int getc(FILE *stream) { return fgetc(stream); }

int getchar(void) {
  osp_hunt_note(OSP_HUNT_CALL_GETCHAR);
  return fgetc(osp_hunt_stdin);
}

char *fgets(char *out, int cap, FILE *stream) {
  osp_hunt_note(OSP_HUNT_CALL_FGETS);
  int length = 0;
  while (length + 1 < cap) {
    int byte = fgetc(stream);
    if (byte == EOF) {
      break;
    }
    out[length++] = (char)byte;
    if (byte == '\n') {
      break;
    }
  }
  if (length == 0) {
    return NULL;
  }
  out[length] = '\0';
  return out;
}

size_t fread(void *out, size_t size, size_t count, FILE *stream) {
  osp_hunt_note(OSP_HUNT_CALL_FREAD);
  if (size == 0) {
    return 0;
  }
  long bytes = take(stream, out, (long)(size * count));
  return (size_t)bytes / size;
}

size_t fwrite(const void *data, size_t size, size_t count, FILE *stream) {
  osp_hunt_note(OSP_HUNT_CALL_FWRITE);
  if (size == 0) {
    return 0;
  }
  return emit(stream, data, size * count) / size;
}

int fputs(const char *text, FILE *stream) {
  return emit(stream, text, strlen(text)) == strlen(text) ? 0 : EOF;
}

int puts(const char *text) {
  osp_hunt_note(OSP_HUNT_CALL_PRINTF);
  emit(osp_hunt_stdout, text, strlen(text));
  return emit(osp_hunt_stdout, "\n", 1) == 1 ? 0 : EOF;
}

int fputc(int byte, FILE *stream) {
  char text = (char)byte;
  return emit(stream, &text, 1) == 1 ? (unsigned char)byte : EOF;
}

int putc(int byte, FILE *stream) { return fputc(byte, stream); }

int putchar(int byte) { return fputc(byte, osp_hunt_stdout); }

static struct osp_hunt_file *free_slot(void) {
  for (int i = 0; i < OSP_HUNT_FILE_SLOTS; i++) {
    if (!files[i].in_use) {
      return &files[i];
    }
  }
  return NULL;
}

// Whether this open succeeds is a fresh choice named `file.ok:<path>`.
static void name_file_protocol(void) {
  if (file_protocol_named) return;
  int version = OSP_HUNT_FILE_PROTOCOL_VERSION;
  klee_make_symbolic(&version, sizeof(version), OSP_HUNT_FILE_PROTOCOL_NAME);
  klee_assume(version == OSP_HUNT_FILE_PROTOCOL_VERSION);
  file_protocol_named = 1;
}

static int opens(const char *path) {
  name_file_protocol();
  char name[OSP_HUNT_NAME_MAX];
  osp_hunt_name(name, "file.ok:", path);
  return klee_range(0, 2, name);
}

FILE *fopen(const char *path, const char *mode) {
  osp_hunt_note(OSP_HUNT_CALL_FOPEN);
  struct osp_hunt_file *stream = free_slot();
  if (stream == NULL) {
    errno = ENFILE;
    return NULL;
  }
  if (!opens(path)) {
    errno = ENOENT;
    return NULL;
  }
  osp_hunt_name(stream->name, path, "");
  stream->readable = mode[0] == 'r';
  stream->writable = !stream->readable || strchr(mode, '+') != NULL;
  stream->len = stream->readable ? -1 : 0;
  stream->pos = 0;
  stream->in_use = 1;
  stream->error = 0;
  stream->eof = 0;
  return stream;
}

int fclose(FILE *stream) {
  osp_hunt_note(OSP_HUNT_CALL_FCLOSE);
  stream->in_use = 0;
  return 0;
}

int fflush(FILE *stream) {
  (void)stream;
  return 0;
}

int ferror(FILE *stream) { return stream->error; }

int feof(FILE *stream) { return stream->eof; }

void clearerr(FILE *stream) {
  stream->error = 0;
  stream->eof = 0;
}

int fseeko(FILE *stream, off_t offset, int whence) {
  if (!stream->readable) {
    return 0;
  }
  materialise(stream);
  long base = whence == SEEK_SET ? 0 : whence == SEEK_CUR ? stream->pos : stream->len;
  long target = base + offset;
  if (target < 0 || target > stream->len) {
    errno = EINVAL;
    return -1;
  }
  stream->pos = target;
  stream->eof = 0;
  return 0;
}

int fseek(FILE *stream, long offset, int whence) { return fseeko(stream, offset, whence); }

off_t ftello(FILE *stream) { return stream->pos; }

long ftell(FILE *stream) { return stream->pos; }

void rewind(FILE *stream) { (void)fseeko(stream, 0, SEEK_SET); }

int setvbuf(FILE *stream, char *buffer, int mode, size_t size) {
  (void)stream;
  (void)buffer;
  (void)mode;
  (void)size;
  return 0;
}

// Deleting or renaming a path has nothing to act on: streams are named, not
// stored, so both succeed [SYMEX-MOCK-STATELESS].
int remove(const char *path) {
  (void)path;
  return 0;
}

int rename(const char *from, const char *to) {
  (void)from;
  (void)to;
  return 0;
}
