// The safetensors reader (tensor_file.h) every tensor loader shares: one named
// tensor of a safetensors file — the Hugging Face interchange format, an
// 8-byte little-endian header length, a JSON header mapping each name to its
// dtype, shape and data_offsets, then the raw tensor bytes. It builds no
// container: tensor_list.c sinks the values into a List<float> for
// `loadTensor` [BUILTIN-FILE-TENSOR], gpu_host.c into a GpuBuffer<float>.
//
// Every stored float widens EXACTLY to the double either container holds.
// Every failure path records WHY on the io_error channel; codegen turns a NULL
// result into `Error { message }` [BUILTIN-FILE-ERRMSG].

#include "io_error.h"
#include "tensor_file.h"

#include <errno.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Seek to a 64-bit offset on every target. Windows spells it _fseeki64; WASI's
// libc declares only fseek(long), and a wasm32 long is 32 bits, so an offset
// beyond it is refused there rather than truncated into the wrong tensor.
#if defined(_WIN32)
#define osp_fseek _fseeki64
#elif defined(__wasi__)
static int osp_fseek(FILE *f, int64_t offset, int whence) {
  if (offset > (int64_t)LONG_MAX) {
    errno = EOVERFLOW;
    return -1;
  }
  return fseek(f, (long)offset, whence);
}
#else
#define osp_fseek fseeko
#endif

// A safetensors header is JSON text; one this large is not a header.
#define SAFETENSORS_HEADER_MAX ((int64_t)100 * 1024 * 1024)
#define SAFETENSORS_PREFIX 8

// Elements per fread while widening a tensor.
#define LOAD_CHUNK 4096

// One read's stored bytes and widened values. It lives on the heap: on the
// stack it alone fills wasm32's default 64 KiB, and the frame traps.
typedef struct {
  unsigned char bytes[LOAD_CHUNK * 8];
  double values[LOAD_CHUNK];
} ReadBuffers;

// --- stored float formats ------------------------------------------------------

// Byte width of one stored element.
static size_t dtype_width(OspDtype t) {
  return t == OSP_DT_BF16 ? 2 : (size_t)t;
}

static double from_f32_bits(uint32_t bits) {
  float f;
  memcpy(&f, &bits, sizeof f);
  return (double)f;
}

// IEEE-754 binary16 → double, exactly: normals, subnormals, zeros, infinities
// and NaN each widen to the double with the same value.
static double from_f16_bits(uint16_t bits) {
  int sign = (bits >> 15) & 1;
  int exponent = (bits >> 10) & 0x1f;
  uint32_t mantissa = bits & 0x3ff;
  double magnitude;
  if (exponent == 0x1f) {
    magnitude = mantissa ? __builtin_nan("") : __builtin_inf();
  } else if (exponent == 0) {
    magnitude = (double)mantissa * 0x1p-24;
  } else {
    magnitude = (double)(mantissa | 0x400) * __builtin_ldexp(0x1p-10, exponent - 15);
  }
  return sign ? -magnitude : magnitude;
}

// Widen the little-endian element at `at` to a double.
static double widen(OspDtype t, const unsigned char *at) {
  switch (t) {
  case OSP_DT_F64: {
    double d;
    memcpy(&d, at, sizeof d);
    return d;
  }
  case OSP_DT_F32:
    return from_f32_bits((uint32_t)at[0] | ((uint32_t)at[1] << 8) | ((uint32_t)at[2] << 16) |
                         ((uint32_t)at[3] << 24));
  case OSP_DT_F16:
    return from_f16_bits((uint16_t)(at[0] | (at[1] << 8)));
  case OSP_DT_BF16:
    return from_f32_bits(((uint32_t)at[0] << 16) | ((uint32_t)at[1] << 24));
  }
  return 0.0;
}

// --- reading -------------------------------------------------------------------

static FILE *open_at(const char *op, const char *path, int64_t offset) {
  FILE *f = path ? fopen(path, "rb") : NULL;
  if (!f) {
    osp_io_error_set(op, path, path ? errno : EINVAL);
    return NULL;
  }
  if (offset < 0 || osp_fseek(f, offset, SEEK_SET) != 0) {
    osp_io_error_set(op, path, offset < 0 ? EINVAL : errno);
    fclose(f);
    return NULL;
  }
  return f;
}

static int read_span(FILE *f, const char *op, const char *path, const OspTensorSpan *span,
                     OspTensorSink sink, void *ctx, ReadBuffers *buffers) {
  size_t width = dtype_width(span->dtype);
  for (int64_t done = 0; done < span->count;) {
    size_t want = (size_t)(span->count - done < LOAD_CHUNK ? span->count - done : LOAD_CHUNK);
    if (fread(buffers->bytes, width, want, f) != want) {
      osp_io_error_text(op, path, ferror(f) ? "read failed" : "file ends before the tensor does");
      return 0;
    }
    for (size_t i = 0; i < want; i++) {
      buffers->values[i] = widen(span->dtype, buffers->bytes + i * width);
    }
    sink(ctx, buffers->values, want);
    done += (int64_t)want;
  }
  return 1;
}

int osp_tensor_read(const char *op, const char *path, const OspTensorSpan *span, OspTensorSink sink,
                    void *ctx) {
  ReadBuffers *buffers = malloc(sizeof *buffers);
  if (!buffers) {
    osp_io_error_set(op, path, ENOMEM);
    return 0;
  }
  FILE *f = open_at(op, path, span->offset);
  int read = f && read_span(f, op, path, span, sink, ctx, buffers);
  if (f) {
    fclose(f);
  }
  free(buffers);
  return read;
}

