// Assertion-driven tests for gpu_runtime.c and gpu_host.c — the
// [GPU-BACKEND-HOST] staging buffer behind toGpu/fromGpu/gpuMap/gpuFold
// ([GPU-BUFFER], [GPU-BUFFER-ELEM], [GPU-BUFFER-LENGTH], [GPU-GET],
// [GPU-FILTER]) and the host operations a trained model needs: the dense
// product [GPU-MATMUL] and the two weight loaders [GPU-BUFFER-LOAD],
// [GPU-BUFFER-TENSOR] (docs/specs/0034). Linked with gpu_host.c,
// file_runtime.c (the io_error channel) and memory_runtime.c (the default
// backend) by the Makefile's _test_c_runtime.
//
// The load-bearing contract is TOTALITY: a negative length, an overflowing
// length, and a failed data allocation must all yield an observably EMPTY
// buffer — never a trap, never a partial buffer, never an out-of-bounds
// write. Every accessor must be NULL-safe and bounds-checked, because these
// are the no-trap backstops behind codegen's counted loops. The host
// operations extend it: every failure is NULL plus a recorded reason, never a
// partial result. [GPU-BUFFER-ELEM] adds the width contract: each element is
// stored at its own width — a million uint8 elements cost one megabyte — and
// a loader's dtype must match the element type the program asked for.
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#include "test_alloc.h"
#include "tensor_file.h"
#include <assert.h>
#include <errno.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

void *osprey_gpu_alloc(int64_t length, int64_t elem_bytes);
int64_t osprey_gpu_len(void *buffer);
int64_t osprey_gpu_elem_bytes(void *buffer);
void *osprey_gpu_bytes(void *buffer);
int64_t osprey_gpu_get(void *buffer, int64_t index);
void osprey_gpu_set(void *buffer, int64_t index, int64_t word);
int32_t osprey_gpu_in_bounds(void *buffer, int64_t index);
void osprey_gpu_take(void *buffer, int64_t count);
void *osprey_gpu_matmul(void *left, void *right, int64_t rows, int64_t inner, int64_t cols);
void *osprey_gpu_load_f32(const char *path, int64_t offset, int64_t count);
void *osprey_gpu_load_tensor(const char *path, const char *name, int64_t elem_bytes);
const char *osp_io_error(void);
void osp_io_error_clear(void);

static long g_checks = 0;
#define CHECK(c)                                                               \
  do {                                                                         \
    g_checks++;                                                                \
    assert(c);                                                                 \
  } while (0)

// A length whose byte count overflows int64 at eight bytes per element.
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

// Out-of-range lengths clamp to the EMPTY buffer — allocation is total.
static void t_alloc_clamps_to_empty(void) {
  int64_t bad[] = {-1, INT64_MIN, GPU_MAX_LEN + 1, INT64_MAX};
  for (unsigned i = 0; i < sizeof(bad) / sizeof(bad[0]); i++) {
    void *b = osprey_gpu_alloc(bad[i], 8);
    CHECK(b != NULL);
    CHECK(osprey_gpu_len(b) == 0);
    CHECK(osprey_gpu_in_bounds(b, 0) == 0);
    CHECK(osprey_gpu_get(b, 0) == 0);
    osprey_gpu_set(b, 0, 99); // ignored, not a wild store
    CHECK(osprey_gpu_get(b, 0) == 0);
  }
  void *zero = osprey_gpu_alloc(0, 8);
  CHECK(zero != NULL && osprey_gpu_len(zero) == 0);
}

// An IN-range length whose data allocation fails (no machine has 2^63-aligned
// bytes) still yields the empty buffer, never NULL-deref or a partial length.
static void t_alloc_failure_is_empty(void) {
  void *b = osprey_gpu_alloc(GPU_MAX_LEN, 8);
  CHECK(b != NULL);
  CHECK(osprey_gpu_len(b) == 0);
  CHECK(osprey_gpu_in_bounds(b, 0) == 0);
  osprey_gpu_set(b, 0, 7);
  CHECK(osprey_gpu_get(b, 0) == 0);
}

