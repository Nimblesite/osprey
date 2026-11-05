// Hardware parity gate [GPU-MATMUL] [GPU-BACKEND-DEVICE] [GPU-BACKEND-PORTABILITY].
// Run explicitly on a GPU runner: make _test_gpu_device. The product's host and
// device words must be identical, and a compiled kernel dispatched through the
// launch contract must compute what the same shader source computes on the
// host, which gpu_shader_tests.c holds to the host's IEEE results.
#include "gpu_device.h"
#include "io_error.h"
#include <assert.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "gpu_f64.metal"
#include "gpu_math64.metal"
#include "gpu_math_ext.metal"
#include "gpu_device_lib.metal"

void *osprey_gpu_alloc(int64_t length, int64_t elem_bytes);
void *osprey_gpu_matmul(void *left, void *right, int64_t rows, int64_t inner, int64_t cols, int64_t elem_bytes);
void *osprey_gpu_data(void *buffer);
int64_t osprey_gpu_len(void *buffer);
int64_t osprey_gpu_elem_bytes(void *buffer);
int32_t osprey_gpu_launch(const char *kernel, const char *source, const char *kinds, const int64_t *uniforms,
                          void *in0, void *in1, void *out, int64_t *acc, int64_t count);
static uint64_t state = 0x3f27d489aeb5106c;
static unsigned checks;
static const char *selected;
static char selected_storage[16];

// The shapes the compiler emits [GPU-KERNEL-EXTRACT]: a per-element kernel
// with a scalar uniform and a buffer uniform with its length, a sequential
// reduction seeded from `out`, and a kernel that faults.
static const char KERNELS[] =
    "kernel void osprey_kernel_probe(GPU_GLOBAL const ulong *in0 GPU_BUFFER(0), GPU_GLOBAL const ulong *in1 GPU_BUFFER(1),"
    " GPU_GLOBAL ulong *out GPU_BUFFER(2), GPU_GLOBAL ulong *fault GPU_BUFFER(3), GPU_PARAM count GPU_BUFFER(4),"
    " GPU_PARAM scale GPU_BUFFER(5), GPU_GLOBAL const ulong *table GPU_BUFFER(6), GPU_PARAM table_len GPU_BUFFER(7) GPU_THREAD) {\n"
    "  ulong i = GPU_INDEX;\n  if (i >= count) return;\n"
    "  ulong gathered = i < table_len ? table[i] : 0ul;\n"
    "  out[i] = add64(tanh64(in0[i]), multiply64(scale, sub64(exp64(in1[i]), ln64(gathered))));\n}\n"
    "kernel void osprey_kernel_sum(GPU_GLOBAL const ulong *in0 GPU_BUFFER(0), GPU_GLOBAL const ulong *in1 GPU_BUFFER(1),"
    " GPU_GLOBAL ulong *out GPU_BUFFER(2), GPU_GLOBAL ulong *fault GPU_BUFFER(3), GPU_PARAM count GPU_BUFFER(4) GPU_THREAD) {\n"
    "  if (GPU_INDEX != 0) return;\n  ulong acc = out[0];\n"
    "  for (ulong i = 0; i < count; i++) acc = add64(acc, in0[i]);\n  out[0] = acc;\n}\n"
    "kernel void osprey_kernel_fault(GPU_GLOBAL const ulong *in0 GPU_BUFFER(0), GPU_GLOBAL const ulong *in1 GPU_BUFFER(1),"
    " GPU_GLOBAL ulong *out GPU_BUFFER(2), GPU_GLOBAL ulong *fault GPU_BUFFER(3), GPU_PARAM count GPU_BUFFER(4) GPU_THREAD) {\n"
    "  ulong i = GPU_INDEX;\n  if (i >= count) return;\n"
    "  out[i] = (ulong)osprey_add((long)in0[i], (long)in0[i], fault);\n}\n"
    "kernel void osprey_kernel_convert(GPU_GLOBAL const long *in0 GPU_BUFFER(0), GPU_GLOBAL const long *in1 GPU_BUFFER(1),"
    " GPU_GLOBAL long *out GPU_BUFFER(2), GPU_GLOBAL ulong *fault GPU_BUFFER(3), GPU_PARAM count GPU_BUFFER(4) GPU_THREAD) {\n"
    "  if (GPU_INDEX < count) out[GPU_INDEX] = (long)osprey_convert_i8(in0[GPU_INDEX], fault);\n}\n"
    "kernel void osprey_kernel_float32(GPU_GLOBAL const float *a GPU_BUFFER(0),"
    " GPU_GLOBAL const float *b GPU_BUFFER(1), GPU_GLOBAL float *out GPU_BUFFER(2) GPU_THREAD) {\n"
    "  if (GPU_INDEX != 0) return;\n"
    "  out[0] = a[0] * b[0] + a[1];\n"
    "  out[1] = (a[2] + a[3]) - a[2];\n}\n"
    "kernel void osprey_kernel_bytes(GPU_GLOBAL const uchar *in GPU_BUFFER(0),"
    " GPU_GLOBAL uchar *out GPU_BUFFER(1) GPU_THREAD) {\n"
    "  if (GPU_INDEX < 3) out[GPU_INDEX] = in[GPU_INDEX] + 1;\n}\n";

