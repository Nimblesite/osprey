// Assertion-driven tests for gpu_runtime.c and gpu_host.c — the
// [GPU-BACKEND-HOST] staging buffer behind toGpu/fromGpu/gpuMap/gpuFold
// ([GPU-BUFFER], [GPU-BUFFER-ELEM], [GPU-BUFFER-LENGTH], [GPU-GET],
// [GPU-FILTER]) and the host operations a trained model needs: the dense
// product [GPU-MATMUL] and the two weight loaders [GPU-BUFFER-LOAD],
// [GPU-BUFFER-TENSOR] (docs/specs/runtime/0034-GPUComputation.md). Linked with gpu_host.c,
// file_runtime.c (the io_error channel) and memory_runtime.c (the default
// backend) by the Makefile's _test_c_runtime.
//
// [FAILURE-EXPLICIT]: negative lengths, overflowing byte sizes and allocation
// failures must return NULL with a recorded reason, never a successful empty
// buffer. Every accessor must be NULL-safe and bounds-checked, because these
// are the no-trap backstops behind codegen's counted loops. The host
// operations extend it: every failure is NULL plus a recorded reason, never a
// partial result.
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#include <assert.h>
#include <errno.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

void *osprey_gpu_alloc(int64_t length, int64_t elem_bytes);
void *osprey_gpu_data(void *buffer);
int64_t osprey_gpu_len(void *buffer);
int64_t osprey_gpu_get(void *buffer, int64_t index);
void osprey_gpu_set(void *buffer, int64_t index, int64_t word);
void osprey_gpu_iota_fill(void *buffer);
int32_t osprey_gpu_in_bounds(void *buffer, int64_t index);
void osprey_gpu_take(void *buffer, int64_t count);
void *osprey_gpu_matmul(void *left, void *right, int64_t rows, int64_t inner, int64_t cols, int64_t elem_bytes);
void *osprey_gpu_load_f32(const char *path, int64_t offset, int64_t count);
void *osprey_gpu_map_file(const char *path, int64_t offset, int64_t count, int64_t elem_bytes);
void *osprey_gpu_load_tensor(const char *path, const char *name, int64_t elem_bytes);
int64_t osprey_gpu_elem_bytes(void *buffer);
const char *osp_io_error(void);
void osp_io_error_clear(void);

static long g_checks = 0;
#define CHECK(c)                                                               \
  do {                                                                         \
    g_checks++;                                                                \
    assert(c);                                                                 \
  } while (0)

// Elements are 8-byte words; this mirrors gpu_runtime.c's GPU_MAX_LEN.
#define GPU_MAX_LEN (INT64_MAX / 8)
#define ROUND_LEN ((int64_t)4096)
#define SMALL_LEN ((int64_t)16)

// A fresh buffer reports its exact length and reads zero at every index.
static void t_alloc_zero_filled(void) {
  void *b = osprey_gpu_alloc(SMALL_LEN, 8);
  CHECK(b != NULL);
  CHECK(osprey_gpu_len(b) == SMALL_LEN);
  for (int64_t i = 0; i < SMALL_LEN; i++) {
    CHECK(osprey_gpu_get(b, i) == 0);
    CHECK(osprey_gpu_in_bounds(b, i) == 1);
  }
  CHECK(osprey_gpu_in_bounds(b, SMALL_LEN) == 0);
  CHECK(osprey_gpu_in_bounds(b, -1) == 0);
}