// Every accessor tolerates NULL — the empty-buffer degenerate case.
static void t_null_safety(void) {
  CHECK(osprey_gpu_len(NULL) == 0);
  CHECK(osprey_gpu_elem_bytes(NULL) == 8);
  CHECK(osprey_gpu_get(NULL, 0) == 0);
  CHECK(osprey_gpu_in_bounds(NULL, 0) == 0);
  CHECK(osprey_gpu_bytes(NULL) == NULL);
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

// Extreme word values survive storage exactly (raw bytes, no coercion).
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

// --- element widths [GPU-BUFFER-ELEM] ------------------------------------------

// The low `elem` bytes of a word, zero-extended — what osprey_gpu_get hands
// back for an element stored as those bytes.
static int64_t low_bytes(int64_t word, int64_t elem) {
  if (elem >= 8) return word;
  return (int64_t)((uint64_t)word & (((uint64_t)1 << (8 * elem)) - 1));
}

// Words chosen so every byte position matters at every width, including the
// sign bits a caller must recover by narrowing (a stored -3 int8 is byte 0xFD).
static const int64_t WIDTH_WORDS[] = {0, -1, -3, 1, 0x7F, -0x80, 0x123456789ABCDEFL,
                                      -0x123456789ABCDEFL, INT64_MIN, INT64_MAX};

// A round trip at one width: every stored element reads back as its low
// bytes, its neighbours never bleed into it, and the payload holds exactly
// length × elem bytes — no more, no less.
static void width_roundtrip(int64_t elem) {
  void *b = osprey_gpu_alloc(ROUND_LEN, elem);
  CHECK(b != NULL && osprey_gpu_len(b) == ROUND_LEN && osprey_gpu_elem_bytes(b) == elem);
  for (int64_t i = 0; i < ROUND_LEN; i++) {
    osprey_gpu_set(b, i, WIDTH_WORDS[i % 10]);
  }
  for (int64_t i = 0; i < ROUND_LEN; i++) {
    int64_t want = low_bytes(WIDTH_WORDS[i % 10], elem);
    CHECK(osprey_gpu_get(b, i) == want);
  }
}

// The empty buffer at every width answers every builtin without elements.
static void width_empty(int64_t elem) {
  void *b = osprey_gpu_alloc(0, elem);
  CHECK(b != NULL && osprey_gpu_len(b) == 0 && osprey_gpu_elem_bytes(b) == elem);
  osprey_gpu_set(b, 0, -1);
  CHECK(osprey_gpu_get(b, 0) == 0 && osprey_gpu_in_bounds(b, 0) == 0);
  CHECK(osprey_gpu_get(b, -1) == 0 && osprey_gpu_get(b, 1 << 20) == 0);
}

// An out-of-range element width is treated as the word, never a trap.
static void t_element_widths(void) {
  for (int64_t elem = 1; elem <= 8; elem <<= 1) {
    width_roundtrip(elem);
    width_empty(elem);
  }
  width_roundtrip(3); // not a power of two: uint32-style width is fine
  width_empty(3);
  void *wide = osprey_gpu_alloc(4, 9);
  CHECK(wide != NULL && osprey_gpu_elem_bytes(wide) == 8);
  osprey_gpu_set(wide, 0, -9);
  CHECK(osprey_gpu_get(wide, 0) == -9);
}

// The Phase 4 gate, measured: a GpuBuffer<uint8> of 1,000,000 elements
// allocates exactly one megabyte [GPU-BUFFER-ELEM]. The injector is armed to
// fail the one-megabyte malloc itself, so the buffer can only come back empty
// if its payload asked for precisely that many bytes — an 8-byte element
// buffer of the same length succeeds where the armed size does not match.
static void t_million_uint8_is_one_megabyte(void) {
  osp_alloc_fail_size(1000000);
  void *b = osprey_gpu_alloc(1000000, 1);
  CHECK(b != NULL && osprey_gpu_len(b) == 0); // the payload allocation failed
  osp_alloc_fail_off();
  void *ok = osprey_gpu_alloc(1000000, 1);
  CHECK(ok != NULL && osprey_gpu_len(ok) == 1000000);
  osprey_gpu_set(ok, 999999, -2);
  CHECK(osprey_gpu_get(ok, 999999) == 0xFE);
  osp_alloc_fail_size(1000000);
  void *words = osprey_gpu_alloc(1000000, 8); // asks for 8 MB: not the armed size
  CHECK(words != NULL && osprey_gpu_len(words) == 1000000);
  osp_alloc_fail_off();
}

// --- float words and the io_error channel --------------------------------------

#include "tensor_file_tests.h"

// The value a buffer element holds, widened to a double at its own width.
static double at(void *b, int64_t i) {
  int64_t word = osprey_gpu_get(b, i);
  switch (osprey_gpu_elem_bytes(b)) {
  case 8: {
    double d;
    memcpy(&d, &word, sizeof d);
    return d;
  }
  case 4: {
    float f;
    int32_t bits = (int32_t)word;
    memcpy(&f, &bits, sizeof f);
    return (double)f;
  }
  default: {
    uint16_t bits = (uint16_t)word;
    unsigned char le[2] = {(unsigned char)(bits & 0xff), (unsigned char)(bits >> 8)};
    return osp_tensor_widen(OSP_DT_F16, le);
  }
  }
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
  void *out = osprey_gpu_matmul(floats(l, 6), floats(r, 6), 2, 3, 2);
  CHECK(out != NULL && osp_io_error() == NULL);
  CHECK(osprey_gpu_len(out) == 4);
  for (int64_t i = 0; i < 4; i++) {
    CHECK(at(out, i) == expected[i]);
  }
}

// The sum runs in k order from 0.0: (2^53 + 1) absorbs the 1 before -2^53
// cancels, so the result is 0.0; the mirrored row gives 1.0. A backend that
// reassociates (pairing ±2^53 first) would answer 1.0 for both.
static void t_matmul_accumulates_in_k_order(void) {
  double big = 9007199254740992.0;
  double forward[] = {big, 1.0, -big};
  double mirrored[] = {-big, 1.0, big};
  double ones[] = {1.0, 1.0, 1.0};
  void *a = osprey_gpu_matmul(floats(forward, 3), floats(ones, 3), 1, 3, 1);
  void *b = osprey_gpu_matmul(floats(mirrored, 3), floats(ones, 3), 1, 3, 1);
  CHECK(a != NULL && osprey_gpu_len(a) == 1 && at(a, 0) == 0.0);
  CHECK(b != NULL && osprey_gpu_len(b) == 1 && at(b, 0) == 1.0);
}

// The float32 product accumulates in binary32 [GPU-MATMUL]: 2^24 + 1 - 2^24
// is 0.0 in k order and 1.0 in any other association.
static void t_matmul32_accumulates_in_k_order(void) {
  unsigned char big[4], one[4], neg[4], ones[4];
  put_f32(big, 16777216.0f);
  put_f32(one, 1.0f);
  put_f32(neg, -16777216.0f);
  put_f32(ones, 1.0f);
  void *l = osprey_gpu_alloc(3, 4), *r = osprey_gpu_alloc(3, 4);
  unsigned char *lb = (unsigned char *)osprey_gpu_bytes(l);
  unsigned char *rb = (unsigned char *)osprey_gpu_bytes(r);
  CHECK(l && r && lb && rb);
  memcpy(lb + 0, big, 4);
  memcpy(lb + 4, one, 4);
  memcpy(lb + 8, neg, 4);
  for (int i = 0; i < 3; i++) memcpy(rb + i * 4, ones, 4);
  void *out = osprey_gpu_matmul(l, r, 1, 3, 1);
  CHECK(out != NULL && osprey_gpu_len(out) == 1 && at(out, 0) == 0.0f);
  memcpy(lb + 0, neg, 4);
  memcpy(lb + 8, big, 4);
  void *mirrored = osprey_gpu_matmul(l, r, 1, 3, 1);
  CHECK(mirrored != NULL && at(mirrored, 0) == 1.0f);
}

// Any zero dimension with consistent lengths is the zero-filled rows × cols
// result; a zero inner dimension means empty (even NULL) operands.
static void t_matmul_zero_dimensions(void) {
  double two[] = {1.0, 2.0};
  osp_io_error_clear();
  void *inner0 = osprey_gpu_matmul(NULL, NULL, 2, 0, 2);
  CHECK(inner0 != NULL && osprey_gpu_len(inner0) == 4 && osp_io_error() == NULL);
  for (int64_t i = 0; i < 4; i++) {
    CHECK(at(inner0, i) == 0.0);
  }
  void *rows0 = osprey_gpu_matmul(osprey_gpu_alloc(0, 8), floats(two, 2), 0, 2, 1);
  CHECK(rows0 != NULL && osprey_gpu_len(rows0) == 0);
  void *cols0 = osprey_gpu_matmul(floats(two, 2), osprey_gpu_alloc(0, 8), 1, 2, 0);
  CHECK(cols0 != NULL && osprey_gpu_len(cols0) == 0);
}

// A shape the operands do not have is NULL plus the reason naming the operand;
// a negative or overflowing result shape is refused before any operand is read.
static void t_matmul_shape_errors(void) {
  double six[] = {1, 2, 3, 4, 5, 6};
  void *l = floats(six, 6);
  void *r = floats(six, 6);
  osp_io_error_clear();
  CHECK(osprey_gpu_matmul(l, r, -1, 3, 2) == NULL);
  expect_text("gpuMatMul: result: rows * cols is negative or overflows");
  osp_io_error_clear();
  CHECK(osprey_gpu_matmul(l, r, GPU_MAX_LEN, 1, 2) == NULL);
  expect_text("gpuMatMul: result: rows * cols is negative or overflows");
  osp_io_error_clear();
  CHECK(osprey_gpu_matmul(l, r, 2, 4, 2) == NULL);
  expect_text("gpuMatMul: left operand: has 6 elements, the stated shape needs 8");
  osp_io_error_clear();
  CHECK(osprey_gpu_matmul(l, r, 2, 3, 3) == NULL);
  expect_text("gpuMatMul: right operand: has 6 elements, the stated shape needs 9");
  osp_io_error_clear();
  CHECK(osprey_gpu_matmul(NULL, r, 1, 1, 6) == NULL);
  expect_text("gpuMatMul: left operand: has 0 elements, the stated shape needs 1");
  osp_io_error_clear();
  CHECK(osprey_gpu_matmul(l, r, 2, -3, 2) == NULL);
  CHECK(strncmp(osp_io_error(), "gpuMatMul: left operand:", 24) == 0);
}

// --- files on disk ---------------------------------------------------------------

// Raw binary32 words load byte for byte with NO widening [GPU-BUFFER-LOAD]:
// the buffer holds the file's own bytes, sign of zero, infinities, NaN and a
// subnormal included, from a byte offset; a short read is an error and count
// zero is the empty buffer.
static void t_load_f32_verbatim(void) {
  float stored[] = {1.5f, -0.0f, INFINITY, NAN, 1e-45f, -2.5e-3f};
  unsigned char bytes[4 + sizeof stored];
  memset(bytes, 0x5a, 4);
  for (size_t i = 0; i < 6; i++) {
    put_f32(bytes + 4 + i * 4, stored[i]);
  }
  const char *path = temp_file(bytes, sizeof bytes);
  osp_io_error_clear();
  void *b = osprey_gpu_load_f32(path, 4, 6);
  CHECK(b != NULL && osprey_gpu_len(b) == 6 && osprey_gpu_elem_bytes(b) == 4);
  CHECK(memcmp(osprey_gpu_bytes(b), bytes + 4, sizeof stored) == 0);
  CHECK(at(b, 0) == 1.5 && at(b, 1) == 0.0 && signbit(at(b, 1)));
  CHECK(isinf(at(b, 2)) && isnan(at(b, 3)));
  CHECK(at(b, 4) == (double)1e-45f && at(b, 5) == (double)-2.5e-3f);
  void *none = osprey_gpu_load_f32(path, 4, 0);
  CHECK(none != NULL && osprey_gpu_len(none) == 0);
  (void)unlink(path);
}

// Every gpuLoad failure names the operation, the path and the reason
// [GPU-BUFFER-LOAD] [BUILTIN-FILE-ERRMSG].
static void t_load_f32_errors(void) {
  float stored[] = {1.5f, -0.0f};
  unsigned char bytes[4 + sizeof stored];
  memset(bytes, 0x5a, 4);
  for (size_t i = 0; i < 2; i++) {
    put_f32(bytes + 4 + i * 4, stored[i]);
  }
  const char *path = temp_file(bytes, sizeof bytes);
  char prefix[96];
  (void)snprintf(prefix, sizeof prefix, "gpuLoad: %s: ", path);
  osp_io_error_clear();
  CHECK(osprey_gpu_load_f32(path, 4, 3) == NULL);
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
  (void)unlink(path);
}

// The dtype-mismatch sentences [GPU-BUFFER-TENSOR]: F64 into `float`, F32 and
// BF16 into `float32`, F16 into `float16`, and nothing else — a mismatch
// names the tensor's dtype and the type the program asked for, never a silent
// conversion.
static void t_tensor_dtype_must_match(const char *path) {
  osp_io_error_clear();
  CHECK(osprey_gpu_load_tensor(path, "h.1.w", 4) == NULL);
  expect_text("gpuLoadTensor: h.1.w: tensor h.1.w is F16, the program asked for float32");
  osp_io_error_clear();
  CHECK(osprey_gpu_load_tensor(path, "a", 8) == NULL);
  expect_text("gpuLoadTensor: a: tensor a is F32, the program asked for float");
  osp_io_error_clear();
  CHECK(osprey_gpu_load_tensor(path, "a", 2) == NULL);
  expect_text("gpuLoadTensor: a: tensor a is F32, the program asked for float16");
  osp_io_error_clear();
  CHECK(osprey_gpu_load_tensor(path, "h.11.w", 2) == NULL);
  expect_text("gpuLoadTensor: h.11.w: tensor h.11.w is BF16, the program asked for float16");
  osp_io_error_clear();
  CHECK(osprey_gpu_load_tensor(path, "d", 4) == NULL);
  expect_text("gpuLoadTensor: d: tensor d is F64, the program asked for float32");
}

// An F16 tensor's bytes reach a float16 buffer unchanged and an F32 tensor's
// a float32 buffer unchanged — the buffer is the file's own bytes at the
// asked width [GPU-BUFFER-TENSOR].
static void t_tensor_bytes_are_verbatim(const char *path, const unsigned char *data) {
  osp_io_error_clear();
  void *f16 = osprey_gpu_load_tensor(path, "h.1.w", 2);
  CHECK(f16 != NULL && osprey_gpu_len(f16) == 5 && osprey_gpu_elem_bytes(f16) == 2);
  for (int64_t i = 0; i < 5; i++) {
    CHECK(osprey_gpu_get(f16, i) == (int64_t)(data[24 + i * 2] | (data[25 + i * 2] << 8)));
  }
  void *f32 = osprey_gpu_load_tensor(path, "a", 4);
  CHECK(f32 != NULL && osprey_gpu_elem_bytes(f32) == 4);
  CHECK(memcmp(osprey_gpu_bytes(f32), data, 24) == 0);
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
  t_alloc_zero_filled();
  t_alloc_clamps_to_empty();
  t_alloc_failure_is_empty();
  t_null_safety();
  t_set_get_roundtrip();
  t_boundary_words();
  t_take_shrinks_only();
  t_element_widths();
  t_million_uint8_is_one_megabyte();
  t_matmul_exact();
  t_matmul_accumulates_in_k_order();
  t_matmul32_accumulates_in_k_order();
  t_matmul_zero_dimensions();
  t_matmul_shape_errors();
  t_load_f32_verbatim();
  t_load_f32_errors();
  unsigned char image[1024];
  const char *path = temp_file(image, safetensors_image(image, sizeof image));
  t_tensor_dtype_must_match(path);
  t_tensor_bytes_are_verbatim(path, safetensors_data(image));
  (void)unlink(path);
  t_tensor_loader(&(TensorLoader){"gpuLoadTensor", osprey_gpu_load_tensor, osprey_gpu_len, at});
  printf("[ok] gpu_runtime: %ld assertions\n", g_checks);
  return 0;
}