static uint64_t random_word(void) {
  state ^= state << 13;
  state ^= state >> 7;
  state ^= state << 17;
  return state;
}

static void *random_buffer(int64_t count, int low, int range) {
  void *buffer = osprey_gpu_alloc(count, 8);
  int64_t *words = osprey_gpu_data(buffer);
  assert(words);
  for (int64_t i = 0; i < count; i++) {
    uint64_t r = random_word();
    uint64_t e = (uint64_t)low + ((r >> 52) % (uint64_t)range);
    uint64_t word = (r & UINT64_C(0x800fffffffffffff)) | (e << 52);
    memcpy(words + i, &word, sizeof word);
  }
  return buffer;
}

static int nan_word(int64_t word) {
  double value;
  memcpy(&value, &word, sizeof value);
  return isnan(value);
}

static void same_words(const int64_t *a, const int64_t *b, int64_t count) {
  for (int64_t i = 0; i < count; i++) {
    if (a[i] != b[i] && !(nan_word(a[i]) && nan_word(b[i])))
      fprintf(stderr, "parity at %lld: host %016llx device %016llx\n",
          (long long)i, (unsigned long long)a[i], (unsigned long long)b[i]);
    assert(a[i] == b[i] || (nan_word(a[i]) && nan_word(b[i])));
    checks++;
  }
}

static void compare(void *left, void *right, int64_t rows, int64_t inner, int64_t cols) {
  assert(setenv("OSPREY_GPU", "host", 1) == 0);
  osp_io_error_clear();
  void *host = osprey_gpu_matmul(left, right, rows, inner, cols, 8);
  assert(host && !osp_io_error());
  assert(setenv("OSPREY_GPU", selected, 1) == 0);
  void *device = osprey_gpu_matmul(left, right, rows, inner, cols, 8);
  if (!device) fprintf(stderr, "%s\n", osp_io_error());
  assert(device && !osp_io_error());
  same_words(osprey_gpu_data(host), osprey_gpu_data(device), rows * cols);
}

static void extremes(void) {
  uint64_t words[] = {0, UINT64_C(0x8000000000000000), 1, UINT64_C(0x000fffffffffffff),
      UINT64_C(0x0010000000000000), UINT64_C(0x3ff0000000000000),
      UINT64_C(0x3ff0000000000001), UINT64_C(0x7fefffffffffffff),
      UINT64_C(0x7ff0000000000000), UINT64_C(0xfff0000000000000)};
  int64_t count = (int64_t)(sizeof words / sizeof words[0]);
  void *left = osprey_gpu_alloc(count, 8), *right = osprey_gpu_alloc(count, 8);
  memcpy(osprey_gpu_data(left), words, sizeof words);
  memcpy(osprey_gpu_data(right), words, sizeof words);
  compare(left, right, count, 1, count);
}

static void *float_buffer(const float *values, int64_t count) {
  void *buffer = osprey_gpu_alloc(count, 4);
  assert(buffer);
  memcpy(osprey_gpu_data(buffer), values, (size_t)count * sizeof(float));
  return buffer;
}