// --- safetensors ---------------------------------------------------------------

// The header text, NUL-terminated, or NULL with the reason recorded. The
// 8-byte prefix is the header length in little-endian.
static char *read_header(FILE *f, const char *op, const char *path, int64_t *data_start) {
  unsigned char prefix[SAFETENSORS_PREFIX];
  if (fread(prefix, 1, sizeof prefix, f) != sizeof prefix) {
    osp_io_error_text(op, path, "shorter than a safetensors header prefix");
    return NULL;
  }
  uint64_t n = 0;
  for (int i = SAFETENSORS_PREFIX - 1; i >= 0; i--) {
    n = (n << 8) | prefix[i];
  }
  if (n > (uint64_t)SAFETENSORS_HEADER_MAX) {
    osp_io_error_text(op, path, "header length is not plausible for safetensors");
    return NULL;
  }
  char *header = malloc((size_t)n + 1);
  if (!header) {
    osp_io_error_set(op, path, ENOMEM);
    return NULL;
  }
  if (fread(header, 1, (size_t)n, f) != n) {
    osp_io_error_text(op, path, "file ends inside the header");
    free(header);
    return NULL;
  }
  header[n] = '\0';
  *data_start = SAFETENSORS_PREFIX + (int64_t)n;
  return header;
}

static const char *json_space(const char *at) {
  return at + strspn(at, " \t\r\n");
}

static const char *member_value(const char *entry, const char *quoted_name) {
  size_t length = strlen(quoted_name);
  const char *at = entry;
  while ((at = strstr(at, quoted_name)) != NULL) {
    const char *after = json_space(at + length);
    if (*after == ':') {
      return json_space(after + 1);
    }
    at += length;
  }
  return NULL;
}

// The dtype named in one tensor's header entry, or 0 when it is not a float
// format this reader widens.
static OspDtype dtype_of(const char *entry) {
  const char *at = member_value(entry, "\"dtype\"");
  if (!at || *at != '"') {
    return 0;
  }
  at++;
  if (strncmp(at, "F64\"", 4) == 0) return OSP_DT_F64;
  if (strncmp(at, "F32\"", 4) == 0) return OSP_DT_F32;
  if (strncmp(at, "F16\"", 4) == 0) return OSP_DT_F16;
  if (strncmp(at, "BF16\"", 5) == 0) return OSP_DT_BF16;
  return 0;
}

static int read_offset(const char **cursor, char separator, int64_t *value) {
  const char *number = json_space(*cursor);
  char *after = NULL;
  errno = 0;
  *value = (int64_t)strtoll(number, &after, 10);
  if (after == number || errno == ERANGE) {
    return 0;
  }
  const char *delimiter = json_space(after);
  if (*delimiter != separator) {
    return 0;
  }
  *cursor = delimiter + 1;
  return 1;
}

// `[begin,end]` byte offsets relative to the data section, or 0 when absent.
static int offsets_of(const char *entry, int64_t *begin, int64_t *end) {
  const char *at = member_value(entry, "\"data_offsets\"");
  if (!at || *at != '[') {
    return 0;
  }
  at++;
  return read_offset(&at, ',', begin) && read_offset(&at, ']', end) &&
         *begin >= 0 && *end >= *begin;
}

// The header entry for `name` — the text between its `{` and `}` — isolated
// in place by terminating the header at the closing brace. Names are matched
// quoted and followed by a colon, so `h.1.x` never matches inside `h.11.x`.
static char *entry_of(char *header, const char *name) {
  size_t len = strlen(name) + 4;
  char *key = malloc(len);
  if (!key) {
    return NULL;
  }
  (void)snprintf(key, len, "\"%s\"", name);
  const char *value = member_value(header, key);
  char *at = value && *value == '{' ? header + (value - header) : NULL;
  free(key);
  char *close = at ? strchr(at, '}') : NULL;
  if (!close) {
    return NULL;
  }
  *close = '\0';
  return at;
}

// The header of `path`, NULL with the reason recorded.
static char *header_of(const char *op, const char *path, int64_t *data_start) {
  FILE *f = open_at(op, path, 0);
  if (!f) {
    return NULL;
  }
  char *header = read_header(f, op, path, data_start);
  fclose(f);
  return header;
}

int osp_tensor_locate(const char *op, const char *path, const char *name, OspTensorSpan *span) {
  int64_t data_start = 0;
  char *header = header_of(op, path, &data_start);
  if (!header) {
    return 0;
  }
  const char *entry = entry_of(header, name ? name : "");
  OspDtype t = entry ? dtype_of(entry) : 0;
  int64_t begin = 0;
  int64_t end = 0;
  int located = entry && t && offsets_of(entry, &begin, &end);
  free(header);
  if (!located) {
    osp_io_error_text(op, name, !entry ? "no tensor of that name in the header"
                                : !t   ? "dtype is not F64, F32, F16 or BF16"
                                       : "data_offsets are missing or inverted");
    return 0;
  }
  int64_t width = (int64_t)dtype_width(t);
  if ((end - begin) % width != 0) {
    osp_io_error_text(op, name, "byte span is not a whole number of elements");
    return 0;
  }
  *span = (OspTensorSpan){data_start + begin, t, (end - begin) / width};
  return 1;
}
