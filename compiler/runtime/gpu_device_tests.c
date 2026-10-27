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

void *osprey_gpu_alloc(int64_t length);
void *osprey_gpu_matmul(void *left, void *right, int64_t rows, int64_t inner, int64_t cols);
int64_t *osprey_gpu_data(void *buffer);
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
    "  out[i] = (ulong)osprey_add((long)in0[i], (long)in0[i], fault);\n}\n";

static uint64_t random_word(void) {
  state ^= state << 13;
  state ^= state >> 7;
  state ^= state << 17;
  return state;
}

static void *random_buffer(int64_t count, int low, int range) {
  void *buffer = osprey_gpu_alloc(count);
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
  void *host = osprey_gpu_matmul(left, right, rows, inner, cols);
  assert(host && !osp_io_error());
  assert(setenv("OSPREY_GPU", selected, 1) == 0);
  void *device = osprey_gpu_matmul(left, right, rows, inner, cols);
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
  void *left = osprey_gpu_alloc(count), *right = osprey_gpu_alloc(count);
  memcpy(osprey_gpu_data(left), words, sizeof words);
  memcpy(osprey_gpu_data(right), words, sizeof words);
  compare(left, right, count, 1, count);
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
  void *out = osprey_gpu_alloc(n);
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
static void compiled_fault(void) {
  void *a = osprey_gpu_alloc(4);
  osprey_gpu_data(a)[2] = INT64_MAX;
  int64_t out[4] = {7, 7, 7, 7}, fault = 0;
  OspreyGpuDispatch d = {"osprey_kernel_fault", KERNELS, {{0, NULL, NULL, 0, NULL}}, 0, 4};
  osprey_gpu_arg(&d, OSPREY_GPU_INPUT, osprey_gpu_data(a), NULL, 4);
  osprey_gpu_arg(&d, OSPREY_GPU_INPUT, osprey_gpu_data(a), NULL, 4);
  osprey_gpu_arg(&d, OSPREY_GPU_OUTPUT, out, out, 4);
  osprey_gpu_arg(&d, OSPREY_GPU_OUTPUT, &fault, &fault, 1);
  osprey_gpu_arg(&d, OSPREY_GPU_SCALAR, NULL, NULL, 4);
  assert(osprey_gpu_device_dispatch(&d) == 1 && fault == OSPREY_GPU_FAULT_OVERFLOW);
  assert(out[0] == 0 && out[1] == 0 && out[2] == 0 && out[3] == 0);
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
#if defined(OSPREY_GPU_METAL)
  if (strcmp(selected, "opencl") != 0) assert(osprey_gpu_metal_completed() == 9);
#endif
  printf("[ok] device binary64 parity: %u results over five products, a compiled kernel chained into a reduction on the device, a sequential reduction and a faulting kernel\n", checks);
  return 0;
}
