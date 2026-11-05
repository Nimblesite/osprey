// Device selection and the compiled-kernel launch contract against mock
// adapters [GPU-DEVICE] [GPU-BACKEND-DEVICE] [GPU-DEVICE-FAULT]
// [GPU-BUFFER-RESIDENT]: the host oracle runs when the host is selected, a
// selected device never falls back to it, a buffer is uploaded once and its
// result downloaded once, when first read, and a fault or a kernel without a
// device form returns a failure with its reason.
#include "gpu_device.h"
#include "gpu_opencl_api.h"
#include "io_error.h"
#include <assert.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

void *osprey_gpu_alloc(int64_t length, int64_t elem_bytes);
void *osprey_gpu_data(void *buffer);
int64_t osprey_gpu_len(void *buffer);
int64_t osprey_gpu_elem_bytes(void *buffer);
int64_t osprey_gpu_get(void *buffer, int64_t index);
void osprey_gpu_set(void *buffer, int64_t index, int64_t word);
void *osprey_gpu_matmul(void *left, void *right, int64_t rows, int64_t inner, int64_t cols, int64_t elem_bytes);
int32_t osprey_gpu_launch(const char *kernel, const char *source, const char *kinds, const int64_t *uniforms,
                          void *in0, void *in1, void *out, int64_t *acc, int64_t count);
int32_t osprey_gpu_declined(const char *what);
void osprey_gpu_finalize(void *buffer);
int osprey_gpu_buffer_tests(int argc, char **argv);

static int device_error;
static unsigned dispatches, last_args, uploads, downloads, releases;
static unsigned releases_by_adapter[4];
static uint64_t last_threads;

#if defined(OSPREY_GPU_METAL) || defined(OSPREY_GPU_OPENCL)
static int64_t injected_fault;
// The mock device holds a buffer as a copy of its words.
typedef struct {
  uint64_t count;
  int64_t words[];
} MockObject;

static MockObject *mock_object(const OspreyGpuArg *arg) {
  MockObject *o = malloc(sizeof *o + osprey_gpu_arg_bytes(arg));
  assert(o);
  o->count = arg->count;
  memcpy(o->words, arg->words, osprey_gpu_arg_bytes(arg));
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
    if (arg->kind == OSPREY_GPU_OUTPUT) o->words[0] = i == 3 ? injected_fault : mark;
    if (arg->kind == OSPREY_GPU_OUTPUT && arg->out) memcpy(arg->out, o->words, arg->count * arg->elem_bytes);
    if (!arg->slot) free(o);
  }
  return device_error ? -1 : 1;
}

static int mock_download(void *object, void *out, uint64_t count) {
  downloads++;
  if (device_error) {
    osp_io_error_text("mock", "readback", "injected download failure");
    return -1;
  }
  memcpy(out, ((MockObject *)object)->words, count);
  return 1;
}

static void mock_release(void *object, unsigned adapter) {
  releases++;
  releases_by_adapter[adapter]++;
  free(object);
}
#endif
#if defined(OSPREY_GPU_METAL)
const char *osprey_gpu_metal_name(void) { return "metal:mock"; }
int osprey_gpu_metal_dispatch(const OspreyGpuDispatch *d) { return mock_dispatch(d, 42); }
int osprey_gpu_metal_download(void *object, void *out, uint64_t count) { return mock_download(object, out, count); }
void osprey_gpu_metal_release(void *object) { mock_release(object, 2); }
void osprey_gpu_metal_recycle(void *object) { mock_release(object, 2); }
#endif
#if defined(OSPREY_GPU_OPENCL)
const char *osprey_gpu_opencl_name(void) { return "opencl:mock"; }
int osprey_gpu_opencl_dispatch(const OspreyGpuDispatch *d) { return mock_dispatch(d, 43); }
int osprey_gpu_opencl_download(void *object, void *out, uint64_t count) { return mock_download(object, out, count); }
void osprey_gpu_opencl_release(void *object) { mock_release(object, 3); }
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

