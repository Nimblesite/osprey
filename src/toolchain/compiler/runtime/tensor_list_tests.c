// Assertion-driven tests for tensor_list.c, `loadTensor` [BUILTIN-FILE-TENSOR]:
// the host loader must satisfy the same safetensors contract as
// `gpuLoadTensor` (tensor_file_tests.h) while building a List<float> and
// linking no GPU unit at all — this suite links tensor_list.c, tensor_file.c,
// list_runtime.c, file_runtime.c (the io_error channel) and memory_runtime.c,
// and nothing named gpu_*.
#include <assert.h>
#include <signal.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#include "collection_runtime.h"
#include "float_spelling.h"
#include "memory_hooks.h"
#include "test_death.h"

OspreyList *osprey_load_tensor(const char *path, const char *name);
OspreyList *osprey_read_file_bytes(const char *path, int64_t offset, int64_t count);
int64_t osprey_file_size(const char *path);
void *osprey_tensor_panic(const char *message);
const char *osp_io_error(void);
void osp_io_error_clear(void);

static long g_checks = 0;
#define CHECK(c)                                                               \
  do {                                                                         \
    g_checks++;                                                                \
    assert(c);                                                                 \
  } while (0)

#include "tensor_file_tests.h"

// The host list widens every dtype to float [BUILTIN-FILE-TENSOR], so the
// asked-for element width is not part of its contract and is ignored here.
static void *load(const char *path, const char *name, int64_t elem) {
  (void)elem;
  return osprey_load_tensor(path, name);
}

static int64_t length(void *list) {
  return osprey_list_length(list);
}

// The double a List<float> element word holds.
static double at(void *list, int64_t i) {
  CHECK(osprey_list_in_bounds(list, i));
  int64_t word = osprey_list_get(list, i);
  double d;
  memcpy(&d, &word, sizeof d);
  return d;
}

// [GPU-BUFFER-TENSOR] Exercise the shared reader through real safetensors IO.
#define HALF_WORDS 65536
static const char *half_fixture(void) {
  const char header[] = "{\"half\":{\"dtype\":\"F16\",\"shape\":[65536],\"data_offsets\":[0,131072]}}";
  size_t prefix = 8 + strlen(header), count = prefix + 2 * HALF_WORDS;
  unsigned char *image = malloc(count);
  CHECK(image != NULL);
  put_u64(image, (uint64_t)strlen(header));
  memcpy(image + 8, header, strlen(header));
  for (uint32_t bits = 0; bits < HALF_WORDS; bits++) put_u16(image + prefix + 2 * bits, (uint16_t)bits);
  const char *path = temp_file(image, count);
  free(image);
  return path;
}

static void expect_same_float(double actual, double expected) {
  if (isnan(expected)) CHECK(isnan(actual) && signbit(actual) == signbit(expected));
  else CHECK(memcmp(&actual, &expected, sizeof actual) == 0);
}

static void t_half_tensor_matches_numeric_widening(void) {
  const char *path = half_fixture();
  OspreyList *list = osprey_load_tensor(path, "half");
  CHECK(list != NULL && osprey_list_length(list) == HALF_WORDS);
  for (uint32_t bits = 0; bits < HALF_WORDS; bits++) {
    expect_same_float(at(list, bits), (double)osp_f16_to_f32((uint16_t)bits));
  }
  osp_release(list);
  CHECK(unlink(path) == 0);
}

static void panic_named(void) {
  (void)osprey_tensor_panic("loadTensor: w: no tensor of that name in the header");
}

static void panic_unnamed(void) {
  (void)osprey_tensor_panic(NULL);
}

// Broken-code quarantine: the CPU handler's failed load aborts rather than
// hand the model a fabricated tensor, with or without a recorded reason.
static void t_panic_aborts(void) {
  CHECK(osp_death_signal(panic_named) == SIGABRT);
  CHECK(osp_death_signal(panic_unnamed) == SIGABRT);
}

