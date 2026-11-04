// Driver lifecycle/error tests without GPU hardware. Hardware execution and
// numerical parity are independently gated by gpu_device_tests.c.
#include "gpu_opencl_api.h"
#include "io_error.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

#define MOCK_MARK 42
// Mirrors gpu_opencl.c's kernel-object table cap.
#define CL_MAX_NAMES 512
static unsigned calls, fail_at, live, next_handle, programs, kernels;
static size_t expected_threads = 1;
static size_t dense_bytes;
static int status(void) { return ++calls == fail_at ? -7 : 0; }
static void *handle(int *error) {
  *error = status();
  if (*error) return NULL;
  live++;
  return (void *)(uintptr_t)++next_handle;
}
static int OSP_CL_CALL platforms(unsigned count, void **out, unsigned *n) {
  if (n) *n = 1;
  if (count && out) *out = (void *)(uintptr_t)42;
  return status();
}
static int OSP_CL_CALL devices(void *platform, uint64_t type, unsigned count, void **out, unsigned *n) {
  assert(platform && type == 4 && count == 1 && !n);
  int error = status();
  if (!error) *out = (void *)(uintptr_t)43;
  return error;
}
static int OSP_CL_CALL info(void *device, unsigned key, size_t size, void *out, size_t *n) {
  assert(device && key == 0x102B && size >= 5 && !n);
  memcpy(out, "mock", 5);
  return status();
}
static void *OSP_CL_CALL context(const intptr_t *p, unsigned count, void *const *d, void *notify, void *user, int *error) {
  assert(!p && count == 1 && *d && !notify && !user);
  return handle(error);
}
static void *OSP_CL_CALL queue(void *ctx, void *device, uint64_t properties, int *error) {
  assert(ctx && device && !properties);
  return handle(error);
}
static void *OSP_CL_CALL program(void *ctx, unsigned count, const char **source, const size_t *size, int *error) {
  assert(ctx && count == 1 && *size && strstr(*source, "osprey_matmul64") && (*source)[*size] == '\0');
  // [GPU-DEVICE-NATIVE]: an OpenCL build must preserve separate multiply/add rounding.
  assert(strstr(*source, "#pragma OPENCL FP_CONTRACT OFF"));
  programs++;
  return handle(error);
}
static int OSP_CL_CALL build(void *p, unsigned count, void *const *device, const char *options, void *notify, void *user) {
  assert(p && count == 1 && *device && strstr(options, "CL1.2") && !notify && !user);
  return status();
}
static int OSP_CL_CALL build_info(void *p, void *d, unsigned key, size_t size, void *out, size_t *n) {
  assert(p && d && key == 0x1183 && size > 5 && !n);
  memcpy(out, "build", 6);
  return 0;
}
static void *OSP_CL_CALL kernel(void *p, const char *name, int *error) {
  assert(p && name[0]);
  kernels++;
  return handle(error);
}
static void *OSP_CL_CALL buffer(void *ctx, uint64_t flags, size_t bytes, const void *source, int *error) {
  assert(ctx && (dense_bytes ? bytes == dense_bytes : bytes >= 8) && (flags == 36 || flags == 33) && source != NULL);
  return handle(error);
}
static int OSP_CL_CALL arg(void *k, unsigned i, size_t bytes, const void *value) {
  assert(k && i < OSPREY_GPU_MAX_ARGS && bytes == 8 && value);
  return status();
}
static int OSP_CL_CALL launch(void *q, void *k, unsigned rank, const size_t *offset, const size_t *global,
                            const size_t *local, unsigned count, const void *events, void *event) {
  assert(q && k && rank == 1 && !offset && *global == expected_threads && !local && !count && !events && !event);
  return status();
}
static int OSP_CL_CALL read_buffer(void *q, void *b, unsigned blocking, size_t offset, size_t size,
                                  void *out, unsigned count, const void *events, void *event) {
  assert(q && b && blocking == 1 && !offset && (dense_bytes ? size == dense_bytes : size >= 8) && !count && !events && !event);
  if (dense_bytes) memset(out, MOCK_MARK, size);
  else for (size_t i = 0; i < size / 8; i++) ((int64_t *)out)[i] = MOCK_MARK;
  return status();
}
static int OSP_CL_CALL release(void *object) { assert(object && live); live--; return 0; }

