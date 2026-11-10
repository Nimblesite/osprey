// File-backed weights on the device [LLM-WEIGHTS-MAPPED]: misaligned regions,
// mapped buffers dropped mid-chain, and regions of one shared whole-file
// mapping, each held to the host oracle. Included once by gpu_device_tests.c,
// after its quantized cases and helpers.
#ifndef OSPREY_GPU_DEVICE_FILE_TESTS_H
#define OSPREY_GPU_DEVICE_FILE_TESTS_H

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

#endif