// Invalid allocation requests fail; every raw accessor remains NULL-safe.
static void t_alloc_refuses_invalid(void) {
  int64_t bad[] = {-1, INT64_MIN, GPU_MAX_LEN + 1, INT64_MAX};
  for (unsigned i = 0; i < sizeof(bad) / sizeof(bad[0]); i++) {
    osp_io_error_clear();
    void *b = osprey_gpu_alloc(bad[i], 8);
    CHECK(b == NULL);
    CHECK(strstr(osp_io_error(), i < 2 ? "negative element count" : "overflows the byte size") != NULL);
    CHECK(osprey_gpu_len(b) == 0);
    CHECK(osprey_gpu_in_bounds(b, 0) == 0);
    CHECK(osprey_gpu_get(b, 0) == 0);
    osprey_gpu_set(b, 0, 99); // ignored, not a wild store
    CHECK(osprey_gpu_get(b, 0) == 0);
  }
  osp_io_error_clear();
  void *zero = osprey_gpu_alloc(0, 8);
  CHECK(zero != NULL && osprey_gpu_len(zero) == 0 && osp_io_error() == NULL);
}

// An IN-range length whose data allocation fails (no machine has 2^63-aligned
// bytes) fails explicitly; the NULL-safe backstops must still hold.
static void t_alloc_failure_is_error(void) {
  osp_io_error_clear();
  void *b = osprey_gpu_alloc(GPU_MAX_LEN, 8);
  CHECK(b == NULL);
  CHECK(strstr(osp_io_error(), "out of memory") != NULL);
  CHECK(osprey_gpu_len(b) == 0);
  CHECK(osprey_gpu_in_bounds(b, 0) == 0);
  osprey_gpu_set(b, 0, 7);
  CHECK(osprey_gpu_get(b, 0) == 0);
}

// Every accessor tolerates NULL — the empty-buffer degenerate case.
static void t_null_safety(void) {
  CHECK(osprey_gpu_len(NULL) == 0);
  CHECK(osprey_gpu_get(NULL, 0) == 0);
  CHECK(osprey_gpu_in_bounds(NULL, 0) == 0);
  osprey_gpu_set(NULL, 0, 1);
  osprey_gpu_take(NULL, 3);
  CHECK(osprey_gpu_len(NULL) == 0);
}

// Full write/read round trip, and out-of-bounds stores land NOWHERE: after
// poking every invalid index the whole payload is still byte-identical.
static void t_set_get_roundtrip(void) {
  void *b = osprey_gpu_alloc(ROUND_LEN, 8);
  CHECK(b != NULL && osprey_gpu_len(b) == ROUND_LEN);
  for (int64_t i = 0; i < ROUND_LEN; i++) {
    osprey_gpu_set(b, i, i * i - 3);
  }
  osprey_gpu_set(b, ROUND_LEN, 0x7A7A);     // one past the end
  osprey_gpu_set(b, -1, 0x7A7A);            // below zero
  osprey_gpu_set(b, INT64_MAX, 0x7A7A);     // absurdly far
  osprey_gpu_set(b, INT64_MIN, 0x7A7A);
  for (int64_t i = 0; i < ROUND_LEN; i++) {
    CHECK(osprey_gpu_get(b, i) == i * i - 3);
  }
}

// [GPU-IOTA] One call fills a buffer with its own indices, whatever its element width,
// and leaves an empty or missing buffer alone.
static void t_iota_fill(void) {
  const int64_t widths[] = {8, 4, 2, 1};
  for (int w = 0; w < 4; w++) {
    void *b = osprey_gpu_alloc(ROUND_LEN, widths[w]);
    osprey_gpu_set(b, 7, 0x7A7A);
    osprey_gpu_iota_fill(b);
    const int64_t mask = widths[w] == 8 ? -1 : ((int64_t)1 << (8 * widths[w])) - 1;
    for (int64_t i = 0; i < ROUND_LEN; i++) {
      CHECK((osprey_gpu_get(b, i) & mask) == (i & mask));
    }
    CHECK(osprey_gpu_len(b) == ROUND_LEN);
  }
  osprey_gpu_iota_fill(NULL);
  void *zero = osprey_gpu_alloc(0, 8);
  osprey_gpu_iota_fill(zero);
  CHECK(osprey_gpu_len(zero) == 0);
}