static const OspreyOpenCLApi mock = {platforms, devices, info, context, queue, program, build,
    build_info, kernel, buffer, arg, launch, read_buffer, release, release, release, release, release};

static OspreyGpuDispatch product_of(const int64_t *in, int64_t *out, void **in_slot, void **out_slot) {
  OspreyGpuDispatch d = {"osprey_matmul64", NULL, {{0, NULL, NULL, 0, NULL, 8, NULL, 0, 0}}, 0, 1};
  osprey_gpu_arg_at(&d, OSPREY_GPU_INPUT, in, NULL, 1, 8, in_slot);
  osprey_gpu_arg_at(&d, OSPREY_GPU_INPUT, in, NULL, 1, 8, in_slot);
  osprey_gpu_arg_at(&d, OSPREY_GPU_OUTPUT, out, out_slot ? NULL : out, 1, 8, out_slot);
  for (uint64_t i = 0; i < 3; i++) osprey_gpu_arg(&d, OSPREY_GPU_SCALAR, NULL, NULL, 1, 8);
  expected_threads = 1;
  return d;
}

static int product(int64_t *out) {
  int64_t in = 0;
  OspreyGpuDispatch d = product_of(&in, out, NULL, NULL);
  return osprey_gpu_opencl_dispatch(&d);
}

static int compiled(const char *name, const char *source, int64_t *out, size_t threads) {
  int64_t in = 7, fault = 0;
  OspreyGpuDispatch d = {name, source, {{0, NULL, NULL, 0, NULL, 8, NULL, 0, 0}}, 0, threads};
  osprey_gpu_arg(&d, OSPREY_GPU_INPUT, &in, NULL, 1, 8);
  osprey_gpu_arg(&d, OSPREY_GPU_INPUT, &in, NULL, 0, 8);
  osprey_gpu_arg(&d, OSPREY_GPU_OUTPUT, out, out, 2, 8);
  osprey_gpu_arg(&d, OSPREY_GPU_OUTPUT, &fault, &fault, 1, 8);
  osprey_gpu_arg(&d, OSPREY_GPU_SCALAR, NULL, NULL, threads, 8);
  expected_threads = threads;
  int result = osprey_gpu_opencl_dispatch(&d);
  assert(result != 1 || fault == MOCK_MARK);
  return result;
}

static void failure_case(unsigned point) {
  osprey_gpu_opencl_test_driver(&mock);
  assert(live == 0);
  calls = 0; fail_at = point;
  int64_t out = 99;
  osp_io_error_clear();
  int result = product(&out);
  assert(point ? result == -1 : result == 1);
  assert(point ? osp_io_error() != NULL : (out == MOCK_MARK && osprey_gpu_opencl_completed() == 1));
  assert(live <= 4); // Failed initialization releases everything; launches retain the context, program and kernels.
}

// A program's kernels recompile the program once, reuse their kernel objects,
// and the table of kernel objects refuses to grow without bound.
static void compiled_case(void) {
  static const char source[] = "kernel void osprey_kernel_1() {}";
  static char names[CL_MAX_NAMES][12];
  osprey_gpu_opencl_test_driver(&mock);
  calls = 0; fail_at = 0; programs = 0; kernels = 0;
  int64_t out[2] = {99, 99};
  assert(product(out) == 1 && programs == 1 && kernels == 1);
  assert(compiled("osprey_kernel_1", source, out, 3) == 1 && programs == 2 && kernels == 2 && out[1] == MOCK_MARK);
  assert(compiled("osprey_kernel_1", source, out, 5) == 1 && programs == 2 && kernels == 2);
  assert(product(out) == 1 && programs == 2 && kernels == 3 && osprey_gpu_opencl_completed() == 4);
  for (unsigned i = 0; i < CL_MAX_NAMES; i++) {
    (void)snprintf(names[i], sizeof names[i], "k%u", i);
    int result = compiled(names[i], source, out, 1);
    assert(i < CL_MAX_NAMES - 2 ? result == 1 : result == -1);
  }
  assert(strstr(osp_io_error(), "kernel table") != NULL);
}