// [BUILTIN-FILE-BYTES] Bytes come back unsigned and whole through every NUL; a
// count past the end stops at the end; the caller's bad numbers and a missing
// file are errors naming the operation and the subject.
static void t_read_file_bytes(void) {
  unsigned char image[1024];
  for (size_t i = 0; i < sizeof image; i++) image[i] = (unsigned char)(i * 7);
  image[0] = 0;
  image[1] = 255;
  const char *path = temp_file(image, sizeof image);
  static const struct { int64_t offset, count, expected; } spans[] = {
      {0, 1024, 1024}, {0, 1, 1}, {1, 1, 1}, {1000, 24, 24}, {1000, 100, 24}, {1023, 5, 1}, {1024, 5, 0}, {5000, 5, 0}, {7, 0, 0},
  };
  for (size_t s = 0; s < sizeof spans / sizeof spans[0]; s++) {
    osp_io_error_clear();
    OspreyList *list = osprey_read_file_bytes(path, spans[s].offset, spans[s].count);
    CHECK(list != NULL);
    CHECK(osp_io_error() == NULL);
    CHECK(osprey_list_length(list) == spans[s].expected);
    for (int64_t i = 0; i < spans[s].expected; i++) {
      CHECK(osprey_list_get(list, i) == image[spans[s].offset + i]);
    }
    osp_release(list);
  }
  static const struct { int64_t offset, count; const char *reason; } refused[] = {
      {-1, 1, "a negative offset"}, {INT64_MIN, 1, "a negative offset"}, {0, -1, "a negative byte count"}, {0, INT64_MIN, "a negative byte count"},
  };
  for (size_t r = 0; r < sizeof refused / sizeof refused[0]; r++) {
    osp_io_error_clear();
    CHECK(osprey_read_file_bytes(path, refused[r].offset, refused[r].count) == NULL);
    CHECK(strstr(osp_io_error(), "readFileBytes") != NULL);
    CHECK(strstr(osp_io_error(), refused[r].reason) != NULL);
  }
  osp_io_error_clear();
  CHECK(osprey_read_file_bytes("/nonexistent/osprey/weights.bin", 0, 1) == NULL);
  CHECK(strstr(osp_io_error(), "readFileBytes: /nonexistent/osprey/weights.bin: ") != NULL);
  osp_io_error_clear();
  CHECK(osprey_read_file_bytes(NULL, 0, 1) == NULL);
  CHECK(strstr(osp_io_error(), "readFileBytes") != NULL);
  osp_io_error_clear();
  CHECK(osprey_read_file_bytes("/tmp", 0, 1) == NULL);
  CHECK(strstr(osp_io_error(), "readFileBytes: /tmp: ") != NULL);
  // [BUILTIN-FILE-SIZE] The length `stat` reports, and -1 with the reason for
  // a missing path, no path, and a directory.
  osp_io_error_clear();
  CHECK(osprey_file_size(path) == (int64_t)sizeof image);
  CHECK(osp_io_error() == NULL);
  osp_io_error_clear();
  CHECK(osprey_file_size("/nonexistent/osprey/weights.bin") == -1);
  CHECK(strstr(osp_io_error(), "fileSize: /nonexistent/osprey/weights.bin: ") != NULL);
  osp_io_error_clear();
  CHECK(osprey_file_size(NULL) == -1);
  CHECK(strstr(osp_io_error(), "fileSize") != NULL);
  osp_io_error_clear();
  CHECK(osprey_file_size("/tmp") == -1);
  CHECK(strstr(osp_io_error(), "not a regular file") != NULL);
  (void)remove(path);
}

int main(void) {
  t_tensor_loader(&(TensorLoader){"loadTensor", load, length, at});
  t_read_file_bytes();
  t_half_tensor_matches_numeric_widening();
  t_panic_aborts();
  printf("[ok] tensor_list: %ld assertions\n", g_checks);
  return 0;
}
