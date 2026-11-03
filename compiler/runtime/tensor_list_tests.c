// Assertion-driven tests for tensor_list.c, `loadTensor` [BUILTIN-FILE-TENSOR]:
// the host loader must satisfy the same safetensors contract as
// `gpuLoadTensor` (tensor_file_tests.h) while building a List<float> and
// linking no GPU unit at all — this suite links tensor_list.c, tensor_file.c,
// list_runtime.c, file_runtime.c (the io_error channel) and memory_runtime.c,
// and nothing named gpu_*.
#include <assert.h>
#include <signal.h>

#include "collection_runtime.h"
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
  t_panic_aborts();
  printf("[ok] tensor_list: %ld assertions\n", g_checks);
  return 0;
}
