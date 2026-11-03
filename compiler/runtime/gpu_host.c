// Host-backend operations on dense buffers beyond the buffer primitive itself
// (docs/specs/runtime/0034-GPUComputation.md): the row-major dense product
// behind `gpuMatMul` [GPU-MATMUL], and the two ways a trained model's weights
// enter a program without passing through a List — raw little-endian binary32
// words at a byte offset (`gpuLoad`, [GPU-BUFFER-LOAD]: the layout of llm.c's
// checkpoints and of any raw dump) and one named tensor out of a safetensors
// file (`gpuLoadTensor`, [GPU-BUFFER-TENSOR]: the Hugging Face interchange
// format — an 8-byte little-endian header length, a JSON header mapping each
// name to its dtype, shape and data_offsets, then the raw tensor bytes). Both
// read through tensor_file.c, the reader the host `loadTensor` shares.
//
// A loader stores each element at the width the program asked for — no
// widening anywhere [GPU-BUFFER-ELEM]: a binary32 word loads byte for byte into
// a float32 buffer, an F16 tensor byte for byte into a float16 one, and a BF16
// tensor widens exactly into float32. Every failure path records WHY on the
// io_error channel and returns NULL; codegen turns that pair into
// `Error { message }` [BUILTIN-FILE-ERRMSG]. Like gpu_runtime.c this unit is
// backend-neutral: buffers come from osprey_gpu_alloc, scratch memory never
// outlives the call, and a buffer allocated for a result that then fails is
// released before the failure is reported, so a NULL result owes the memory
// backend nothing.

#include "io_error.h"
#include "memory_hooks.h"
#include "gpu_device.h"
#include "tensor_file.h"

#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

void *osprey_gpu_alloc(int64_t length, int64_t elem_bytes);
int64_t osprey_gpu_len(void *buffer);
int64_t osprey_gpu_elem_bytes(void *buffer);
void *osprey_gpu_bytes(void *buffer);
int32_t osprey_gpu_declined(const char *what);

// A length whose element count overflows int64 as bytes is not allocatable.
#define GPU_MAX_COUNT (INT64_MAX / 8)

static const char OP_MATMUL[] = "gpuMatMul";
static const char OP_LOAD[] = "gpuLoad";
static const char OP_TENSOR[] = "gpuLoadTensor";

// The NULL of a failed operation, with the buffer it had allocated released.
// The ARC corpus lane found two live objects per failed load before this
// existed: tests/core/gpu/transformer pins it, memory_golden_tests.c counts it.
static void *abandoned(void *buffer) {
  osp_release(buffer);
  return NULL;
}

// --- the dense product -------------------------------------------------------

static double word_double(int64_t word) {
  double d;
  memcpy(&d, &word, sizeof d);
  return d;
}

static int64_t double_word(double d) {
  int64_t word;
  memcpy(&word, &d, sizeof word);
  return word;
}

// The little-endian float32 at `at`.
static float at_f32(const unsigned char *at) {
  uint32_t bits = (uint32_t)at[0] | ((uint32_t)at[1] << 8) | ((uint32_t)at[2] << 16) |
                  ((uint32_t)at[3] << 24);
  float f;
  memcpy(&f, &bits, sizeof f);
  return f;
}

static void put_f32(unsigned char *at, float f) {
  uint32_t bits;
  memcpy(&bits, &f, sizeof bits);
  for (int i = 0; i < 4; i++) at[i] = (unsigned char)((bits >> (8 * i)) & 0xff);
}

// rows × cols as an element count, or -1 when negative or overflowing.
static int64_t area(int64_t rows, int64_t cols) {
  if (rows < 0 || cols < 0 || (rows != 0 && cols > GPU_MAX_COUNT / rows)) {
    return -1;
  }
  return rows * cols;
}

// Reject an operand whose length is not the stated shape, naming which one.
static int shaped(void *buffer, int64_t count, const char *operand) {
  if (count >= 0 && osprey_gpu_len(buffer) == count) {
    return 1;
  }
  char reason[96];
  (void)snprintf(reason, sizeof reason, "has %lld elements, the stated shape needs %lld",
                 (long long)osprey_gpu_len(buffer), (long long)count);
  osp_io_error_text(OP_MATMUL, operand, reason);
  return 0;
}

