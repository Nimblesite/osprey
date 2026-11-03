// Device selection and the compiled-kernel launch contract against mock
// adapters [GPU-DEVICE] [GPU-BACKEND-DEVICE] [GPU-DEVICE-FAULT]
// [GPU-BUFFER-RESIDENT]: the host oracle runs when the host is selected, a
// selected device never falls back to it, a buffer is uploaded once and its
// result downloaded once, when first read, and a fault or a kernel without a
// device form aborts by name.
#include "gpu_device.h"
#include "gpu_opencl_api.h"
#include "io_error.h"
#include "test_death.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

void *osprey_gpu_alloc(int64_t length, int64_t elem_bytes);
int64_t osprey_gpu_elem_bytes(void *buffer);
int64_t osprey_gpu_get(void *buffer, int64_t index);
void osprey_gpu_set(void *buffer, int64_t index, int64_t word);
void *osprey_gpu_matmul(void *left, void *right, int64_t rows, int64_t inner, int64_t cols);
int32_t osprey_gpu_launch(const char *kernel, const char *source, const char *kinds, const int64_t *uniforms,
                          void *in0, void *in1, void *out, int64_t *acc, int64_t count);
int32_t osprey_gpu_declined(const char *what);
void osprey_gpu_finalize(void *buffer);
int osprey_gpu_buffer_tests(int argc, char **argv);

static int device_error;
static unsigned dispatches, last_args, uploads, downloads, releases;
static uint64_t last_threads;

#if defined(OSPREY_GPU_METAL) || defined(OSPREY_GPU_OPENCL)
static int64_t injected_fault;
// The mock device holds a buffer as a copy of its bytes [GPU-BUFFER-ELEM].
typedef struct {
  uint64_t count;
  unsigned char bytes[];
} MockObject;

static MockObject *mock_object(const OspreyGpuArg *arg) {
  MockObject *o = malloc(sizeof *o + osprey_gpu_arg_bytes(arg));
  assert(o);
  o->count = arg->count;
  memcpy(o->bytes, arg->bytes, osprey_gpu_arg_bytes(arg));
  uploads++;
  return o;
}

// The object an argument is bound to: its slot's, created on first use; a
// transient argument's lives for the dispatch.
static MockObject *bind(const OspreyGpuArg *arg) {
  if (!arg->slot) return mock_object(arg);
  if (!*arg->slot) *arg->slot = mock_object(arg);
  return *arg->slot;
}

// The mock device "runs" a kernel by stamping word 0 of every output and
// recording the injected fault, if any, in the fault word (argument 3); the
// outputs the host asked for are copied back, the rest stay in their slots.
static int mock_dispatch(const OspreyGpuDispatch *d, int64_t mark) {
  assert(d->kernel && d->count >= 1 && d->threads >= 1);
  dispatches++;
  last_args = d->count;
  last_threads = d->threads;
  for (unsigned i = 0; i < d->count; i++) {
    const OspreyGpuArg *arg = &d->args[i];
    if (arg->kind == OSPREY_GPU_SCALAR) continue;
    MockObject *o = bind(arg);
    if (arg->kind == OSPREY_GPU_OUTPUT) {
      int64_t stamp = i == 3 ? injected_fault : mark;
      memcpy(o->bytes, &stamp, sizeof stamp);
    }
    if (arg->kind == OSPREY_GPU_OUTPUT && arg->out) memcpy(arg->out, o->bytes, osprey_gpu_arg_bytes(arg));
    if (!arg->slot) free(o);
  }
  return device_error ? -1 : 1;
}

static int mock_download(void *object, void *out, uint64_t bytes) {
  downloads++;
  memcpy(out, ((MockObject *)object)->bytes, bytes);
  return 1;
}

static void mock_release(void *object) {
  releases++;
  free(object);
}
#endif
#if defined(OSPREY_GPU_METAL)
const char *osprey_gpu_metal_name(void) { return "metal:mock"; }
int osprey_gpu_metal_dispatch(const OspreyGpuDispatch *d) { return mock_dispatch(d, 42); }
int osprey_gpu_metal_download(void *object, void *out, uint64_t bytes) { return mock_download(object, out, bytes); }
void osprey_gpu_metal_release(void *object) { mock_release(object); }
#endif
#if defined(OSPREY_GPU_OPENCL)
const char *osprey_gpu_opencl_name(void) { return "opencl:mock"; }
int osprey_gpu_opencl_dispatch(const OspreyGpuDispatch *d) { return mock_dispatch(d, 43); }
int osprey_gpu_opencl_download(void *object, void *out, uint64_t bytes) { return mock_download(object, out, bytes); }
void osprey_gpu_opencl_release(void *object) { mock_release(object); }
#endif