// [GPU-MATMUL]: compare raw binary32 output independently on host and device.
static void compare32(const float *a, const float *b, const float *expected, const int64_t shape[3]) {
  void *left = float_buffer(a, shape[0] * shape[1]);
  void *right = float_buffer(b, shape[1] * shape[2]);
  for (int mode = 0; mode < 2; mode++) {
    assert(setenv("OSPREY_GPU", mode ? selected : "host", 1) == 0);
    osp_io_error_clear();
    void *result = osprey_gpu_matmul(left, right, shape[0], shape[1], shape[2], 4);
    if (!result) fprintf(stderr, "%s\n", osp_io_error());
    assert(result && !osp_io_error());
    assert(osprey_gpu_len(result) == shape[0] * shape[2]);
    assert(osprey_gpu_elem_bytes(result) == 4);
    assert(memcmp(osprey_gpu_data(result), expected, (size_t)(shape[0] * shape[2]) * sizeof(float)) == 0);
    checks += 4;
  }
}

static void native_float32_products(void) {
  const int64_t square[] = {2, 2, 2}, ordered[] = {1, 3, 1}, rounding[] = {1, 2, 1};
  const float a[] = {1, 2, 3, 4}, b[] = {5, 6, 7, 8}, expected[] = {19, 22, 43, 50};
  const float cancellation[] = {0x1p24f, 1, -0x1p24f}, ones[] = {1, 1, 1}, zero[] = {0};
  const float factors[] = {0x1.000002p0f, -1}, other[] = {0x1.fffffcp-1f, 1};
  compare32(a, b, expected, square);
  compare32(cancellation, ones, zero, ordered);
  compare32(factors, other, zero, rounding);
}

// [LLM-MATMUL-QUANT]: the Q4_K product on host and device over seeded blocks
// with every byte value reached, binary16 scales at ordinary magnitudes, bit for bit.
static void quantized_products(void) {
  const int64_t rows = 3, inner = 512, cols = 5, blocks = inner / 256;
  void *x = osprey_gpu_alloc(rows * inner, 4), *w = osprey_gpu_alloc(cols * blocks * 144, 1);
  float *xs = osprey_gpu_data(x);
  unsigned char *bytes = osprey_gpu_data(w);
  for (int64_t i = 0; i < rows * inner; i++) xs[i] = (float)(int64_t)(random_word() % 2001) / 1000.0f - 1.0f;
  for (int64_t i = 0; i < cols * blocks * 144; i++) bytes[i] = (unsigned char)(random_word() >> 56);
  for (int64_t b = 0; b < cols * blocks; b++) {
    // d in [2^-14, 2^-6), dmin in [2^-16, 2^-10): ordinary binary16 words.
    uint16_t d = (uint16_t)(0x0400 + (random_word() % 0x2000)), dmin = (uint16_t)(0x0100 + (random_word() % 0x1800));
    memcpy(bytes + b * 144, &d, 2);
    memcpy(bytes + b * 144 + 2, &dmin, 2);
  }
  float *host = NULL;
  for (int mode = 0; mode < 2; mode++) {
    assert(setenv("OSPREY_GPU", mode ? selected : "host", 1) == 0);
    osp_io_error_clear();
    void *result = osprey_gpu_matmul_quant(x, w, "Q4_K", rows, inner, cols);
    if (!result) fprintf(stderr, "%s\n", osp_io_error());
    assert(result && !osp_io_error() && osprey_gpu_len(result) == rows * cols && osprey_gpu_elem_bytes(result) == 4);
    float *values = osprey_gpu_data(result);
    if (!mode) host = values;
    else for (int64_t i = 0; i < rows * cols; i++) {
      if (memcmp(&host[i], &values[i], 4) != 0) fprintf(stderr, "quant parity at %lld: host %a device %a\n", (long long)i, host[i], values[i]);
      assert(memcmp(&host[i], &values[i], 4) == 0);
      checks++;
    }
    assert(isfinite(values[0]));
  }
  assert(setenv("OSPREY_GPU", selected, 1) == 0);
  osp_io_error_clear();
  assert(osprey_gpu_matmul_quant(x, w, "Q8_0", rows, inner, cols) == NULL && strstr(osp_io_error(), "expected Q4_K"));
  assert(osprey_gpu_matmul_quant(x, w, "Q4_K", rows, 500, cols) == NULL && strstr(osp_io_error(), "multiple of 256"));
  assert(osprey_gpu_matmul_quant(x, w, "Q4_K", rows, inner, cols + 1) == NULL && strstr(osp_io_error(), "weights"));
  checks += 3;
}