// The zero-filled rows × cols result, or NULL with the reason recorded when
// either operand's length disagrees with the shape or the shape overflows.
static void *matrix_result(void *left, void *right, int64_t rows, int64_t inner, int64_t cols) {
  int64_t n = area(rows, cols);
  if (n < 0) {
    osp_io_error_text(OP_MATMUL, "result", "rows * cols is negative or overflows");
    return NULL;
  }
  if (!shaped(left, area(rows, inner), "left operand") ||
      !shaped(right, area(inner, cols), "right operand")) {
    return NULL;
  }
  void *out = osprey_gpu_alloc(n, osprey_gpu_elem_bytes(left));
  if (!out || osprey_gpu_len(out) != n) {
    osp_io_error_set(OP_MATMUL, "result", ENOMEM);
    return abandoned(out);
  }
  return out;
}

// One rank-1 update of double elements, out[r][*] += left[r][k] * right[k][*]:
// the product and the sum are separate statements so no target contracts them
// into a fused multiply-add, and every output element still accumulates in k
// order from 0.0 [GPU-MATMUL].
static void axpy64(int64_t *out, const int64_t *right, double x, int64_t cols) {
  for (int64_t c = 0; c < cols; c++) {
    double product = x * word_double(right[c]);
    out[c] = double_word(word_double(out[c]) + product);
  }
}

static void host_product64(int64_t *o, const int64_t *a, const int64_t *b, int64_t rows,
                           int64_t inner, int64_t cols) {
  for (int64_t r = 0; r < rows; r++) {
    for (int64_t k = 0; k < inner; k++)
      axpy64(o + r * cols, b + k * cols, word_double(a[r * inner + k]), cols);
  }
}

// One rank-1 update of float32 elements, in binary32 throughout: each product
// is formed and rounded at binary32 before it is added [GPU-MATMUL], which the
// separate statements guarantee where a fused multiply-add would keep one more
// bit.
static void axpy32(unsigned char *out, const unsigned char *right, float x, int64_t cols) {
  for (int64_t c = 0; c < cols; c++) {
    float product = x * at_f32(right + c * 4);
    put_f32(out + c * 4, at_f32(out + c * 4) + product);
  }
}

static void host_product32(unsigned char *o, const unsigned char *a, const unsigned char *b,
                           int64_t rows, int64_t inner, int64_t cols) {
  for (int64_t r = 0; r < rows; r++) {
    for (int64_t k = 0; k < inner; k++)
      axpy32(o + r * cols * 4, b + k * cols * 4, at_f32(a + (r * inner + k) * 4), cols);
  }
}

// The product on the selected device, where its operands and result stay
// resident [GPU-BUFFER-RESIDENT], or on the host when the host is selected.
// The float32 product has no device kernel until plan 0050 phase 5, so a
// selected device is asked the declined question rather than silently running
// the host loop [GPU-BACKEND-DEVICE].
void *osprey_gpu_matmul(void *left, void *right, int64_t rows, int64_t inner, int64_t cols) {
  void *out = matrix_result(left, right, rows, inner, cols);
  if (!out) return NULL;
  int64_t elem = osprey_gpu_elem_bytes(left);
  if (elem != 8 && osprey_gpu_declined("gpuMatMul over float32, whose device kernel is plan 0050 phase 5's") != 0) {
    return abandoned(out);
  }
  int device = osprey_gpu_device_matmul(left, right, out, rows, inner, cols);
  if (device != 0) return device > 0 ? out : abandoned(out);
  if (elem == 8)
    host_product64((int64_t *)osprey_gpu_bytes(out), (int64_t *)osprey_gpu_bytes(left),
                   (int64_t *)osprey_gpu_bytes(right), rows, inner, cols);
  else
    host_product32(osprey_gpu_bytes(out), osprey_gpu_bytes(left), osprey_gpu_bytes(right), rows,
                   inner, cols);
  return out;
}

// --- loading ---------------------------------------------------------------------

// The reader's sink context: where this chunk lands and how wide a stored
// element is.
typedef struct {
  unsigned char *cursor;
  size_t stored;
} BufferLoad;