// Extreme word values survive storage exactly (raw 8-byte words, no coercion).
static void t_boundary_words(void) {
  void *b = osprey_gpu_alloc(2, 8);
  CHECK(b != NULL);
  osprey_gpu_set(b, 0, INT64_MIN);
  osprey_gpu_set(b, 1, INT64_MAX);
  CHECK(osprey_gpu_get(b, 0) == INT64_MIN);
  CHECK(osprey_gpu_get(b, 1) == INT64_MAX);
}

// take publishes a compaction prefix: it only ever SHRINKS, the surviving
// prefix is untouched, and the amputated tail becomes unaddressable.
static void t_take_shrinks_only(void) {
  void *b = osprey_gpu_alloc(10, 8);
  CHECK(b != NULL);
  for (int64_t i = 0; i < 10; i++) {
    osprey_gpu_set(b, i, 100 + i);
  }
  osprey_gpu_take(b, 20); // larger than length: no growth
  CHECK(osprey_gpu_len(b) == 10);
  osprey_gpu_take(b, 7);
  CHECK(osprey_gpu_len(b) == 7);
  for (int64_t i = 0; i < 7; i++) {
    CHECK(osprey_gpu_get(b, i) == 100 + i);
  }
  CHECK(osprey_gpu_in_bounds(b, 7) == 0);
  CHECK(osprey_gpu_get(b, 7) == 0);
  osprey_gpu_set(b, 7, 55); // past the published length: ignored
  osprey_gpu_take(b, 7);    // idempotent at the same count
  CHECK(osprey_gpu_len(b) == 7);
  osprey_gpu_take(b, -3);   // negative clamps to zero: fully compacted away
  CHECK(osprey_gpu_len(b) == 0);
  CHECK(osprey_gpu_in_bounds(b, 0) == 0);
}

// --- float words and the io_error channel --------------------------------------

#include "memory_hooks.h"
#include "tensor_file.h"
#include "tensor_file_tests.h"

// The double a buffer word holds [GPU-BUFFER-ELEM].
static double at(void *b, int64_t i) {
  int64_t word = osprey_gpu_get(b, i);
  int64_t width = osprey_gpu_elem_bytes(b);
  return osp_tensor_widen(width == 8 ? OSP_DT_F64 : width == 4 ? OSP_DT_F32 : OSP_DT_F16,
                          (const unsigned char *)&word);
}

// A float buffer holding `xs` verbatim.
static void *floats(const double *xs, int64_t n) {
  void *b = osprey_gpu_alloc(n, 8);
  CHECK(b != NULL && osprey_gpu_len(b) == n);
  for (int64_t i = 0; i < n; i++) {
    int64_t word;
    memcpy(&word, &xs[i], sizeof word);
    osprey_gpu_set(b, i, word);
  }
  return b;
}

// --- the dense product ---------------------------------------------------------

// A 2×3 · 3×2 product lands exactly, row-major, at rows × cols.
static void t_matmul_exact(void) {
  double l[] = {1, 2, 3, 4, 5, 6};
  double r[] = {7, 8, 9, 10, 11, 12};
  double expected[] = {58, 64, 139, 154};
  osp_io_error_clear();
  void *out = osprey_gpu_matmul(floats(l, 6), floats(r, 6), 2, 3, 2, 8);
  CHECK(out != NULL && osp_io_error() == NULL);
  CHECK(osprey_gpu_len(out) == 4);
  for (int64_t i = 0; i < 4; i++) {
    CHECK(at(out, i) == expected[i]);
  }
}

// The element type a requested width names, for the refusal sentence.
static const char *asked_name(int64_t width) {
  switch (width) {
  case 8: return "float";
  case 4: return "float32";
  case 2: return "float16";
  }
  return "a non-float buffer";
}