// The host's fold of `words` in element order, as the sequential kernel computes it.
static int64_t summed(const int64_t *words, int64_t count, int64_t seed) {
  for (int64_t i = 0; i < count; i++) seed = (int64_t)add64((ulong)seed, (ulong)words[i]);
  return seed;
}

// The per-element kernel over 1000 elements against the shader source run as
// C; its result feeds the reduction without leaving the device, and only then
// is it read back [GPU-BUFFER-RESIDENT].
static void compiled_probe(void) {
  const int64_t n = 1000;
  void *a = random_buffer(n, 1010, 20), *b = random_buffer(n, 1000, 30), *table = random_buffer(n - 1, 900, 300);
  int64_t *ta = osprey_gpu_data(table);
  for (int64_t i = 0; i < n - 1; i++) ta[i] &= (int64_t)~SIGN;
  void *out = osprey_gpu_alloc(n, 8);
  double scale = 1.5;
  int64_t uniforms[2] = {0, (int64_t)(intptr_t)table};
  memcpy(uniforms, &scale, sizeof scale);
  assert(setenv("OSPREY_GPU", selected, 1) == 0);
  assert(osprey_gpu_launch("osprey_kernel_probe", KERNELS, "sb", uniforms, a, b, out, NULL, n) == 1);
  int64_t *x = osprey_gpu_data(a), *y = osprey_gpu_data(b), expected[1000], acc = 0;
  for (int64_t i = 0; i < n; i++) {
    ulong gathered = i < n - 1 ? (ulong)ta[i] : 0ul;
    expected[i] = (int64_t)add64(tanh64((ulong)x[i]), multiply64((ulong)uniforms[0], sub64(exp64((ulong)y[i]), ln64(gathered))));
  }
  assert(osprey_gpu_launch("osprey_kernel_sum", KERNELS, "", NULL, out, NULL, NULL, &acc, n) == 1);
  int64_t total = summed(expected, n, 0);
  same_words(&total, &acc, 1);
  same_words(expected, osprey_gpu_data(out), n);
}

static void compiled_reduction(void) {
  const int64_t n = 777;
  void *a = random_buffer(n, 1000, 40);
  double seed = 0.25;
  int64_t acc;
  memcpy(&acc, &seed, sizeof acc);
  int64_t expected = summed(osprey_gpu_data(a), n, acc);
  assert(osprey_gpu_launch("osprey_kernel_sum", KERNELS, "", NULL, a, NULL, NULL, &acc, n) == 1);
  same_words(&expected, &acc, 1);
}

// A faulting kernel records the fault word the runtime aborts on; the abort
// itself is covered by gpu_selection_tests.c, since a Metal device must not be
// used across a fork.
static void fault_case(const char *name, const int64_t input[4], const int64_t expected[4]) {
  void *a = osprey_gpu_alloc(4, 8);
  memcpy(osprey_gpu_data(a), input, 4 * sizeof(int64_t));
  int64_t out[4] = {7, 7, 7, 7}, fault = 0;
  OspreyGpuDispatch d = {name, KERNELS, {{0, NULL, NULL, 0, NULL, 8, NULL, 0, 0}}, 0, 4};
  osprey_gpu_arg(&d, OSPREY_GPU_INPUT, osprey_gpu_data(a), NULL, 4, 8);
  osprey_gpu_arg(&d, OSPREY_GPU_INPUT, osprey_gpu_data(a), NULL, 4, 8);
  osprey_gpu_arg(&d, OSPREY_GPU_OUTPUT, out, out, 4, 8);
  osprey_gpu_arg(&d, OSPREY_GPU_OUTPUT, &fault, &fault, 1, 8);
  osprey_gpu_arg(&d, OSPREY_GPU_SCALAR, NULL, NULL, 4, 8);
  assert(osprey_gpu_device_dispatch(&d) == 1 && fault == OSPREY_GPU_FAULT_OVERFLOW);
  for (unsigned i = 0; i < 4; i++) assert(out[i] == expected[i]);
  checks += 5;
}

