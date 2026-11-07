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
#include <unistd.h>

// The shared shader source's binary64 oracle, compiled as C inside gpu_host.c.
typedef uint64_t ulong;
static const ulong SIGN = 0x8000000000000000ul;
ulong add64(ulong a, ulong b);
ulong sub64(ulong a, ulong b);
ulong multiply64(ulong a, ulong b);
ulong exp64(ulong x);
ulong ln64(ulong x);
ulong tanh64(ulong x);

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

// Whether every one of the 256 byte values occurs in `bytes`, which the quantized
// parity cases assert of their seeded blocks.
static int all_byte_values(const unsigned char *bytes, int64_t count) {
  unsigned seen[256] = {0}, distinct = 0;
  for (int64_t i = 0; i < count; i++) distinct += !seen[bytes[i]]++;
  return distinct == 256;
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

// `cols` weight rows of `block_bytes` blocks each: a byte sweep whose period is 256
// bytes, so every nibble, 2-bit field, high-bit mask and sign reaches every value and
// consecutive blocks start at different points of it, with ordinary binary16 scales
// written where the format stores them.
static void *quant_weights(int64_t cols, int64_t blocks, int64_t block_bytes, const int64_t *scale_at, unsigned scales) {
  void *w = osprey_gpu_alloc(cols * blocks * block_bytes, 1);
  unsigned char *bytes = osprey_gpu_data(w);
  for (int64_t i = 0; i < cols * blocks * block_bytes; i++) bytes[i] = (unsigned char)((i * 167 + 13) & 255);
  for (int64_t b = 0; b < cols * blocks; b++)
    for (unsigned f = 0; f < scales; f++) {
      // d in [2^-14, 2^-6), then dmin in [2^-16, 2^-10): ordinary binary16 words.
      uint16_t word = (uint16_t)((f ? 0x0100 : 0x0400) + (random_word() % (f ? 0x1800 : 0x2000)));
      memcpy(bytes + b * block_bytes + scale_at[f], &word, 2);
    }
  assert(all_byte_values(bytes, cols * blocks * block_bytes));
  checks++;
  return w;
}

// [LLM-MATMUL-QUANT]: the host oracle's words and the device's, bit for bit.
static void quant_words(const char *type, void *x, void *w, const int64_t shape[3]) {
  float *host = NULL;
  for (int mode = 0; mode < 2; mode++) {
    assert(setenv("OSPREY_GPU", mode ? selected : "host", 1) == 0);
    osp_io_error_clear();
    void *result = osprey_gpu_matmul_quant(x, w, type, shape[0], shape[1], shape[2]);
    if (!result) fprintf(stderr, "%s: %s\n", type, osp_io_error());
    assert(result && !osp_io_error() && osprey_gpu_len(result) == shape[0] * shape[2]);
    assert(osprey_gpu_elem_bytes(result) == 4 && isfinite(*(float *)osprey_gpu_data(result)));
    float *values = osprey_gpu_data(result);
    if (!mode) host = values;
    else for (int64_t i = 0; i < shape[0] * shape[2]; i++) {
      if (memcmp(&host[i], &values[i], 4) != 0)
        fprintf(stderr, "%s parity at %lld: host %a device %a\n", type, (long long)i, host[i], values[i]);
      assert(memcmp(&host[i], &values[i], 4) == 0);
      checks++;
    }
  }
}

// One block format's parity: seeded operands over one full row tile and one
// partial tile (QUANT_ROWS is 8), the words compared, and the three errors a
// wrong format, a short row and a short weight buffer are.
static void quant_parity(const char *type, int64_t block_bytes, int64_t block_elements, const int64_t *scale_at,
                         unsigned scales) {
  const int64_t shape[3] = {11, 512, 5}, blocks = shape[1] / block_elements;
  void *x = osprey_gpu_alloc(shape[0] * shape[1], 4);
  float *xs = osprey_gpu_data(x);
  for (int64_t i = 0; i < shape[0] * shape[1]; i++) xs[i] = (float)(int64_t)(random_word() % 2001) / 1000.0f - 1.0f;
  void *w = quant_weights(shape[2], blocks, block_bytes, scale_at, scales);
  quant_words(type, x, w, shape);
  assert(setenv("OSPREY_GPU", selected, 1) == 0);
  osp_io_error_clear();
  assert(osprey_gpu_matmul_quant(x, w, "Q2_K", shape[0], shape[1], shape[2]) == NULL &&
         strstr(osp_io_error(), "unsupported tensor type"));
  assert(osprey_gpu_matmul_quant(x, w, type, shape[0], shape[1] - block_elements / 2, shape[2]) == NULL &&
         strstr(osp_io_error(), "multiple of"));
  assert(osprey_gpu_matmul_quant(x, w, type, shape[0], shape[1], shape[2] + 1) == NULL && strstr(osp_io_error(), "weights"));
  checks += 3;
}

// The block formats the parity gate covers, each with where its binary16 scales sit.
typedef struct {
  const char *type;
  int64_t bytes;
  int64_t elements;
  int64_t scale_at[2];
  unsigned scales;
} QuantCase;

static const QuantCase QUANT_CASES[] = {
    {"Q4_K", 144, 256, {0, 2}, 2},
    {"Q8_0", 34, 32, {0, 0}, 1},
    {"Q6_K", 210, 256, {208, 0}, 1},
    {"Q5_K", 176, 256, {0, 2}, 2},
    {"Q3_K", 110, 256, {108, 0}, 1},
    {"IQ4_NL", 18, 32, {0, 0}, 1},
    {"IQ4_XS", 136, 256, {0, 0}, 1},
    {"IQ3_S", 110, 256, {0, 0}, 1},
};

// The decoders read whole words, so weights mapped off a 4-byte boundary are an
// error naming the boundary, and the same region one byte later succeeds.
void *osprey_gpu_map_file(const char *path, int64_t offset, int64_t count, int64_t elem_bytes);
void osprey_gpu_finalize(void *buffer);
static void misaligned_weights(void *x, int64_t rows, int64_t inner, int64_t cols) {
  char path[] = "/tmp/osprey_quant_XXXXXX";
  int fd = mkstemp(path);
  const int64_t bytes = cols * 2 * 144;
  assert(fd >= 0 && ftruncate(fd, bytes + 4) == 0 && close(fd) == 0);
  void *odd = osprey_gpu_map_file(path, 1, bytes, 1), *even = osprey_gpu_map_file(path, 4, bytes, 1);
  assert(odd && even && unlink(path) == 0);
  osp_io_error_clear();
  assert(osprey_gpu_matmul_quant(x, odd, "Q4_K", rows, inner, cols) == NULL &&
         strstr(osp_io_error(), "must start on a 4-byte boundary"));
  assert(osprey_gpu_matmul_quant(x, even, "Q4_K", rows, inner, cols) != NULL);
  checks += 2;
}
// One chain of products over mapped weights: the first result feeds the second and
// dies, so the third reuses its device object while the second may still be queued,
// and the mapping goes away before anything is read. Words stay the host's.
static float *dropped_chain(const char *mode, const char *path, void *x, void *other) {
  const int64_t inner = 512, cols = 512, words = cols * 2 * 144;
  assert(setenv("OSPREY_GPU", mode, 1) == 0);
  void *mapped = osprey_gpu_map_file(path, 0, words, 1);
  void *first = osprey_gpu_matmul_quant(x, mapped, "Q4_K", 1, inner, cols);
  void *second = osprey_gpu_matmul_quant(first, mapped, "Q4_K", 1, inner, cols);
  osprey_gpu_finalize(first);
  void *third = osprey_gpu_matmul_quant(other, mapped, "Q4_K", 1, inner, cols);
  osprey_gpu_finalize(mapped);
  assert(second && third);
  float *both = malloc((size_t)cols * 2 * sizeof(float));
  assert(both);
  memcpy(both, osprey_gpu_data(second), (size_t)cols * sizeof(float));
  memcpy(both + cols, osprey_gpu_data(third), (size_t)cols * sizeof(float));
  return both;
}

static void dropped_buffers(const QuantCase *c) {
  const int64_t cols = 512, inner = 512, blocks = 2;
  void *w = quant_weights(cols, blocks, c->bytes, c->scale_at, c->scales);
  void *x = osprey_gpu_alloc(inner, 4), *other = osprey_gpu_alloc(inner, 4);
  float *xs = osprey_gpu_data(x), *os = osprey_gpu_data(other);
  for (int64_t i = 0; i < inner; i++) {
    xs[i] = (float)(int64_t)(random_word() % 2001) / 1000.0f - 1.0f;
    os[i] = (float)(int64_t)(random_word() % 2001) / 1000.0f - 1.0f;
  }
  char path[] = "/tmp/osprey_quant_XXXXXX";
  int fd = mkstemp(path);
  assert(fd >= 0 && write(fd, osprey_gpu_data(w), (size_t)(cols * blocks * c->bytes)) == cols * blocks * c->bytes && close(fd) == 0);
  float *host = dropped_chain("host", path, x, other), *device = dropped_chain(selected, path, x, other);
  assert(unlink(path) == 0);
  assert(memcmp(host, device, (size_t)cols * 2 * sizeof(float)) == 0);
  checks++;
  free(host);
  free(device);
}

// A file past the sharing threshold: regions of it share one mapped object, a
// dropped region leaves the others readable, and the words stay the host's.
static float *shared_regions(const char *mode, const char *path, int64_t at, int64_t bytes, void *x) {
  const int64_t inner = 512, cols = 512;
  assert(setenv("OSPREY_GPU", mode, 1) == 0);
  void *dropped = osprey_gpu_map_file(path, at, bytes, 1), *kept = osprey_gpu_map_file(path, at, bytes, 1);
  void *first = osprey_gpu_matmul_quant(x, dropped, "Q4_K", 1, inner, cols);
  osprey_gpu_finalize(dropped);
  void *second = osprey_gpu_matmul_quant(first, kept, "Q4_K", 1, inner, cols);
  assert(second);
  float *words = malloc((size_t)cols * sizeof(float));
  assert(words);
  memcpy(words, osprey_gpu_data(second), (size_t)cols * sizeof(float));
  return words;
}

static void shared_file(const QuantCase *c) {
  const int64_t cols = 512, inner = 512, blocks = 2, bytes = cols * blocks * c->bytes, at = (int64_t)1 << 30;
  void *w = quant_weights(cols, blocks, c->bytes, c->scale_at, c->scales);
  void *x = osprey_gpu_alloc(inner, 4);
  float *xs = osprey_gpu_data(x);
  for (int64_t i = 0; i < inner; i++) xs[i] = (float)(int64_t)(random_word() % 2001) / 1000.0f - 1.0f;
  char path[] = "/tmp/osprey_quant_XXXXXX";
  int fd = mkstemp(path);
  assert(fd >= 0 && pwrite(fd, osprey_gpu_data(w), (size_t)bytes, at) == bytes && close(fd) == 0);
  float *host = shared_regions("host", path, at, bytes, x), *device = shared_regions(selected, path, at, bytes, x);
  assert(unlink(path) == 0);
  assert(memcmp(host, device, (size_t)cols * sizeof(float)) == 0);
  checks++;
  free(host);
  free(device);
}

static void quantized_products(void) {
  for (size_t i = 0; i < sizeof QUANT_CASES / sizeof QUANT_CASES[0]; i++) {
    const QuantCase *c = &QUANT_CASES[i];
    quant_parity(c->type, c->bytes, c->elements, c->scale_at, c->scales);
  }
  const int64_t rows = 3, inner = 512, cols = 5;
  void *x = osprey_gpu_alloc(rows * inner, 4), *w = osprey_gpu_alloc(cols * 2 * 144, 1);
  osp_io_error_clear();
  assert(osprey_gpu_matmul_quant(x, w, "Q2_K", rows, inner, cols) == NULL &&
         strstr(osp_io_error(), "expected Q4_K, Q8_0, Q6_K, Q5_K, Q3_K, IQ4_NL, IQ4_XS, IQ3_S"));
  checks++;
  misaligned_weights(x, rows, inner, cols);
  dropped_buffers(&QUANT_CASES[0]);
  shared_file(&QUANT_CASES[0]);
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
  if (strcmp(selected, "opencl") != 0) assert(osprey_gpu_metal_completed() == 24);
#endif
  native_float32_rounding();
#if defined(OSPREY_GPU_METAL)
  if (strcmp(selected, "opencl") != 0) assert(osprey_gpu_metal_completed() == 25);
#endif
  native_byte_transfers();
#if defined(OSPREY_GPU_METAL)
  if (strcmp(selected, "opencl") != 0) assert(osprey_gpu_metal_completed() == 26);
#endif
  native_float32_products();
#if defined(OSPREY_GPU_METAL)
  if (strcmp(selected, "opencl") != 0) assert(osprey_gpu_metal_completed() == 29);
#endif
  printf("[ok] device parity: %u checks over binary64 and binary32 products, compiled kernels, reductions, faults, byte transfers and the quantized products\n", checks);
  return 0;
}