// [GPU-BUFFER-TENSOR]: requested storage is never inferred from the file, and
// a mismatch names both the stored dtype and the asked-for type — never a
// silent conversion.
static void t_tensor_requested_widths(void) {
  unsigned char image[2048];
  size_t size = safetensors_image(image, sizeof image);
  const char *path = temp_file(image, size);
  const char *names[] = {"a", "h.1.w", "h.11.w", "d"};
  const char *dtypes[] = {"F32", "F16", "BF16", "F64"};
  const int64_t widths[] = {4, 2, 4, 8};
  for (size_t i = 0; i < 4; i++) {
    for (int64_t width = 1; width <= 8; width *= 2) {
      osp_io_error_clear();
      void *buffer = osprey_gpu_load_tensor(path, names[i], width);
      if (width == widths[i]) {
        CHECK(buffer != NULL && osprey_gpu_elem_bytes(buffer) == width && osp_io_error() == NULL);
      } else {
        CHECK(buffer == NULL);
        char expected[160];
        (void)snprintf(expected, sizeof expected,
                       "gpuLoadTensor: %s: tensor %s is %s, the program asked for %s",
                       names[i], names[i], dtypes[i], asked_name(width));
        expect_text(expected);
      }
    }
  }
  CHECK(unlink(path) == 0);
}

// [GPU-BUFFER-TENSOR]: the refusal sentence the plan pins, word for word — a
// binary16 weight asked as binary32 names both.
static void t_tensor_mismatch_wording(void) {
  const char *header = "{\"wte.weight\":{\"dtype\":\"F16\",\"shape\":[2],\"data_offsets\":[0,4]}}";
  size_t n = strlen(header);
  unsigned char image[512] = {0};
  put_u64(image, (uint64_t)n);
  memcpy(image + 8, header, n);
  put_u16(image + 8 + n, 0x3C00);     // 1.0
  put_u16(image + 8 + n + 2, 0xC100); // -2.5
  const char *path = temp_file(image, 8 + n + 4);
  void *small = osprey_gpu_load_tensor(path, "wte.weight", 2);
  CHECK(small != NULL && osprey_gpu_len(small) == 2 && osprey_gpu_elem_bytes(small) == 2);
  CHECK(at(small, 0) == 1.0 && at(small, 1) == -2.5);
  osp_io_error_clear();
  CHECK(osprey_gpu_load_tensor(path, "wte.weight", 4) == NULL);
  expect_text("gpuLoadTensor: wte.weight: tensor wte.weight is F16, the program asked for float32");
  CHECK(unlink(path) == 0);
}

// The sum runs in k order from 0.0: (2^53 + 1) absorbs the 1 before -2^53
// cancels, so the result is 0.0; the mirrored row gives 1.0. A backend that
// reassociates (pairing ±2^53 first) would answer 1.0 for both.
static void t_matmul_accumulates_in_k_order(void) {
  double big = 9007199254740992.0;
  double forward[] = {big, 1.0, -big};
  double mirrored[] = {-big, 1.0, big};
  double ones[] = {1.0, 1.0, 1.0};
  void *a = osprey_gpu_matmul(floats(forward, 3), floats(ones, 3), 1, 3, 1, 8);
  void *b = osprey_gpu_matmul(floats(mirrored, 3), floats(ones, 3), 1, 3, 1, 8);
  CHECK(a != NULL && osprey_gpu_len(a) == 1 && at(a, 0) == 0.0);
  CHECK(b != NULL && osprey_gpu_len(b) == 1 && at(b, 0) == 1.0);
}

// Any zero dimension with consistent lengths is the zero-filled rows × cols
// result; a zero inner dimension means empty (even NULL) operands.
static void t_matmul_zero_dimensions(void) {
  double two[] = {1.0, 2.0};
  osp_io_error_clear();
  void *inner0 = osprey_gpu_matmul(NULL, NULL, 2, 0, 2, 8);
  CHECK(inner0 != NULL && osprey_gpu_len(inner0) == 4 && osp_io_error() == NULL);
  for (int64_t i = 0; i < 4; i++) {
    CHECK(at(inner0, i) == 0.0);
  }
  void *rows0 = osprey_gpu_matmul(osprey_gpu_alloc(0, 8), floats(two, 2), 0, 2, 1, 8);
  CHECK(rows0 != NULL && osprey_gpu_len(rows0) == 0);
  void *cols0 = osprey_gpu_matmul(floats(two, 2), osprey_gpu_alloc(0, 8), 1, 2, 0, 8);
  CHECK(cols0 != NULL && osprey_gpu_len(cols0) == 0);
}