static void compiled_fault(void) {
  const int64_t addition[4] = {0, 0, INT64_MAX, 0}, zeros[4] = {0, 0, 0, 0};
  fault_case("osprey_kernel_fault", addition, zeros);
  const int64_t conversion[4] = {127, 128, -128, -129}, narrowed[4] = {127, 0, -128, 0};
  fault_case("osprey_kernel_convert", conversion, narrowed);
}

// [GPU-DEVICE-NATIVE]: FMA and reassociation each change these binary32 results.
// Raw ABI storage here tests the driver independently of typed-buffer lowering.
static void native_float32_rounding(void) {
  float a[4] = {0x1.000002p0f, -1.0f, 0x1p24f, 1.0f};
  float b[4] = {0x1.fffffcp-1f, 0.0f, 0.0f, 0.0f}, actual[2] = {99.0f, 99.0f};
  OspreyGpuDispatch d = {"osprey_kernel_float32", KERNELS, {{0, NULL, NULL, 0, NULL, 8, NULL, 0, 0}}, 0, 1};
  osprey_gpu_arg(&d, OSPREY_GPU_INPUT, a, NULL, 4, 4);
  osprey_gpu_arg(&d, OSPREY_GPU_INPUT, b, NULL, 4, 4);
  osprey_gpu_arg(&d, OSPREY_GPU_OUTPUT, actual, actual, 2, 4);
  assert(osprey_gpu_device_dispatch(&d) == 1);
  assert(actual[0] == 0.0f && !signbit(actual[0]));
  assert(actual[1] == 0.0f && !signbit(actual[1]));
  checks += 2;
}

// [GPU-BUFFER-ELEM]: a three-byte GPU output cannot overwrite either guard.
static void native_byte_transfers(void) {
  uint8_t input[3] = {0, 127, 254}, output[5] = {0xaa, 0, 0, 0, 0xbb};
  OspreyGpuDispatch d = {"osprey_kernel_bytes", KERNELS, {{0, NULL, NULL, 0, NULL, 8, NULL, 0, 0}}, 0, 3};
  osprey_gpu_arg(&d, OSPREY_GPU_INPUT, input, NULL, 3, 1);
  osprey_gpu_arg(&d, OSPREY_GPU_OUTPUT, output + 1, output + 1, 3, 1);
  assert(osprey_gpu_device_dispatch(&d) == 1);
  assert(output[0] == 0xaa && output[4] == 0xbb);
  assert(output[1] == 1 && output[2] == 128 && output[3] == 255);
  checks += 5;
}

int main(void) {
  selected = getenv("OSPREY_GPU");
  (void)snprintf(selected_storage, sizeof selected_storage, "%s", selected ? selected : "auto");
  selected = selected_storage;
  extremes();
  compare(random_buffer(64, 0, 2047), random_buffer(64, 0, 2047), 64, 1, 64);
  compare(random_buffer(32 * 511, 700, 600), random_buffer(511 * 16, 700, 600), 32, 511, 16);
  compare(random_buffer(32 * 511, 0, 100), random_buffer(511 * 16, 0, 2047), 32, 511, 16);
  compare(random_buffer(32 * 511, 1020, 7), random_buffer(511 * 16, 1020, 7), 32, 511, 16);
  compiled_probe();
  compiled_reduction();
  compiled_fault();
  quantized_products();
#if defined(OSPREY_GPU_METAL)
  if (strcmp(selected, "opencl") != 0) assert(osprey_gpu_metal_completed() == 12);
#endif
  native_float32_rounding();
#if defined(OSPREY_GPU_METAL)
  if (strcmp(selected, "opencl") != 0) assert(osprey_gpu_metal_completed() == 13);
#endif
  native_byte_transfers();
#if defined(OSPREY_GPU_METAL)
  if (strcmp(selected, "opencl") != 0) assert(osprey_gpu_metal_completed() == 14);
#endif
  native_float32_products();
#if defined(OSPREY_GPU_METAL)
  if (strcmp(selected, "opencl") != 0) assert(osprey_gpu_metal_completed() == 17);
#endif
  printf("[ok] device parity: %u checks over binary64 and binary32 products, compiled kernels, reductions, faults, byte transfers and the Q4_K product\n", checks);
  return 0;
}