static void exact(void *buffer, int64_t count, const float *want) {
  assert(buffer && osprey_gpu_len(buffer) == count && osprey_gpu_elem_bytes(buffer) == 4);
  const float *got = osprey_gpu_data(buffer);
  assert(got);
  for (int64_t i = 0; i < count; i++) assert(got[i] == want[i]);
}
static void refused(void *result) {
  assert(!result && osp_io_error() && *osp_io_error());
  osp_io_error_clear();
}
static void *floats_of(const float *values, int64_t count) {
  void *buffer = osprey_gpu_alloc(count, 4);
  assert(buffer && osprey_gpu_len(buffer) == count && osprey_gpu_elem_bytes(buffer) == 4);
  memcpy(osprey_gpu_data(buffer), values, (size_t)count * sizeof *values);
  return buffer;
}
// [LLM-MATMUL-QUANT] Fixed host projection, gating and recurrent results.
static void host_quant_zero_case(void *x) {
  static const struct { const char *name; int elements, bytes; } formats[] = {
      {"Q4_K", 256, 144}, {"Q8_0", 32, 34}, {"Q6_K", 256, 210}, {"Q5_K", 256, 176},
      {"Q3_K", 256, 110}, {"IQ4_NL", 32, 18}, {"IQ4_XS", 256, 136}, {"IQ3_S", 256, 110}};
  float zeroed[2] = {0, 0};
  for (size_t i = 0; i < sizeof formats / sizeof formats[0]; i++) {
    int64_t blocks = 2 * (512 / formats[i].elements);
    void *w = osprey_gpu_alloc(blocks * formats[i].bytes, 1);
    assert(w && osprey_gpu_elem_bytes(w) == 1);
    memset(osprey_gpu_data(w), 0, (size_t)blocks * formats[i].bytes);
    void *out = osprey_gpu_matmul_quant(x, w, formats[i].name, 1, 512, 2);
    exact(out, 2, zeroed);
    osprey_gpu_finalize(w); osprey_gpu_finalize(out);
  }
}
static void host_quant_nonzero_case(void *x) {
  int64_t q8blocks = 2 * (512 / 32);
  void *w = osprey_gpu_alloc(q8blocks * 34, 1);
  unsigned char *weights = osprey_gpu_data(w);
  for (int64_t b = 0; b < q8blocks; b++) {
    weights[b * 34] = 0x00; weights[b * 34 + 1] = 0x3c;
    memset(weights + b * 34 + 2, 1, 32);
  }
  float want512[2] = {512.0f, 512.0f}; void *out = osprey_gpu_matmul_quant(x, w, "Q8_0", 1, 512, 2);
  exact(out, 2, want512);
  assert(((float *)osprey_gpu_data(x))[0] == 1.0f && ((float *)osprey_gpu_data(x))[511] == 1.0f);
  assert(weights[0] == 0 && weights[1] == 0x3c && weights[2] == 1 && weights[q8blocks * 34 - 1] == 1);
  refused(osprey_gpu_matmul_quant(x, w, "Q9_0", 1, 512, 2));
  refused(osprey_gpu_matmul_quant(x, w, "Q8_0", 2, 512, 2));
  osprey_gpu_finalize(w); osprey_gpu_finalize(out);
}
static void host_sigmoid_case(void) {
  void *wide = osprey_gpu_alloc(1, 8); refused(osprey_gpu_sigmoid(wide));
  osprey_gpu_finalize(wide);
  float sig_in[3] = {-INFINITY, 0.0f, INFINITY}, sig_want[3] = {0.0f, 0.5f, 1.0f};
  void *sx = floats_of(sig_in, 3), *sy = osprey_gpu_sigmoid(sx);
  exact(sy, 3, sig_want); exact(sx, 3, sig_in);
  osprey_gpu_finalize(sx); osprey_gpu_finalize(sy);
}
static void host_delta_case(void) {
  float zero6[6] = {0}, query[2] = {0.5f, 0.25f}, key[2] = {1.0f, 2.0f}, value[2] = {3.0f, 4.0f};
  float beta[1] = {1.0f}, decay[1] = {1.0f};
  float first[6] = {3, 4, 6, 8, 3, 4}, second[6] = {-9, -12, -18, -24, -9, -12};
  void *state = floats_of(zero6, 6), *q = floats_of(query, 2), *k = floats_of(key, 2);
  void *v = floats_of(value, 2), *b = floats_of(beta, 1), *d = floats_of(decay, 1);
  void *r1 = osprey_gpu_delta_rule(state, q, k, v, b, d, 1, 2);
  exact(r1, 6, first);
  void *r2 = osprey_gpu_delta_rule(r1, q, k, v, b, d, 1, 2);
  exact(r2, 6, second); exact(r1, 6, first);
  exact(state, 6, zero6); exact(q, 2, query); exact(k, 2, key); exact(v, 2, value);
  exact(b, 1, beta); exact(d, 1, decay);
  osprey_gpu_finalize(state); osprey_gpu_finalize(q); osprey_gpu_finalize(k);
  osprey_gpu_finalize(v); osprey_gpu_finalize(b); osprey_gpu_finalize(d);
  osprey_gpu_finalize(r1); osprey_gpu_finalize(r2);
}
static void host_api_workflow(void) {
  select_mode("host"); osp_io_error_clear(); refused(osprey_gpu_alloc(1, 3));
  float ones[512];
  for (int i = 0; i < 512; i++) ones[i] = 1.0f;
  void *x = floats_of(ones, 512);
  host_quant_zero_case(x); host_quant_nonzero_case(x);
  assert(memcmp(osprey_gpu_data(x), ones, sizeof ones) == 0);
  host_sigmoid_case(); host_delta_case();
  osprey_gpu_finalize(x);
}