// A shape the operands do not have is NULL plus the reason naming the operand;
// a negative or overflowing result shape is refused before any operand is read.
static void t_matmul_shape_errors(void) {
  double six[] = {1, 2, 3, 4, 5, 6};
  void *l = floats(six, 6);
  void *r = floats(six, 6);
  osp_io_error_clear();
  CHECK(osprey_gpu_matmul(l, r, -1, 3, 2, 8) == NULL);
  expect_text("gpuMatMul: result: rows * cols is negative or overflows");
  osp_io_error_clear();
  CHECK(osprey_gpu_matmul(l, r, GPU_MAX_LEN, 1, 2, 8) == NULL);
  expect_text("gpuMatMul: result: rows * cols is negative or overflows");
  osp_io_error_clear();
  CHECK(osprey_gpu_matmul(l, r, 2, 4, 2, 8) == NULL);
  expect_text("gpuMatMul: left operand: has 6 elements, the stated shape needs 8");
  osp_io_error_clear();
  CHECK(osprey_gpu_matmul(l, r, 2, 3, 3, 8) == NULL);
  expect_text("gpuMatMul: right operand: has 6 elements, the stated shape needs 9");
  osp_io_error_clear();
  CHECK(osprey_gpu_matmul(NULL, r, 1, 1, 6, 8) == NULL);
  expect_text("gpuMatMul: left operand: has 0 elements, the stated shape needs 1");
  osp_io_error_clear();
  CHECK(osprey_gpu_matmul(l, r, 2, -3, 2, 8) == NULL);
  CHECK(strncmp(osp_io_error(), "gpuMatMul: left operand:", 24) == 0);
  osp_io_error_clear();
  CHECK(osprey_gpu_matmul(NULL, NULL, GPU_MAX_LEN, 0, 1, 8) == NULL);
  expect_errno("gpuMatMul: result: ", ENOMEM);
}

static void *singles(const float *xs, int64_t n) {
  void *buffer = osprey_gpu_alloc(n, 4);
  memcpy(osprey_gpu_data(buffer), xs, (size_t)n * sizeof(float));
  return buffer;
}

// [GPU-MATMUL]: a float32 product rounds at binary32 after every operation, so
// 2^24 + 1 - 2^24 is 0 here and 1 at binary64.
static void t_matmul_float32_rounds_at_its_width(void) {
  float l[] = {1, 2, 3, 4, 5, 6};
  float r[] = {7, 8, 9, 10, 11, 12};
  float expected[] = {58, 64, 139, 154};
  osp_io_error_clear();
  void *out = osprey_gpu_matmul(singles(l, 6), singles(r, 6), 2, 3, 2, 4);
  CHECK(out != NULL && osp_io_error() == NULL);
  CHECK(osprey_gpu_len(out) == 4 && osprey_gpu_elem_bytes(out) == 4);
  CHECK(memcmp(osprey_gpu_data(out), expected, sizeof expected) == 0);
  float big[] = {16777216.0f, 1.0f, -16777216.0f};
  float ones[] = {1, 1, 1};
  void *sum = osprey_gpu_matmul(singles(big, 3), singles(ones, 3), 1, 3, 1, 4);
  float zero = 0;
  CHECK(sum != NULL && memcmp(osprey_gpu_data(sum), &zero, sizeof zero) == 0);
}