static void select_mode(const char *mode) {
  if (mode) assert(setenv("OSPREY_GPU", mode, 1) == 0);
  else assert(unsetenv("OSPREY_GPU") == 0);
}

static int64_t one_word(void) {
  double one = 1.0;
  int64_t word;
  memcpy(&word, &one, sizeof word);
  return word;
}

// A one-element float buffer holding 1.0.
static void *unit(void) {
  void *b = osprey_gpu_alloc(1, 8);
  osprey_gpu_set(b, 0, one_word());
  return b;
}

static int32_t launch(void *out, const char *kinds, const int64_t *uniforms) {
  void *in = osprey_gpu_alloc(3, 8);
  return osprey_gpu_launch("osprey_kernel_1", "kernel void osprey_kernel_1() {}", kinds, uniforms, in, NULL, out, NULL, 3);
}

static void host_case(const char *mode) {
  select_mode(mode);
  void *a = unit(), *out = osprey_gpu_alloc(1, 8);
  osprey_gpu_set(out, 0, 99);
  unsigned before = dispatches;
  assert(strcmp(osprey_gpu_device_name(), "host") == 0);
  assert(osprey_gpu_get(osprey_gpu_matmul(a, a, 1, 1, 1), 0) == one_word() && dispatches == before);
  assert(launch(out, "", NULL) == 0 && osprey_gpu_get(out, 0) == 99);
  assert(osprey_gpu_declined("gpuScan") == 0);
}

// The product's inputs went up once (the same buffer twice binds one object)
// and its result comes down once, when first read; a dropped buffer releases
// its device copy exactly once.
static void resident_product(void *a, void *product, int64_t mark) {
  unsigned up = uploads, down = downloads;
  assert(osprey_gpu_get(product, 0) == mark && downloads == down + 1);
  assert(osprey_gpu_get(product, 0) == mark && downloads == down + 1);
  void *again = osprey_gpu_matmul(a, a, 1, 1, 1);
  assert(again && uploads == up + 1);
  unsigned freed = releases;
  osprey_gpu_finalize(again);
  osprey_gpu_finalize(again);
  assert(releases == freed + 1);
}

// A launch's result stays on the device until read, a buffer uniform binds
// as a resident input with its length, a fold's accumulator comes straight
// back, and a host write forgets the device copy.
static void resident_launch(int64_t mark) {
  void *out = osprey_gpu_alloc(1, 8), *table = osprey_gpu_alloc(4, 8), *in = osprey_gpu_alloc(3, 8);
  int64_t uniforms[2] = {7, (int64_t)(intptr_t)table}, acc = 5;
  unsigned down = downloads;
  assert(launch(out, "sb", uniforms) == 1 && downloads == down);
  assert(last_args == 5 + 1 + 2 && last_threads == 3);
  assert(osprey_gpu_get(out, 0) == mark && downloads == down + 1);
  assert(osprey_gpu_launch("osprey_kernel_1", "kernel void osprey_kernel_1() {}", "", NULL, in, NULL, NULL, &acc, 3) == 1);
  assert(acc == mark && last_threads == 1 && downloads == down + 1);
  unsigned freed = releases;
  osprey_gpu_set(out, 0, 5);
  assert(releases == freed + 1 && osprey_gpu_get(out, 0) == 5 && downloads == down + 1);
}

static void device_case(const char *mode, int available, const char *name) {
  select_mode(mode);
  void *a = unit();
  osp_io_error_clear();
  assert(strcmp(osprey_gpu_device_name(), name) == 0);
  void *product = osprey_gpu_matmul(a, a, 1, 1, 1);
  if (!available) {
    assert(!product && strstr(osp_io_error(), "no GPU adapter") != NULL);
    return;
  }
  assert(product && last_args == 6 && last_threads == 1);
  resident_product(a, product, name[0] == 'm' ? 42 : 43);
  device_error = 1;
  assert(!osprey_gpu_matmul(a, a, 1, 1, 1));
  device_error = 0;
  unsigned before = dispatches;
  assert(osprey_gpu_matmul(osprey_gpu_alloc(0, 8), a, 0, 1, 1) && dispatches == before);
  resident_launch(name[0] == 'm' ? 42 : 43);
}

