// Portable direct-driver adapter [GPU-BACKEND-PORTABILITY] [GPU-BACKEND-DEVICE]:
// one program holding the shader source plus the running program's compiled
// kernels, one kernel object per name, and one dispatch path for the builtin
// product and every compiled kernel alike.
#include "gpu_device.h"
#include "gpu_opencl_api.h"
#include "io_error.h"
#include "../bin/gpu_shader_source.h"
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#if defined(_WIN32)
#include <windows.h>
#else
#include <dlfcn.h>
#endif

#define CL_MAX_KERNELS 512
// cl_mem_flags: READ_ONLY | COPY_HOST_PTR for inputs, READ_WRITE | COPY_HOST_PTR
// for outputs, which a kernel may read (a reduction's seed) before it writes.
#define CL_INPUT_FLAGS UINT64_C(36)
#define CL_OUTPUT_FLAGS UINT64_C(33)
#define CL_DEVICE_TYPE_GPU UINT64_C(4)
#define CL_DEVICE_NAME 0x102B
#define CL_PROGRAM_BUILD_LOG 0x1183

static OspreyOpenCLApi cl;
static void *cl_device, *cl_context, *cl_queue, *cl_program;
static struct { const char *name; void *kernel; } cl_kernels[CL_MAX_KERNELS];
static unsigned cl_kernel_count;
static const char *cl_source;
static pthread_mutex_t cl_lock = PTHREAD_MUTEX_INITIALIZER;
static uint64_t cl_completed;
static int cl_ready;
static char cl_name[256];
static char cl_failure[1024];

#define CL_FUNCTIONS(X) \
  X(platforms, "clGetPlatformIDs") X(devices, "clGetDeviceIDs") \
  X(info, "clGetDeviceInfo") X(context, "clCreateContext") \
  X(queue, "clCreateCommandQueue") X(program, "clCreateProgramWithSource") \
  X(build, "clBuildProgram") X(build_info, "clGetProgramBuildInfo") \
  X(kernel, "clCreateKernel") X(buffer, "clCreateBuffer") \
  X(arg, "clSetKernelArg") X(launch, "clEnqueueNDRangeKernel") \
  X(read, "clEnqueueReadBuffer") X(release_buffer, "clReleaseMemObject") \
  X(release_kernel, "clReleaseKernel") X(release_program, "clReleaseProgram") \
  X(release_queue, "clReleaseCommandQueue") X(release_context, "clReleaseContext")

#if defined(OSPREY_GPU_TESTING)
static const OspreyOpenCLApi *test_driver;
#endif

static int failed(const char *operation, int status) {
  (void)snprintf(cl_failure, sizeof cl_failure, "%s failed (OpenCL status %d)", operation, status);
  return -1;
}

static void *load_driver(void) {
#if defined(OSPREY_GPU_TESTING)
  if (test_driver) return (void *)(uintptr_t)1;
#endif
#if defined(_WIN32)
  return (void *)LoadLibraryA("OpenCL.dll");
#elif defined(__APPLE__)
  return dlopen("/System/Library/Frameworks/OpenCL.framework/OpenCL", RTLD_NOW | RTLD_LOCAL);
#else
  return dlopen("libOpenCL.so.1", RTLD_NOW | RTLD_LOCAL);
#endif
}

#if defined(OSPREY_GPU_TESTING)
static int test_symbol(const char *name, void *slot, size_t size) {
#define MATCH(field, text) if (strcmp(name, text) == 0 && test_driver->field && size == sizeof test_driver->field) { \
    memcpy(slot, &test_driver->field, size); return 0; }
  CL_FUNCTIONS(MATCH)
#undef MATCH
  return failed(name, -1);
}
#endif

static int symbol(void *library, const char *name, void *slot, size_t size) {
#if defined(OSPREY_GPU_TESTING)
  if (test_driver) return test_symbol(name, slot, size);
#endif
#if defined(_WIN32)
  FARPROC address = GetProcAddress((HMODULE)library, name);
#else
  void *address = dlsym(library, name);
#endif
  if (!address || size != sizeof address) return failed(name, -1);
  memcpy(slot, &address, size);
  return 0;
}

#define LOAD(field, name) if (symbol(library, name, &cl.field, sizeof cl.field)) return -1;
static int load_api(void) {
  void *library = load_driver();
  if (!library) return failed("OpenCL driver unavailable", -1);
  CL_FUNCTIONS(LOAD)
  return 0;
}
#undef LOAD

static void release_program(void) {
  for (unsigned i = 0; i < cl_kernel_count; i++) (void)cl.release_kernel(cl_kernels[i].kernel);
  cl_kernel_count = 0;
  if (cl_program) (void)cl.release_program(cl_program);
  cl_program = NULL;
}