// [GPU-MATMUL]: only float and float32 multiply, and both operands share one width.
static void t_matmul_element_type_errors(void) {
  double six[] = {1, 2, 3, 4, 5, 6};
  float singles_six[] = {1, 2, 3, 4, 5, 6};
  osp_io_error_clear();
  CHECK(osprey_gpu_matmul(floats(six, 6), floats(six, 6), 2, 3, 2, 2) == NULL);
  expect_text("gpuMatMul: result: must hold float or float32 elements");
  osp_io_error_clear();
  CHECK(osprey_gpu_matmul(floats(six, 6), singles(singles_six, 6), 2, 3, 2, 8) == NULL);
  expect_text("gpuMatMul: operands: must hold the same float or float32 element type");
  osp_io_error_clear();
  CHECK(osprey_gpu_matmul(singles(singles_six, 6), floats(six, 6), 2, 3, 2, 4) == NULL);
  expect_text("gpuMatMul: operands: must hold the same float or float32 element type");
}

// --- files on disk ---------------------------------------------------------------

// [LLM-WEIGHTS-MAPPED] A mapped buffer reads the file's own bytes at any aligned
// offset across a page boundary, at every width, and refuses what it can't map.
static void t_map_file(void) {
  enum { SIZE = 3 * 16384 + 40 };
  static unsigned char bytes[SIZE];
  for (size_t i = 0; i < SIZE; i++) bytes[i] = (unsigned char)(i * 13 + (i >> 8));
  const char *path = temp_file(bytes, SIZE);
  static const struct { int64_t offset, count, width; } spans[] = {
      {0, SIZE, 1},  {1, 1, 1},     {SIZE - 1, 1, 1}, {16384, 16384, 1}, {16380, 8, 2},
      {16380, 8, 4}, {16376, 8, 8}, {32, 100, 4},     {SIZE - 8, 1, 8},  {0, SIZE / 8, 8},
  };
  for (size_t s = 0; s < sizeof spans / sizeof spans[0]; s++) {
    osp_io_error_clear();
    void *b = osprey_gpu_map_file(path, spans[s].offset, spans[s].count, spans[s].width);
    CHECK(b != NULL && osp_io_error() == NULL);
    CHECK(osprey_gpu_len(b) == spans[s].count && osprey_gpu_elem_bytes(b) == spans[s].width);
    CHECK(memcmp(osprey_gpu_data(b), bytes + spans[s].offset, (size_t)(spans[s].count * spans[s].width)) == 0);
    osp_release(b);
  }
  void *empty = osprey_gpu_map_file(path, 4, 0, 4);
  CHECK(empty != NULL && osprey_gpu_len(empty) == 0);
  osp_release(empty);
  static const struct { int64_t offset, count, width; const char *reason; } refused[] = {
      {-1, 1, 4, "a negative offset"},          {0, -1, 4, "a negative element count"},
      {2, 1, 4, "not a multiple of"},           {0, 1, 3, "element width"},
      {SIZE, 1, 1, "the file ends before"},     {SIZE - 3, 1, 4, "not a multiple of"},
      {0, SIZE + 1, 1, "the file ends before"}, {4, INT64_MAX, 4, "the file ends before"},
      {INT64_MAX - 3, 1, 4, "the file ends before"},
  };
  for (size_t r = 0; r < sizeof refused / sizeof refused[0]; r++) {
    osp_io_error_clear();
    CHECK(osprey_gpu_map_file(path, refused[r].offset, refused[r].count, refused[r].width) == NULL);
    CHECK(strstr(osp_io_error(), refused[r].reason) != NULL);
  }
  osp_io_error_clear();
  CHECK(osprey_gpu_map_file("/nonexistent/osprey/weights.bin", 0, 1, 4) == NULL);
  expect_errno("gpuMapFile: /nonexistent/osprey/weights.bin: ", ENOENT);
  osp_io_error_clear();
  CHECK(osprey_gpu_map_file(NULL, 0, 1, 4) == NULL);
  CHECK(strstr(osp_io_error(), "gpuMapFile") != NULL);
  osp_io_error_clear();
  CHECK(osprey_gpu_map_file("/tmp", 0, 1, 4) == NULL);
  CHECK(strstr(osp_io_error(), "not a regular file") != NULL);
  // A sparse large file selects the shared mapping without allocating its size.
  FILE *large = fopen(path, "r+b");
  CHECK(large != NULL);
  CHECK(ftruncate(fileno(large), (off_t)1 << 30) == 0);
  CHECK(fclose(large) == 0);
  for (int round = 0; round < 2; round++) {
    void *first = osprey_gpu_map_file(path, 0, 8, 1);
    void *second = osprey_gpu_map_file(path, 8, 8, 1);
    CHECK(first != NULL && second != NULL);
    CHECK(memcmp(osprey_gpu_data(first), bytes, 8) == 0);
    osp_release(first);
    CHECK(memcmp(osprey_gpu_data(second), bytes + 8, 8) == 0);
    osp_release(second);
  }
  (void)unlink(path);
}