// A resident buffer is created once however many dispatches bind it (twice
// in one dispatch binds one object), an output left on the device is read on
// demand, and its release is the only one [GPU-BUFFER-RESIDENT].
static void resident_case(void) {
  int64_t in = 5, out = 99;
  void *in_slot = NULL, *out_slot = NULL;
  unsigned held = 0;
  osprey_gpu_opencl_test_driver(&mock);
  calls = 0; fail_at = 0;
  for (unsigned round = 0; round < 2; round++) {
    OspreyGpuDispatch d = product_of(&in, &out, &in_slot, &out_slot);
    assert(osprey_gpu_opencl_dispatch(&d) == 1 && in_slot && out_slot && out == 99);
    if (!round) held = live;
  }
  assert(live == held); // the second dispatch created and released nothing
  assert(osprey_gpu_opencl_download(out_slot, &out, sizeof out) == 1 && out == MOCK_MARK);
  osprey_gpu_opencl_release(in_slot);
  osprey_gpu_opencl_release(out_slot);
  assert(live == held - 2);
}

// [GPU-BUFFER-ELEM]: exact transfers, including one-element empty allocations.
static void dense_transfers(void) {
  osprey_gpu_opencl_test_driver(&mock);
  calls = 0; fail_at = 0; expected_threads = 1;
  for (uint64_t width = 1; width <= 8; width *= 2) {
    for (uint64_t count = 0; count <= 3; count += 3) {
      for (unsigned resident = 0; resident <= 1; resident++) {
        unsigned char input[24] = {0}, output[26];
        void *slot = NULL;
        memset(output, 0xa5, sizeof output);
        dense_bytes = (size_t)((count ? count : 1) * width);
        OspreyGpuDispatch d = {"dense_transfer", "kernel void dense_transfer() {}", {{0}}, 0, 1};
        osprey_gpu_arg(&d, OSPREY_GPU_INPUT, input, NULL, count, width);
        osprey_gpu_arg_at(&d, OSPREY_GPU_OUTPUT, output + 1, resident ? NULL : output + 1,
                          count, width, resident ? &slot : NULL);
        assert(osprey_gpu_opencl_dispatch(&d) == 1);
        if (resident) {
          for (size_t i = 0; i < sizeof output; i++) assert(output[i] == 0xa5);
          if (count) assert(osprey_gpu_opencl_download(slot, output + 1, count * width) == 1);
          osprey_gpu_opencl_release(slot);
        }
        for (size_t i = 0; i < sizeof output; i++)
          assert(output[i] == (i > 0 && i <= count * width ? MOCK_MARK : 0xa5));
      }
    }
  }
  dense_bytes = 0;
}

int main(void) {
  // Exercise the real dynamic loader even on a machine without a compute GPU.
  assert(osprey_gpu_opencl_name() != NULL);
  for (unsigned point = 0; point <= 20; point++) failure_case(point);
  compiled_case();
  resident_case();
  dense_transfers();
  osprey_gpu_opencl_test_driver(&mock);
  assert(live == 0);
  puts("[ok] OpenCL driver: product and compiled-kernel dispatch, program recompilation, kernel reuse, resident buffers created once and read back on demand, every initialization/transfer/launch/readback failure releases resources");
  return 0;
}