static void invalid_case(void) {
  select_mode("invalid");
  void *a = unit();
  assert(strcmp(osprey_gpu_device_name(), "invalid:device") == 0);
  assert(!osprey_gpu_matmul(a, a, 1, 1, 1) && strstr(osp_io_error(), "unknown backend") != NULL);
}

// Runs `scenario` in a child that must abort with `expected` on stderr.
static void aborts(void (*scenario)(void), const char *expected) {
  int pipes[2];
  assert(pipe(pipes) == 0);
  pid_t child = fork();
  assert(child >= 0);
  if (!child) {
    // The guard's own lines count: dump the coverage counters before the abort.
    (void)signal(SIGABRT, osp_death_dump_and_reraise);
    assert(dup2(pipes[1], 2) >= 0);
    scenario();
    _exit(0);
  }
  close(pipes[1]);
  char text[1024] = {0};
  ssize_t n = read(pipes[0], text, sizeof text - 1);
  close(pipes[0]);
  int status = 0;
  assert(waitpid(child, &status, 0) == child && n > 0);
  assert(WIFSIGNALED(status) && strstr(text, expected) != NULL);
}

static void declined_on_device(void) {
  select_mode("auto");
  (void)osprey_gpu_declined("gpuScan over a kernel the device cannot run");
}
#if defined(OSPREY_GPU_METAL)
static void faulting_launch(void) {
  select_mode("auto");
  injected_fault = OSPREY_GPU_FAULT_OVERFLOW;
  (void)launch(osprey_gpu_alloc(1, 8), "", NULL);
}
static void too_many_uniforms(void) {
  select_mode("auto");
  int64_t uniforms[OSPREY_GPU_MAX_ARGS] = {0};
  char kinds[OSPREY_GPU_MAX_ARGS + 1];
  memset(kinds, 's', OSPREY_GPU_MAX_ARGS);
  kinds[OSPREY_GPU_MAX_ARGS] = '\0';
  (void)launch(osprey_gpu_alloc(1, 8), kinds, uniforms);
}
static void device_failure_on_launch(void) {
  select_mode("auto");
  device_error = 1;
  (void)launch(osprey_gpu_alloc(1, 8), "", NULL);
}
#endif

int main(void) {
  host_case(NULL); host_case(""); host_case("host");
  assert(osprey_gpu_buffer_tests(0, NULL) == 0);
#if defined(OSPREY_GPU_METAL)
  device_case("auto", 1, "metal:mock"); device_case("metal", 1, "metal:mock");
  aborts(faulting_launch, "kernel osprey_kernel_1: integer overflow");
  aborts(declined_on_device, "gpuScan over a kernel the device cannot run; a selected device cannot run it");
  aborts(too_many_uniforms, "too many kernel arguments");
  aborts(device_failure_on_launch, "kernel osprey_kernel_1:");
#else
  device_case("auto", 0, "unavailable:device"); device_case("metal", 0, "unavailable:device");
  aborts(declined_on_device, "a selected device cannot run it");
#endif
#if defined(OSPREY_GPU_OPENCL)
  device_case("opencl", 1, "opencl:mock");
#else
  device_case("opencl", 0, "unavailable:device");
#endif
  invalid_case();
  assert(unsetenv("OSPREY_GPU_TRACE") == 0 && osprey_gpu_traced(1) && !osprey_gpu_traced(2));
  assert(setenv("OSPREY_GPU_TRACE", "0", 1) == 0 && !osprey_gpu_traced(2));
  assert(setenv("OSPREY_GPU_TRACE", "1", 1) == 0 && osprey_gpu_traced(2));
  puts("[ok] GPU selection: host, auto, Metal, OpenCL, missing adapter, compiled-kernel launch, resident buffers uploaded once and read back on demand, faults and declines abort by name, trace switch and no silent CPU fallback");
  return 0;
}