// Raw binary32 words widen exactly — sign of zero, infinities, NaN and a
// subnormal included — from a byte offset, a short read is an error and count
// zero is the empty buffer.
static void t_load_f32(void) {
  float stored[] = {1.5f, -0.0f, INFINITY, NAN, 1e-45f, -2.5e-3f};
  unsigned char bytes[4 + sizeof stored];
  memset(bytes, 0x5a, 4);
  for (size_t i = 0; i < 6; i++) {
    put_f32(bytes + 4 + i * 4, stored[i]);
  }
  const char *path = temp_file(bytes, sizeof bytes);
  osp_io_error_clear();
  void *b = osprey_gpu_load_f32(path, 4, 6);
  CHECK(b != NULL && osprey_gpu_len(b) == 6 && osp_io_error() == NULL);
  // [GPU-BUFFER-LOAD]: binary32 stays binary32, including signed zero/NaN bytes.
  CHECK(osprey_gpu_get(b, 0) == INT64_C(0x3fc00000));
  CHECK(memcmp(osprey_gpu_data(b), bytes + 4, sizeof stored) == 0);
  CHECK(at(b, 0) == 1.5 && at(b, 1) == 0.0 && signbit(at(b, 1)));
  CHECK(isinf(at(b, 2)) && isnan(at(b, 3)));
  CHECK(at(b, 4) == (double)1e-45f && at(b, 5) == (double)-2.5e-3f);
  void *none = osprey_gpu_load_f32(path, 4, 0);
  CHECK(none != NULL && osprey_gpu_len(none) == 0);
  char prefix[96];
  (void)snprintf(prefix, sizeof prefix, "gpuLoad: %s: ", path);
  osp_io_error_clear();
  CHECK(osprey_gpu_load_f32(path, 4, 7) == NULL);
  CHECK(strncmp(osp_io_error(), prefix, strlen(prefix)) == 0);
  CHECK(strstr(osp_io_error(), "file ends before the tensor does") != NULL);
  osp_io_error_clear();
  CHECK(osprey_gpu_load_f32(path, 0, -1) == NULL);
  CHECK(strstr(osp_io_error(), "a negative element count") != NULL);
  osp_io_error_clear();
  CHECK(osprey_gpu_load_f32(path, -1, 1) == NULL);
  expect_errno(prefix, EINVAL);
  osp_io_error_clear();
  CHECK(osprey_gpu_load_f32("/nonexistent/osprey/weights.bin", 0, 1) == NULL);
  expect_errno("gpuLoad: /nonexistent/osprey/weights.bin: ", ENOENT);
  osp_io_error_clear();
  CHECK(osprey_gpu_load_f32(NULL, 0, 1) == NULL);
  expect_errno("gpuLoad: ", EINVAL);
  osp_io_error_clear();
  CHECK(osprey_gpu_load_f32(path, 0, INT64_MAX) == NULL);
  expect_errno(prefix, ENOMEM);
  (void)unlink(path);
}