static void release_context(void) {
  release_program();
  if (cl_queue) (void)cl.release_queue(cl_queue);
  if (cl_context) (void)cl.release_context(cl_context);
  cl_queue = cl_context = NULL;
}

static int find_device(void) {
  unsigned count = 0;
  int status = cl.platforms(0, NULL, &count);
  if (status || !count) return failed("GPU platform discovery", status);
  void **platforms = calloc(count, sizeof *platforms);
  if (!platforms) return failed("platform allocation", -1);
  status = cl.platforms(count, platforms, NULL);
  for (unsigned i = 0; !status && i < count; i++) {
    if (cl.devices(platforms[i], CL_DEVICE_TYPE_GPU, 1, &cl_device, NULL) == 0 && cl_device) break;
  }
  free(platforms);
  if (status || !cl_device) return failed("compute GPU discovery", status);
  char name[192] = {0};
  status = cl.info(cl_device, CL_DEVICE_NAME, sizeof name, name, NULL);
  if (status) return failed("device name", status);
  (void)snprintf(cl_name, sizeof cl_name, "opencl:0 (%s)", name);
  return 0;
}

static int create_context(void) {
  int status = 0;
  cl_context = cl.context(NULL, 1, &cl_device, NULL, NULL, &status);
  if (status || !cl_context) return failed("context creation", status);
  cl_queue = cl.queue(cl_context, cl_device, 0, &status);
  return status || !cl_queue ? failed("queue creation", status) : 0;
}

static void build_failure(int status) {
  (void)failed("kernel compilation", status);
  char log[900] = {0};
  if (cl.build_info(cl_program, cl_device, CL_PROGRAM_BUILD_LOG, sizeof log, log, NULL) == 0 && log[0])
    (void)snprintf(cl_failure, sizeof cl_failure, "kernel compilation (status %d): %.899s", status, log);
}

// The shader source followed by the program's kernels, as one text.
static char *program_text(const char *source, size_t *size) {
  size_t extra = source ? strlen(source) : 0;
  char *text = malloc((size_t)osprey_gpu_shader_len + extra + 2);
  if (!text) return NULL;
  memcpy(text, osprey_gpu_shader, osprey_gpu_shader_len);
  text[osprey_gpu_shader_len] = '\n';
  memcpy(text + osprey_gpu_shader_len + 1, source ? source : "", extra);
  *size = (size_t)osprey_gpu_shader_len + 1 + extra;
  text[*size] = '\0';
  return text;
}

// Compile the program when there is none, or when a dispatch brings kernels
// the current one lacks; the kernel objects of the old program are released.
static int ensure_program(const char *source) {
  if (cl_program && (!source || source == cl_source)) return 0;
  release_program();
  size_t size = 0;
  char *text = program_text(source, &size);
  if (!text) return failed("program text allocation", -1);
  int status = 0;
  const char *lines = text;
  cl_program = cl.program(cl_context, 1, &lines, &size, &status);
  free(text);
  if (status || !cl_program) return failed("program creation", status);
  status = cl.build(cl_program, 1, &cl_device, "-cl-std=CL1.2", NULL, NULL);
  if (status) {
    build_failure(status);
    return -1;
  }
  cl_source = source;
  return 0;
}

static void *kernel_of(const char *name) {
  for (unsigned i = 0; i < cl_kernel_count; i++) {
    if (strcmp(cl_kernels[i].name, name) == 0) return cl_kernels[i].kernel;
  }
  if (cl_kernel_count == CL_MAX_KERNELS) {
    (void)failed("kernel table", -1);
    return NULL;
  }
  int status = 0;
  void *kernel = cl.kernel(cl_program, name, &status);
  if (status || !kernel) {
    (void)failed("kernel creation", status);
    return NULL;
  }
  cl_kernels[cl_kernel_count].name = name;
  cl_kernels[cl_kernel_count++].kernel = kernel;
  return kernel;
}

static int initialize(void) {
  if (cl_ready) return cl_ready;
  if ((!cl.platforms && load_api()) || find_device() || create_context() || ensure_program(NULL)) {
    release_context();
    cl_ready = -1;
  } else {
    cl_ready = 1;
  }
  return cl_ready;
}

const char *osprey_gpu_opencl_name(void) {
  (void)pthread_mutex_lock(&cl_lock);
  int ready = initialize();
  (void)pthread_mutex_unlock(&cl_lock);
  return ready > 0 ? cl_name : "opencl:unavailable";
}

uint64_t osprey_gpu_opencl_completed(void) {
  (void)pthread_mutex_lock(&cl_lock);
  uint64_t count = cl_completed;
  (void)pthread_mutex_unlock(&cl_lock);
  return count;
}