// A kernel text that hands its fault word to a checked helper, so the launch
// reads the word back at once [GPU-DEVICE-FAULT] instead of joining the batch.
static const char FAULTING_KERNEL[] =
    "kernel void osprey_kernel_1(GPU_GLOBAL ulong *fault GPU_BUFFER(3)) { osprey_add(1, 2, fault); }";

static int32_t launch(void *out, const char *kinds, const int64_t *uniforms) {
  void *in = osprey_gpu_alloc(3, 8);
  return osprey_gpu_launch("osprey_kernel_1", FAULTING_KERNEL, kinds, uniforms, in, NULL, out, NULL, 3);
}

static void host_case(const char *mode) {
  select_mode(mode);
  void *a = unit(), *out = osprey_gpu_alloc(1, 8);
  osprey_gpu_set(out, 0, 99);
  unsigned before = dispatches;
  assert(strcmp(osprey_gpu_device_name(), "host") == 0);
  assert(osprey_gpu_get(osprey_gpu_matmul(a, a, 1, 1, 1, 8), 0) == one_word() && dispatches == before);
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
  void *again = osprey_gpu_matmul(a, a, 1, 1, 1, 8);
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
  assert(osprey_gpu_launch("osprey_kernel_1", FAULTING_KERNEL, "", NULL, in, NULL, NULL, &acc, 3) == 1);
  assert(acc == mark && last_threads == 1 && downloads == down + 1);
  unsigned freed = releases;
  osprey_gpu_set(out, 0, 5);
  assert(releases == freed + 1 && osprey_gpu_get(out, 0) == 5 && downloads == down + 1);
}

// [FAILURE-EXPLICIT]: failed synchronization leaves the buffer pending and
// preserves the reason; invalid indices never ask the device to download.
static void readable_failure(int64_t mark) {
  void *out = osprey_gpu_alloc(1, 8);
  assert(launch(out, "", NULL) == 1);
  unsigned down = downloads;
  device_error = 1;
  osp_io_error_clear();
  assert(osprey_gpu_readable(out, -1) == 0 && osprey_gpu_readable(out, 1) == 0 && downloads == down);
  assert(osprey_gpu_readable(out, 0) == -1 && downloads == down + 1);
  assert(strcmp(osp_io_error(), "mock: readback: injected download failure") == 0);
  device_error = 0;
  osp_io_error_clear();
  assert(osprey_gpu_readable(out, 0) == 1 && downloads == down + 2 && !osp_io_error());
  assert(osprey_gpu_get(out, 0) == mark && downloads == down + 2);
}

// [FAILURE-EXPLICIT] A host matrix product must propagate resident read errors.
static void matmul_readback_failure(const char *mode) {
  void *pending = osprey_gpu_alloc(1, 8), *ready = unit();
  assert(launch(pending, "", NULL) == 1);
  unsigned down = downloads;
  select_mode("host");
  osp_io_error_clear();
  assert(!osprey_gpu_matmul(pending, ready, 1, 1, 1, 8));
  assert(downloads == down && strstr(osp_io_error(), "device readback: no GPU adapter") != NULL);
  osp_io_error_clear();
  assert(!osprey_gpu_matmul(ready, pending, 1, 1, 1, 8));
  assert(downloads == down && strstr(osp_io_error(), "device readback: no GPU adapter") != NULL);
  select_mode(mode);
  osp_io_error_clear();
  assert(osprey_gpu_matmul(pending, ready, 1, 1, 1, 8) && downloads == down && !osp_io_error());
}

// [GPU-BUFFER-RESIDENT] Both uploaded inputs and resident outputs retain
// their owning adapter when selection changes; finalization never changes IO.
// Implements [GPU-BUFFER-RESIDENT-OWNER].
static void finalized_pair(void *input, void *out, unsigned owner) {
  unsigned freed = releases, owned = releases_by_adapter[owner];
  osp_io_error_text("test", "preserved", "read error");
  osprey_gpu_finalize(input);
  osprey_gpu_finalize(out);
  assert(releases == freed + 2 && releases_by_adapter[owner] == owned + 2);
  assert(strcmp(osp_io_error(), "test: preserved: read error") == 0);
  osprey_gpu_finalize(input);
  osprey_gpu_finalize(out);
  assert(releases == freed + 2);
}

