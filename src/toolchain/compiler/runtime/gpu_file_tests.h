// The file-backed cases of gpu_runtime_tests.c [LLM-WEIGHTS-MAPPED]
// [GPU-BUFFER-LOAD]: mapped regions at every offset and width, their refusals,
// shared whole-file mappings, and binary32 loads. Included once, after its
// CHECK, temp_file and expect_errno, the way tensor_file_tests.h is.
#ifndef OSPREY_GPU_FILE_TESTS_H
#define OSPREY_GPU_FILE_TESTS_H

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
    osprey_gpu_finalize(b); // unmaps: no word stays reachable once it is dropped
    CHECK(osprey_gpu_data(b) == NULL);
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
    osprey_gpu_finalize(second); // a shared region gives back its slot, never the file's mapping
    CHECK(osprey_gpu_data(second) == NULL);
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

#endif
