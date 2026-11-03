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
// Every stored float widens EXACTLY to the double word a GpuBuffer<float>
// holds. Every failure path records WHY on the io_error channel and returns
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
void *osprey_gpu_data(void *buffer);

// Elements are 8-byte words; mirrors gpu_runtime.c's GPU_MAX_LEN.
#define GPU_MAX_LEN (INT64_MAX / 8)

// The NULL of a failed operation, with the buffer it had allocated released.
// The ARC corpus lane found two live objects per failed load before this
// existed: tests/core/gpu/transformer pins it, memory_golden_tests.c counts it.
static void *abandoned(void *buffer) {
  osp_release(buffer);
  return NULL;
}

static const char OP_MATMUL[] = "gpuMatMul";
static const char OP_LOAD[] = "gpuLoad";
static const char OP_TENSOR[] = "gpuLoadTensor";

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

// --- the dense product -------------------------------------------------------

// rows × cols as an element count, or -1 when negative or overflowing.
static int64_t area(int64_t rows, int64_t cols) {
  if (rows < 0 || cols < 0 || (rows != 0 && cols > GPU_MAX_LEN / rows)) {
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

// One rank-1 update, out[r][*] += left[r][k] * right[k][*]: the product and the
// sum are separate statements so no target contracts them into a fused
// multiply-add, and every output element still accumulates in k order.
static void axpy_row(int64_t *out, const int64_t *right, double x, int64_t cols) {
  for (int64_t c = 0; c < cols; c++) {
    double product = x * word_double(right[c]);
    out[c] = double_word(word_double(out[c]) + product);
  }
}

// out[r*cols + c] = sum over k, in order from 0.0, of left[r*inner + k] *
// right[k*cols + c], in double. NULL with the reason recorded when either
// operand's length disagrees with the shape or the shape overflows.
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
  void *out = osprey_gpu_alloc(n, 8);
  if (!out || osprey_gpu_len(out) != n) {
    osp_io_error_set(OP_MATMUL, "result", ENOMEM);
    return abandoned(out);
  }
  return out;
}

static void host_product(int64_t *o, const int64_t *a, const int64_t *b, int64_t rows, int64_t inner, int64_t cols) {
  for (int64_t r = 0; o && a && b && r < rows; r++) {
    for (int64_t k = 0; k < inner; k++)
      axpy_row(o + r * cols, b + k * cols, word_double(a[r * inner + k]), cols);
  }
}

// The product on the selected device, where its operands and result stay
// resident [GPU-BUFFER-RESIDENT], or on the host when the host is selected.
void *osprey_gpu_matmul(void *left, void *right, int64_t rows, int64_t inner, int64_t cols) {
  void *out = matrix_result(left, right, rows, inner, cols);
  if (!out) return NULL;
  int device = osprey_gpu_device_matmul(left, right, out, rows, inner, cols);
  if (device != 0) return device > 0 ? out : abandoned(out);
  host_product(osprey_gpu_data(out), osprey_gpu_data(left), osprey_gpu_data(right), rows, inner, cols);
  return out;
}

// --- loading ---------------------------------------------------------------------

// Copy each widened value into the buffer as the double word it holds.
static void into_words(void *ctx, const double *values, size_t n) {
  int64_t **cursor = ctx;
  memcpy(*cursor, values, n * sizeof **cursor);
  *cursor += n;
}

// A fresh buffer of `span->count` double words read from `path`. A short read
// is an error, not a shorter buffer: a truncated weight file must never load
// as a smaller model.
static void *load(const char *op, const char *path, const OspTensorSpan *span) {
  if (span->count < 0) {
    osp_io_error_text(op, path, "a negative element count");
    return NULL;
  }
  void *out = span->count > GPU_MAX_LEN ? NULL : osprey_gpu_alloc(span->count, 8);
  int64_t *cursor = osprey_gpu_data(out);
  if (!out || osprey_gpu_len(out) != span->count || (span->count > 0 && !cursor)) {
    osp_io_error_set(op, path, ENOMEM);
    return abandoned(out);
  }
  return osp_tensor_read(op, path, span, into_words, &cursor) ? out : abandoned(out);
}

// `count` little-endian binary32 values starting `offset` bytes into `path`.
void *osprey_gpu_load_f32(const char *path, int64_t offset, int64_t count) {
  return load(OP_LOAD, path, &(OspTensorSpan){offset, OSP_DT_F32, count});
}

// One tensor out of a safetensors file, widened to a float buffer.
void *osprey_gpu_load_tensor(const char *path, const char *name) {
  OspTensorSpan span;
  return osp_tensor_locate(OP_TENSOR, path, name, &span) ? load(OP_TENSOR, path, &span) : NULL;
}