#if defined(__APPLE__)
uint64_t osprey_gpu_metal_completed(void);
__attribute__((weak, noinline)) uint64_t osprey_gpu_metal_completed(void) { return 0; }
// A correct numerical answer alone cannot distinguish CPU fallback from GPU
// execution: require a successfully completed Metal command as well.
static void t_matmul_reaches_metal(void) {
  CHECK(setenv("OSPREY_GPU", "metal", 1) == 0);
  t_matmul_exact();
  CHECK(osprey_gpu_metal_completed() > 0);
}
#endif

#if defined(OSPREY_GPU_BUFFER_SUITE)
#define main osprey_gpu_buffer_tests
#endif

// [GPU-BUFFER-ELEM]: narrowing preserves low bits without touching neighbours.
static void t_dense_word_roundtrip(void) {
  int64_t values[] = {0, -1, INT64_MIN, INT64_MAX, 127};
  for (int64_t width = 1; width <= 8; width *= 2) {
    void *buffer = osprey_gpu_alloc(5, width);
    uint64_t mask = width == 8 ? UINT64_MAX : (UINT64_C(1) << (width * 8)) - 1;
    for (int64_t i = 0; i < 5; i++) osprey_gpu_set(buffer, i, values[i]);
    for (int64_t i = 0; i < 5; i++) CHECK((uint64_t)osprey_gpu_get(buffer, i) == ((uint64_t)values[i] & mask));
    CHECK(!osprey_gpu_in_bounds(buffer, 5));
    CHECK(!osprey_gpu_in_bounds(buffer, -1));
  }
}

// [GPU-BUFFER-ELEM]: a million uint8 elements occupy one megabyte of payload —
// one byte each, not eight — pinned by the buffer's own byte accounting and by
// storing through the data pointer at the final byte.
static void t_uint8_million_allocates_one_megabyte(void) {
  void *buffer = osprey_gpu_alloc(1000000, 1);
  CHECK(buffer != NULL && osprey_gpu_len(buffer) == 1000000);
  CHECK(osprey_gpu_elem_bytes(buffer) == 1);
  unsigned char *data = (unsigned char *)osprey_gpu_data(buffer);
  CHECK(data != NULL);
  osprey_gpu_set(buffer, 999999, 255);
  CHECK(data[999999] == 255 && osprey_gpu_get(buffer, 999999) == 255);
  // A store wider than one byte truncates and never touches its neighbour.
  osprey_gpu_set(buffer, 999998, 0x1FF);
  CHECK(data[999998] == 255 && data[999999] == 255 && osprey_gpu_get(buffer, 999998) == 255);
  CHECK(osprey_gpu_in_bounds(buffer, 1000000) == 0);
}

int main(int argc, char **argv) {
#if defined(__APPLE__)
  if (argc == 2 && strcmp(argv[1], "--metal") == 0) {
    t_matmul_reaches_metal();
    t_matmul_accumulates_in_k_order();
    t_matmul_zero_dimensions();
    t_matmul_shape_errors();
    printf("[ok] gpu_runtime: Metal command completed, %ld assertions\n", g_checks);
    return 0;
  }
#else
  (void)argc;
  (void)argv;
#endif
  t_dense_word_roundtrip();
  t_uint8_million_allocates_one_megabyte();
  t_alloc_zero_filled();
  t_alloc_refuses_invalid();
  t_alloc_failure_is_error();
  t_null_safety();
  t_set_get_roundtrip();
  t_iota_fill();
  t_boundary_words();
  t_take_shrinks_only();
  t_matmul_exact();
  t_matmul_float32_rounds_at_its_width();
  t_matmul_element_type_errors();
  t_matmul_accumulates_in_k_order();
  t_matmul_zero_dimensions();
  t_matmul_shape_errors();
  t_load_f32();
  t_map_file();
  t_tensor_loader(&(TensorLoader){"gpuLoadTensor", osprey_gpu_load_tensor, osprey_gpu_len, at});
  t_tensor_requested_widths();
  t_tensor_mismatch_wording();
  printf("[ok] gpu_runtime: %ld assertions\n", g_checks);
  return 0;
}