static void owned_finalization(const char *mode, unsigned owner) {
  const char *next[] = {"host", "invalid-for-release", owner == 2 ? "opencl" : "metal"};
  for (unsigned i = 0; i < sizeof next / sizeof next[0]; i++) {
    select_mode(mode);
    void *input = unit();
    void *out = osprey_gpu_matmul(input, input, 1, 1, 1, 8);
    assert(out);
    select_mode(next[i]);
    finalized_pair(input, out, owner);
  }
  select_mode(mode);
}

// A host write discards an already synchronized device copy through the same
// owner-aware path, even after selection changes to the host.
static void writable_finalization(const char *mode, unsigned owner) {
  select_mode(mode);
  void *input = unit();
  void *out = osprey_gpu_matmul(input, input, 1, 1, 1, 8);
  assert(out && osprey_gpu_readable(out, 0) == 1);
  unsigned freed = releases, owned = releases_by_adapter[owner];
  select_mode("host");
  osprey_gpu_set(out, 0, 99);
  assert(osprey_gpu_get(out, 0) == 99);
  osprey_gpu_finalize(input);
  osprey_gpu_finalize(out);
  assert(releases == freed + 2 && releases_by_adapter[owner] == owned + 2);
  select_mode(mode);
}

static void device_case(const char *mode, int available, const char *name) {
  select_mode(mode);
  void *a = unit();
  osp_io_error_clear();
  assert(strcmp(osprey_gpu_device_name(), name) == 0);
  void *product = osprey_gpu_matmul(a, a, 1, 1, 1, 8);
  if (!available) {
    assert(!product && strstr(osp_io_error(), "no GPU adapter") != NULL);
    return;
  }
  assert(product && last_args == 6 && last_threads == 1);
  resident_product(a, product, name[0] == 'm' ? 42 : 43);
  device_error = 1;
  assert(!osprey_gpu_matmul(a, a, 1, 1, 1, 8));
  device_error = 0;
  unsigned before = dispatches;
  assert(osprey_gpu_matmul(osprey_gpu_alloc(0, 8), a, 0, 1, 1, 8) && dispatches == before);
  resident_launch(name[0] == 'm' ? 42 : 43);
  readable_failure(name[0] == 'm' ? 42 : 43);
  matmul_readback_failure(mode);
  owned_finalization(mode, name[0] == 'm' ? 2 : 3);
  writable_finalization(mode, name[0] == 'm' ? 2 : 3);
}

static void invalid_case(void) {
  select_mode("invalid");
  void *a = unit();
  assert(strcmp(osprey_gpu_device_name(), "invalid:device") == 0);
  assert(!osprey_gpu_matmul(a, a, 1, 1, 1, 8) && strstr(osp_io_error(), "unknown backend") != NULL);
}

// [FAILURE-EXPLICIT] Preserve every former death-test input and reason as a
// handled status; an Error never masquerades as a device success or host retry.
static void fails(int32_t status, const char *expected) {
  assert(status == -1);
  if (!osp_io_error() || !strstr(osp_io_error(), expected))
    fprintf(stderr, "expected failure %s; actual %s\n", expected, osp_io_error() ? osp_io_error() : "missing");
  assert(osp_io_error() && strstr(osp_io_error(), expected) != NULL);
  osp_io_error_clear();
  device_error = 0;
#if defined(OSPREY_GPU_METAL) || defined(OSPREY_GPU_OPENCL)
  injected_fault = 0;
#endif
}

static int32_t declined_on_device(void) {
  select_mode("auto");
  return osprey_gpu_declined("gpuScan over a kernel the device cannot run");
}
#if defined(OSPREY_GPU_METAL)
static int32_t faulting_launch(void) {
  select_mode("auto");
  injected_fault = OSPREY_GPU_FAULT_OVERFLOW;
  return launch(osprey_gpu_alloc(1, 8), "", NULL);
}
static int32_t too_many_uniforms(void) {
  select_mode("auto");
  int64_t uniforms[OSPREY_GPU_MAX_ARGS] = {0};
  char kinds[OSPREY_GPU_MAX_ARGS + 1];
  memset(kinds, 's', OSPREY_GPU_MAX_ARGS);
  kinds[OSPREY_GPU_MAX_ARGS] = '\0';
  return launch(osprey_gpu_alloc(1, 8), kinds, uniforms);
}
static int32_t device_failure_on_launch(void) {
  select_mode("auto");
  device_error = 1;
  return launch(osprey_gpu_alloc(1, 8), "", NULL);
}

