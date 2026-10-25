// The safetensors fixture and the loader-agnostic test body every tensor
// loader suite shares: gpu_runtime_tests.c runs it over `gpuLoadTensor`'s
// GpuBuffer, tensor_list_tests.c over `loadTensor`'s List<float> — the same
// reader (tensor_file.c) behind both, so both must widen, match and refuse
// identically. The including suite defines CHECK and declares osp_io_error
// and osp_io_error_clear first.
#ifndef OSPREY_TENSOR_FILE_TESTS_H
#define OSPREY_TENSOR_FILE_TESTS_H

#include <errno.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

// One loader under test: its op name in failure messages, the load itself,
// and how to measure and read what it returned.
typedef struct {
  const char *op;
  void *(*load)(const char *path, const char *name);
  int64_t (*length)(void *tensor);
  double (*at)(void *tensor, int64_t i);
} TensorLoader;

// Every failure records exactly one sentence, "op: subject: reason".
static void expect_text(const char *message) {
  const char *actual = osp_io_error();
  if (!actual || strcmp(actual, message) != 0) {
    fprintf(stderr, "expected: %s\n  actual: %s\n", message, actual ? actual : "(none)");
  }
  CHECK(actual != NULL && strcmp(actual, message) == 0);
}

// An errno failure records "op: subject: " followed by the OS's own wording.
static void expect_errno(const char *prefix, int err) {
  const char *message = osp_io_error();
  CHECK(message != NULL);
  CHECK(strncmp(message, prefix, strlen(prefix)) == 0);
  CHECK(strstr(message, strerror(err)) != NULL);
}

static void put_u16(unsigned char *at, uint16_t v) {
  at[0] = (unsigned char)(v & 0xff);
  at[1] = (unsigned char)(v >> 8);
}

static void put_u32(unsigned char *at, uint32_t v) {
  put_u16(at, (uint16_t)(v & 0xffff));
  put_u16(at + 2, (uint16_t)(v >> 16));
}

static void put_u64(unsigned char *at, uint64_t v) {
  put_u32(at, (uint32_t)(v & 0xffffffffu));
  put_u32(at + 4, (uint32_t)(v >> 32));
}

static void put_f32(unsigned char *at, float f) {
  uint32_t bits;
  memcpy(&bits, &f, sizeof bits);
  put_u32(at, bits);
}

static void put_f64(unsigned char *at, double d) {
  uint64_t bits;
  memcpy(&bits, &d, sizeof bits);
  put_u64(at, bits);
}

// `bytes` written to a fresh temporary file whose path is returned (static).
static const char *temp_file(const unsigned char *bytes, size_t n) {
  static char path[64];
  (void)snprintf(path, sizeof path, "/tmp/osprey_tensor_XXXXXX");
  int fd = mkstemp(path);
  CHECK(fd >= 0);
  FILE *f = fdopen(fd, "wb");
  CHECK(f != NULL);
  CHECK(fwrite(bytes, 1, n, f) == n);
  CHECK(fclose(f) == 0);
  return path;
}

// The safetensors header this suite reads: `__metadata__` LAST, a name that
// is a prefix of another (`h.1.w` inside `h.11.w`), every admitted dtype, a
// refused one, and three malformed entries. Data offsets are relative to the
// data section. Returns the complete file image's length.
static size_t safetensors_image(unsigned char *out, size_t cap) {
  const char *header =
      "{\"a\":{\"dtype\":\"F32\",\"shape\":[2,3],\"data_offsets\":[0,24]},"
      "\"h.1.w\":{\"dtype\":\"F16\",\"shape\":[5],\"data_offsets\":[24,34]},"
      "\"h.11.w\":{\"dtype\":\"BF16\",\"shape\":[2],\"data_offsets\":[34,38]},"
      "\"d\":{\"dtype\":\"F64\",\"shape\":[2],\"data_offsets\":[38,54]},"
      "\"i\":{\"dtype\":\"I64\",\"shape\":[1],\"data_offsets\":[54,62]},"
      "\"inv\":{\"dtype\":\"F32\",\"shape\":[1],\"data_offsets\":[8,4]},"
      "\"odd\":{\"dtype\":\"F32\",\"shape\":[1],\"data_offsets\":[0,6]},"
      "\"beyond\":{\"dtype\":\"F32\",\"shape\":[250],\"data_offsets\":[0,1000]},"
      "\"__metadata__\":{\"format\":\"pt\"}}";
  size_t n = strlen(header);
  CHECK(8 + n + 62 <= cap);
  put_u64(out, (uint64_t)n);
  memcpy(out + 8, header, n);
  unsigned char *data = out + 8 + n;
  float a[] = {0.5f, -1.0f, 2.0f, 3.0f, -4.5f, 6.0f};
  for (size_t i = 0; i < 6; i++) {
    put_f32(data + i * 4, a[i]);
  }
  uint16_t f16[] = {0x3C00, 0xC100, 0x0001, 0x7C00, 0x7E00}; // 1, -2.5, 2^-24, inf, nan
  for (size_t i = 0; i < 5; i++) {
    put_u16(data + 24 + i * 2, f16[i]);
  }
  put_u16(data + 34, 0x3F80); // bf16 1.0
  put_u16(data + 36, 0xC040); // bf16 -3.0
  put_f64(data + 38, 3.25);
  put_f64(data + 46, -1e300);
  put_u64(data + 54, 7);
  return 8 + n + 62;
}

