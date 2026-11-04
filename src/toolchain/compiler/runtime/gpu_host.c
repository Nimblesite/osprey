// Host-backend operations on dense buffers beyond the buffer primitive itself
// (docs/specs/runtime/0034-GPUComputation.md): the row-major dense product behind
// `gpuMatMul` [GPU-MATMUL], and the two ways a trained model's weights enter a
// program without passing through a List — raw little-endian binary32 words
// at a byte offset (`gpuLoad`, [GPU-BUFFER-LOAD]: the layout of llm.c's
// checkpoints and of any raw dump) and one named tensor out of a safetensors
// file (`gpuLoadTensor`, [GPU-BUFFER-TENSOR]: the Hugging Face interchange
// format — an 8-byte little-endian header length, a JSON header mapping each
// name to its dtype, shape and data_offsets, then the raw tensor bytes). Both
// read through tensor_file.c, the reader the host `loadTensor` shares.
//
// Each stored dtype loads at the requested width; only BF16 widens to float32.
// Every failure path records WHY on the io_error channel and returns
// NULL; codegen turns that pair into `Error { message }` [BUILTIN-FILE-ERRMSG].
// Like gpu_runtime.c this unit is backend-neutral: buffers come from
// osprey_gpu_alloc, scratch memory never outlives the call, and a buffer
// allocated for a result that then fails is released before the failure is
// reported, so a NULL result owes the memory backend nothing.

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
void *osprey_gpu_data(void *buffer);

// The NULL of a failed operation, with the buffer it had allocated released.
// The ARC corpus lane found two live objects per failed load before this
// existed: src/osprey/tests/core/gpu/transformer pins it, memory_golden_tests.c counts it.
static void *abandoned(void *buffer) {
  osp_release(buffer);
  return NULL;
}

static const char OP_MATMUL[] = "gpuMatMul";
static const char OP_LOAD[] = "gpuLoad";
static const char OP_TENSOR[] = "gpuLoadTensor";

// --- the dense product -------------------------------------------------------

// rows × cols as an element count, or -1 when negative or overflowing.
static int64_t area(int64_t rows, int64_t cols, int64_t elem) {
  if (rows < 0 || cols < 0 || (rows != 0 && cols > INT64_MAX / elem / rows)) {
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

// out[r*cols + c] = sum over k, in order from 0.0, of left[r*inner + k] *
// right[k*cols + c], at the operand width. NULL with the reason recorded when either
// operand's length disagrees with the shape or the shape overflows.
static void *matrix_result(void *left, void *right, int64_t rows, int64_t inner, int64_t cols, int64_t elem) {
  if (elem != 4 && elem != 8) {
    osp_io_error_text(OP_MATMUL, "result", "must hold float or float32 elements");
    return NULL;
  }
  int64_t n = area(rows, cols, elem);
  if (n < 0) {
    osp_io_error_text(OP_MATMUL, "result", "rows * cols is negative or overflows");
    return NULL;
  }
  if (!shaped(left, area(rows, inner, elem), "left operand") ||
      !shaped(right, area(inner, cols, elem), "right operand")) {
    return NULL;
  }
  if ((left && osprey_gpu_elem_bytes(left) != elem) || (right && osprey_gpu_elem_bytes(right) != elem)) {
    osp_io_error_text(OP_MATMUL, "operands", "must hold the same float or float32 element type");
    return NULL;
  }
  void *out = osprey_gpu_alloc(n, elem);
  if (!out || osprey_gpu_len(out) != n) {
    osp_io_error_set(OP_MATMUL, "result", ENOMEM);
    return abandoned(out);
  }
  return out;
}

// [GPU-MATMUL]: one source template, instantiated at each arithmetic width.
// Multiplication is a separate full expression: both products and sums round.
#define PRODUCT(SUFFIX, SCALAR) \
static SCALAR read##SUFFIX(const unsigned char *at) { SCALAR value; memcpy(&value, at, sizeof value); return value; } \
static void write##SUFFIX(unsigned char *at, SCALAR value) { memcpy(at, &value, sizeof value); } \
static void product##SUFFIX(unsigned char *out, const unsigned char *a, const unsigned char *b, int64_t rows, int64_t inner, int64_t cols) { \
  for (int64_t r = 0; r < rows; r++) for (int64_t k = 0; k < inner; k++) { \
    SCALAR x = read##SUFFIX(a + (r * inner + k) * sizeof(SCALAR)); \
    for (int64_t c = 0; c < cols; c++) { \
      SCALAR product = x * read##SUFFIX(b + (k * cols + c) * sizeof(SCALAR)); \
      unsigned char *at = out + (r * cols + c) * sizeof(SCALAR); \
      write##SUFFIX(at, read##SUFFIX(at) + product); \
    } \
  } \
}
PRODUCT(32, float)
PRODUCT(64, double)
#undef PRODUCT

// The product on the selected device, where its operands and result stay
// resident [GPU-BUFFER-RESIDENT], or on the host when the host is selected.
void *osprey_gpu_matmul(void *left, void *right, int64_t rows, int64_t inner, int64_t cols, int64_t elem_bytes) {
  void *out = matrix_result(left, right, rows, inner, cols, elem_bytes);
  if (!out) return NULL;
  int device = osprey_gpu_device_matmul(left, right, out, rows, inner, cols);
  if (device != 0) return device > 0 ? out : abandoned(out);
  // [FAILURE-EXPLICIT] Synchronize before either raw accessor can quarantine
  // a failed download; the Result channel retains the reason and owns no out.
  if (rows && inner && cols &&
      (osprey_gpu_readable(left, 0) != 1 || osprey_gpu_readable(right, 0) != 1)) return abandoned(out);
  if (elem_bytes == 4)
    product32(osprey_gpu_data(out), osprey_gpu_data(left), osprey_gpu_data(right), rows, inner, cols);
  else
    product64(osprey_gpu_data(out), osprey_gpu_data(left), osprey_gpu_data(right), rows, inner, cols);
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
// float32 buffer exactly: place the stored bits in the upper half of the
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
  unsigned char *cursor = (unsigned char *)osprey_gpu_data(out);
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