// The device object a buffer argument is bound to [GPU-BUFFER-RESIDENT]: the
// one its slot holds, else a new one created from its bytes — `elem`-byte
// elements [GPU-BUFFER-ELEM] — kept in the slot of a resident argument and
// released after this dispatch otherwise.
static void *object_of(const OspreyGpuArg *arg, int *status) {
  if (arg->slot && *arg->slot) return *arg->slot;
  uint64_t flags = arg->kind == OSPREY_GPU_INPUT ? CL_INPUT_FLAGS : CL_OUTPUT_FLAGS;
  void *object = cl.buffer(cl_context, flags, (size_t)osprey_gpu_arg_bytes(arg), arg->bytes, status);
  if (object && !*status && arg->slot) *arg->slot = object;
  return object;
}

// Bind every argument; the buffers are kept for the read-back.
static int bind_arguments(void *kernel, const OspreyGpuDispatch *d, void **buffers) {
  for (unsigned i = 0; i < d->count; i++) {
    const OspreyGpuArg *arg = &d->args[i];
    int status = 0;
    if (arg->kind == OSPREY_GPU_SCALAR) {
      status = cl.arg(kernel, i, sizeof arg->count, &arg->count);
      if (status) return failed("kernel argument", status);
      continue;
    }
    buffers[i] = object_of(arg, &status);
    if (status || !buffers[i]) return failed("GPU buffer allocation", status);
    status = cl.arg(kernel, i, sizeof buffers[i], &buffers[i]);
    if (status) return failed("kernel argument", status);
  }
  return 0;
}

// Read back every output the host asked for; the rest stay on the device.
static int read_back(const OspreyGpuDispatch *d, void **buffers) {
  for (unsigned i = 0; i < d->count; i++) {
    const OspreyGpuArg *arg = &d->args[i];
    if (arg->kind != OSPREY_GPU_OUTPUT || !arg->count || !arg->out) continue;
    int status = cl.read(cl_queue, buffers[i], 1, 0, (size_t)osprey_gpu_arg_bytes(arg), arg->out, 0, NULL, NULL);
    if (status) return failed("GPU completion/readback", status);
  }
  return 0;
}

static int dispatch(void *kernel, const OspreyGpuDispatch *d) {
  void *buffers[OSPREY_GPU_MAX_ARGS] = {NULL};
  size_t count = (size_t)d->threads;
  int status = bind_arguments(kernel, d, buffers);
  if (!status) {
    status = cl.launch(cl_queue, kernel, 1, NULL, &count, NULL, 0, NULL, NULL);
    if (status) status = failed("GPU dispatch", status);
  }
  if (!status) status = read_back(d, buffers);
  for (unsigned i = 0; i < d->count; i++) if (buffers[i] && !d->args[i].slot) (void)cl.release_buffer(buffers[i]);
  if (status) return -1;
  cl_completed++;
  osprey_gpu_report(cl_name, cl_completed, d);
  return 1;
}

static int dispatch_locked(const OspreyGpuDispatch *d) {
  if (initialize() < 0 || ensure_program(d->source)) return -1;
  void *kernel = kernel_of(d->kernel);
  return kernel ? dispatch(kernel, d) : -1;
}

int osprey_gpu_opencl_download(void *object, void *out, uint64_t bytes) {
  (void)pthread_mutex_lock(&cl_lock);
  int status = cl.read ? cl.read(cl_queue, object, 1, 0, (size_t)bytes, out, 0, NULL, NULL) : -1;
  if (status) {
    (void)failed("GPU readback", status);
    osp_io_error_text("gpu", cl_name[0] ? cl_name : "OpenCL", cl_failure);
  }
  (void)pthread_mutex_unlock(&cl_lock);
  return status ? -1 : 1;
}

void osprey_gpu_opencl_release(void *object) {
  (void)pthread_mutex_lock(&cl_lock);
  if (cl.release_buffer) (void)cl.release_buffer(object);
  (void)pthread_mutex_unlock(&cl_lock);
}

int osprey_gpu_opencl_dispatch(const OspreyGpuDispatch *d) {
  (void)pthread_mutex_lock(&cl_lock);
  int status = dispatch_locked(d);
  if (status < 0) osp_io_error_text(d->kernel, cl_name[0] ? cl_name : "OpenCL", cl_failure);
  (void)pthread_mutex_unlock(&cl_lock);
  return status;
}

#if defined(OSPREY_GPU_TESTING)
void osprey_gpu_opencl_test_driver(const OspreyOpenCLApi *api) {
  if (cl.release_kernel) release_context();
  test_driver = api;
  memset(&cl, 0, sizeof cl);
  cl_device = NULL;
  cl_source = NULL;
  cl_ready = 0;
  cl_completed = 0;
}
#endif
