// Assertion-driven tests for tensor_list.c, `loadTensor` [BUILTIN-FILE-TENSOR]:
// the host loader must satisfy the same safetensors contract as
// `gpuLoadTensor` (tensor_file_tests.h) while building a List<float> and
// linking no GPU unit at all — this suite links tensor_list.c, tensor_file.c,
// list_runtime.c, file_runtime.c (the io_error channel) and memory_runtime.c,
// and nothing named gpu_*.
#include <assert.h>
#include <signal.h>

#include "collection_runtime.h"
#include "float_spelling.h"
#include "memory_hooks.h"
#include "test_death.h"

OspreyList *osprey_load_tensor(const char *path, const char *name);
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

static void *load(const char *path, const char *name) {
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

int main(void) {
  t_tensor_loader(&(TensorLoader){"loadTensor", load, length, at});
  t_half_tensor_matches_numeric_widening();
  t_panic_aborts();
  printf("[ok] tensor_list: %ld assertions\n", g_checks);
  return 0;
}