// [FAILURE-EXPLICIT] [GPU-DEVICE-FAULT] A recorded device fault, a refused
// decline and a dispatch failure are failures the caller handles: -1 with the
// reason on the io_error channel, never an abort. The same launch works again
// once the device recovers, so a failure leaves nothing to retry behind.
static void handled_launch_failure(void) {
  select_mode("auto");
  void *out = osprey_gpu_alloc(1, 8);
  injected_fault = OSPREY_GPU_FAULT_DIVIDE_BY_ZERO;
  osp_io_error_clear();
  assert(launch(out, "", NULL) == -1);
  assert(strstr(osp_io_error(), "kernel: osprey_kernel_1") != NULL);
  assert(strstr(osp_io_error(), "division by zero") != NULL);
  injected_fault = 0;
  osp_io_error_clear();
  assert(launch(out, "", NULL) == 1 && !osp_io_error());
  int64_t uniforms[OSPREY_GPU_MAX_ARGS] = {0};
  char kinds[OSPREY_GPU_MAX_ARGS + 1];
  memset(kinds, 's', OSPREY_GPU_MAX_ARGS);
  kinds[OSPREY_GPU_MAX_ARGS] = '\0';
  osp_io_error_clear();
  assert(launch(osprey_gpu_alloc(1, 8), kinds, uniforms) == -1);
  assert(strstr(osp_io_error(), "too many kernel arguments") != NULL);
  device_error = 1;
  osp_io_error_clear();
  assert(launch(osprey_gpu_alloc(1, 8), "", NULL) == -1 && osp_io_error());
  device_error = 0;
  osp_io_error_clear();
  assert(launch(osprey_gpu_alloc(1, 8), "", NULL) == 1 && !osp_io_error());
  osp_io_error_clear();
  assert(osprey_gpu_declined("gpuScan over a kernel the device cannot run") == -1);
  assert(strstr(osp_io_error(), "cannot run it") != NULL);
}

// [FAILURE-EXPLICIT] A readback that fails records its reason and yields no
// words: no accessor aborts, a host write is refused rather than landing in
// half-current data, and the retry after recovery returns the device's result.
static void handled_readback_failure(int64_t mark) {
  select_mode("auto");
  void *out = osprey_gpu_alloc(1, 8);
  int64_t *host_words = osprey_gpu_data(out);
  assert(host_words && *host_words == 0);
  assert(launch(out, "", NULL) == 1);
  device_error = 1;
  osp_io_error_clear();
  assert(osprey_gpu_data(out) == NULL);
  assert(strcmp(osp_io_error(), "mock: readback: injected download failure") == 0);
  assert(osprey_gpu_get(out, 0) == 0 && osp_io_error() != NULL);
  osprey_gpu_set(out, 0, 99);
  assert(*host_words == 0 && osprey_gpu_len(out) == 1);
  assert(strcmp(osp_io_error(), "mock: readback: injected download failure") == 0);
  device_error = 0;
  osp_io_error_clear();
  assert(osprey_gpu_get(out, 0) == mark && !osp_io_error());
}
#endif

int main(void) {
  host_api_workflow();
  host_case(NULL); host_case(""); host_case("host");
  assert(osprey_gpu_buffer_tests(0, NULL) == 0);
#if defined(OSPREY_GPU_METAL)
  device_case("auto", 1, "metal:mock"); device_case("metal", 1, "metal:mock");
  handled_launch_failure();
  handled_readback_failure(42);
  fails(faulting_launch(), "kernel: osprey_kernel_1: integer overflow");
  fails(declined_on_device(), "gpuScan over a kernel the device cannot run; a selected device cannot run it");
  fails(too_many_uniforms(), "too many kernel arguments");
  fails(device_failure_on_launch(), "kernel: osprey_kernel_1:");
#else
#if defined(OSPREY_GPU_OPENCL)
  device_case("auto", 1, "opencl:mock");
#else
  device_case("auto", 0, "unavailable:device");
#endif
  device_case("metal", 0, "unavailable:device");
  fails(declined_on_device(), "a selected device cannot run it");
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
  puts("[ok] GPU selection: host, auto, Metal, OpenCL, missing adapter, compiled-kernel launch, resident buffers uploaded once and read back on demand, handled faults, declines and readback failures with their reasons, trace switch and no silent CPU fallback");
  return 0;
}