// "op: subject: reason" for this loader.
static void expect_reason(const TensorLoader *t, const char *subject, const char *reason) {
  char message[160];
  (void)snprintf(message, sizeof message, "%s: %s: %s", t->op, subject, reason);
  expect_text(message);
}

// The load of `name` fails, naming `subject` and `reason`.
static void refuses(const TensorLoader *t, const char *path, const char *name, const char *subject,
                    const char *reason) {
  osp_io_error_clear();
  CHECK(t->load(path, name) == NULL);
  expect_reason(t, subject, reason);
}

// JSON permits whitespace around punctuation in names, dtypes and offsets.
static void t_whitespace_header(const TensorLoader *t) {
  const char *header = "{ \"w\" : { \"dtype\" : \"F32\", \"shape\" : [1], \"data_offsets\" : [0, 4] } }";
  unsigned char image[256] = {0};
  size_t n = ((strlen(header) + 7) / 8) * 8;
  put_u64(image, (uint64_t)n);
  memset(image + 8, ' ', n);
  memcpy(image + 8, header, strlen(header));
  put_f32(image + 8 + n, 2.5f);
  const char *path = temp_file(image, 8 + n + 4);
  void *tensor = t->load(path, "w");
  CHECK(unlink(path) == 0);
  CHECK(tensor != NULL);
  CHECK(t->length(tensor) == 1);
  CHECK(t->at(tensor, 0) == 2.5);
}

// Each admitted dtype widens exactly and the scanner matches whole quoted
// names regardless of entry order.
static void t_tensor_widens(const TensorLoader *t, const char *path) {
  osp_io_error_clear();
  void *a = t->load(path, "a");
  CHECK(a != NULL && t->length(a) == 6 && osp_io_error() == NULL);
  CHECK(t->at(a, 0) == 0.5 && t->at(a, 1) == -1.0 && t->at(a, 4) == -4.5 && t->at(a, 5) == 6.0);
  void *h1 = t->load(path, "h.1.w");
  CHECK(h1 != NULL && t->length(h1) == 5);
  CHECK(t->at(h1, 0) == 1.0 && t->at(h1, 1) == -2.5 && t->at(h1, 2) == ldexp(1.0, -24));
  CHECK(isinf(t->at(h1, 3)) && t->at(h1, 3) > 0 && isnan(t->at(h1, 4)));
  void *h11 = t->load(path, "h.11.w");
  CHECK(h11 != NULL && t->length(h11) == 2 && t->at(h11, 0) == 1.0 && t->at(h11, 1) == -3.0);
  void *d = t->load(path, "d");
  CHECK(d != NULL && t->length(d) == 2 && t->at(d, 0) == 3.25 && t->at(d, 1) == -1e300);
}

// Every malformed entry is a named error, never a shorter tensor.
static void t_tensor_entry_errors(const TensorLoader *t, const char *path) {
  refuses(t, path, "i", "i", "dtype is not F64, F32, F16 or BF16");
  refuses(t, path, "__metadata__", "__metadata__", "dtype is not F64, F32, F16 or BF16");
  refuses(t, path, "h.1", "h.1", "no tensor of that name in the header");
  refuses(t, path, "inv", "inv", "data_offsets are missing or inverted");
  refuses(t, path, "odd", "odd", "byte span is not a whole number of elements");
  refuses(t, path, NULL, "<null path>", "no tensor of that name in the header");
  osp_io_error_clear();
  CHECK(t->load(path, "beyond") == NULL);
  CHECK(strstr(osp_io_error(), "file ends before the tensor does") != NULL);
}

// A file that is not a safetensors file is refused at the first byte that
// proves it: too short for the prefix, an implausible header length, or a
// header the file ends inside of.
static void t_tensor_malformed_files(const TensorLoader *t) {
  unsigned char stub[16] = {0};
  const char *tiny = temp_file(stub, 4);
  osp_io_error_clear();
  CHECK(t->load(tiny, "a") == NULL);
  CHECK(strstr(osp_io_error(), "shorter than a safetensors header prefix") != NULL);
  (void)unlink(tiny);
  put_u64(stub, (uint64_t)200 * 1024 * 1024);
  const char *huge = temp_file(stub, 16);
  osp_io_error_clear();
  CHECK(t->load(huge, "a") == NULL);
  CHECK(strstr(osp_io_error(), "header length is not plausible for safetensors") != NULL);
  (void)unlink(huge);
  put_u64(stub, 100);
  const char *cut = temp_file(stub, 16);
  osp_io_error_clear();
  CHECK(t->load(cut, "a") == NULL);
  CHECK(strstr(osp_io_error(), "file ends inside the header") != NULL);
  (void)unlink(cut);
  char prefix[96];
  (void)snprintf(prefix, sizeof prefix, "%s: /nonexistent/osprey/model.safetensors: ", t->op);
  osp_io_error_clear();
  CHECK(t->load("/nonexistent/osprey/model.safetensors", "a") == NULL);
  expect_errno(prefix, ENOENT);
}

// The whole contract, over one loader.
static void t_tensor_loader(const TensorLoader *t) {
  t_whitespace_header(t);
  unsigned char image[1024];
  const char *path = temp_file(image, safetensors_image(image, sizeof image));
  t_tensor_widens(t, path);
  t_tensor_entry_errors(t, path);
  (void)unlink(path);
  t_tensor_malformed_files(t);
}

#endif