// Copy each chunk's stored bytes straight into the buffer's payload, byte for
// byte [GPU-BUFFER-ELEM]: an F64 into `float`, an F32 into `float32`, an F16
// into `float16`.
static void into_bytes(void *ctx, const unsigned char *bytes, size_t n) {
  BufferLoad *load = ctx;
  memcpy(load->cursor, bytes, n * load->stored);
  load->cursor += n * load->stored;
}

// Widen each stored BF16 (the upper sixteen bits of a binary32) into the
// float32 buffer exactly: zero-extend the sixteen bits to the low half of the
// binary32 word [GPU-BUFFER-TENSOR].
static void into_f32_from_bf16(void *ctx, const unsigned char *bytes, size_t n) {
  BufferLoad *load = ctx;
  for (size_t i = 0; i < n; i++) {
    load->cursor[0] = 0;
    load->cursor[1] = 0;
    load->cursor[2] = bytes[2 * i];
    load->cursor[3] = bytes[2 * i + 1];
    load->cursor += 4;
  }
}

// The sink that fills a buffer of `elem`-byte elements from storage of
// `span->dtype`.
static OspTensorSink sink_of(const OspTensorSpan *span, int64_t elem) {
  if (span->dtype == OSP_DT_BF16 && elem == 4) return into_f32_from_bf16;
  return into_bytes;
}

// A fresh buffer of `span->count` elements of `elem` bytes read from `path`. A
// short read is an error, not a shorter buffer: a truncated weight file must
// never load as a smaller model [GPU-BUFFER-LOAD].
static void *load(const char *op, const char *path, const OspTensorSpan *span, int64_t elem) {
  if (span->count < 0) {
    osp_io_error_text(op, path, "a negative element count");
    return NULL;
  }
  void *out = span->count > INT64_MAX / elem ? NULL : osprey_gpu_alloc(span->count, elem);
  unsigned char *cursor = (unsigned char *)osprey_gpu_bytes(out);
  if (!out || osprey_gpu_len(out) != span->count || (span->count > 0 && !cursor)) {
    osp_io_error_set(op, path, ENOMEM);
    return abandoned(out);
  }
  BufferLoad sink = {cursor, osp_tensor_width(span->dtype)};
  return osp_tensor_read(op, path, span, sink_of(span, elem), &sink) ? out : abandoned(out);
}

// `count` little-endian binary32 values starting `offset` bytes into `path`,
// byte for byte with no widening [GPU-BUFFER-LOAD].
void *osprey_gpu_load_f32(const char *path, int64_t offset, int64_t count) {
  return load(OP_LOAD, path, &(OspTensorSpan){offset, OSP_DT_F32, count}, 4);
}

// The element type an element width names [GPU-BUFFER-TENSOR], or NULL when
// the width is not a float's.
static const char *asked_type(int64_t elem) {
  switch (elem) {
  case 8: return "float";
  case 4: return "float32";
  case 2: return "float16";
  }
  return NULL;
}

// The storage dtype `elem` admits [GPU-BUFFER-TENSOR]: F64 into `float`, F32
// and BF16 into `float32`, F16 into `float16`.
static int admits(OspDtype dtype, int64_t elem) {
  return (dtype == OSP_DT_F64 && elem == 8) || (dtype == OSP_DT_F32 && elem == 4) ||
         (dtype == OSP_DT_BF16 && elem == 4) || (dtype == OSP_DT_F16 && elem == 2);
}

// One tensor out of a safetensors file, at the element width `elem` names. A
// dtype the program did not ask for is an error naming both, never a silent
// conversion.
void *osprey_gpu_load_tensor(const char *path, const char *name, int64_t elem) {
  OspTensorSpan span;
  if (!osp_tensor_locate(OP_TENSOR, path, name, &span)) return NULL;
  if (!admits(span.dtype, elem)) {
    char reason[128];
    const char *asked = asked_type(elem);
    (void)snprintf(reason, sizeof reason, "tensor %s is %s, the program asked for %s", name,
                   osp_tensor_dtype_name(span.dtype), asked ? asked : "a non-float buffer");
    osp_io_error_text(OP_TENSOR, name, reason);
    return NULL;
  }
  return load(OP_TENSOR, path, &span, elem);
}
