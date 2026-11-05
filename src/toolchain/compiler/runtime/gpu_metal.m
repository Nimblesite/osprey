// Direct Metal compute adapter [GPU-BACKEND-PORTABILITY] [GPU-BACKEND-DEVICE]:
// one library holding the shader source plus the running program's compiled
// kernels, one pipeline per kernel name, and one dispatch path for the builtin
// product and every compiled kernel alike.
#import <Foundation/Foundation.h>
#import <Metal/Metal.h>
#include "gpu_device.h"
#include "io_error.h"
#include "../bin/gpu_shader_source.h"
#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static id<MTLDevice> gpu_device;
static id<MTLCommandQueue> gpu_queue;
static id<MTLLibrary> gpu_library;
static NSMutableDictionary<NSString *, id<MTLComputePipelineState>> *gpu_pipelines;
// The program source the library was compiled with, compared by identity: a
// launch site always passes the same constant.
static const char *gpu_source;
static pthread_once_t gpu_once = PTHREAD_ONCE_INIT;
static pthread_mutex_t gpu_lock = PTHREAD_MUTEX_INITIALIZER;
static _Atomic uint64_t gpu_completed = 0;
static char gpu_failure[512];
static char gpu_name[256];

static void init_device(void) {
  @autoreleasepool {
    gpu_device = MTLCreateSystemDefaultDevice();
    gpu_queue = [gpu_device newCommandQueue];
    if (!gpu_device || !gpu_queue) {
      (void)snprintf(gpu_failure, sizeof gpu_failure, "no Metal compute device or command queue");
      return;
    }
    (void)snprintf(gpu_name, sizeof gpu_name, "metal:0 (%s)", gpu_device.name.UTF8String);
    gpu_pipelines = [NSMutableDictionary new];
  }
}

// The math options that replaced `fastMathEnabled`, which fails the build once
// the deployment target reaches the release that deprecated it.
static void use_safe_math(MTLCompileOptions *options) API_AVAILABLE(macos(15.0), ios(18.0)) {
  options.mathMode = MTLMathModeSafe;
  options.mathFloatingPointFunctions = MTLMathFloatingPointFunctionsPrecise;
}

// Compile the library when there is none, or when a dispatch brings program
// kernels the current one lacks; the pipelines of the old library are dropped.
static bool ensure_library(const char *source) {
  if (gpu_queue == nil) return false;
  if (gpu_library != nil && (!source || source == gpu_source)) return true;
  NSString *text = [NSString stringWithFormat:@"%s\n%s", osprey_gpu_shader, source ? source : ""];
  NSError *error = nil;
  // [GPU-DEVICE-NATIVE]: preserve IEEE arithmetic independently of driver defaults.
  MTLCompileOptions *options = [MTLCompileOptions new];
#if (defined(__MAC_OS_X_VERSION_MIN_REQUIRED) && __MAC_OS_X_VERSION_MIN_REQUIRED >= 150000) || \
    (defined(__IPHONE_OS_VERSION_MIN_REQUIRED) && __IPHONE_OS_VERSION_MIN_REQUIRED >= 180000)
  use_safe_math(options);
#else
  if (@available(macOS 15.0, iOS 18.0, *)) {
    use_safe_math(options);
  } else {
    options.fastMathEnabled = NO;
  }
#endif
  gpu_library = [gpu_device newLibraryWithSource:text options:options error:&error];
  [gpu_pipelines removeAllObjects];
  gpu_source = source;
  if (gpu_library == nil)
    (void)snprintf(gpu_failure, sizeof gpu_failure, "%s",
                   error ? error.localizedDescription.UTF8String : "Metal kernel compilation failed");
  return gpu_library != nil;
}

static id<MTLComputePipelineState> pipeline(const char *kernel) {
  NSString *name = @(kernel);
  id<MTLComputePipelineState> state = gpu_pipelines[name];
  if (state != nil) return state;
  NSError *error = nil;
  id<MTLFunction> function = [gpu_library newFunctionWithName:name];
  if (function != nil) state = [gpu_device newComputePipelineStateWithFunction:function error:&error];
  if (state == nil) {
    (void)snprintf(gpu_failure, sizeof gpu_failure, "%s: %s", kernel,
                   error ? error.localizedDescription.UTF8String : "no such kernel in the compiled library");
    return nil;
  }
  gpu_pipelines[name] = state;
  return state;
}

const char *osprey_gpu_metal_name(void) {
  (void)pthread_once(&gpu_once, init_device);
  (void)pthread_mutex_lock(&gpu_lock);
  bool ready;
  @autoreleasepool { ready = ensure_library(NULL); }
  (void)pthread_mutex_unlock(&gpu_lock);
  return ready ? gpu_name : "metal:unavailable";
}

uint64_t osprey_gpu_metal_completed(void) { return atomic_load(&gpu_completed); }

static bool within_limits(const OspreyGpuDispatch *d) {
  for (unsigned i = 0; i < d->count; i++) {
    if (d->args[i].kind != OSPREY_GPU_SCALAR && MAX(osprey_gpu_arg_bytes(&d->args[i]), d->args[i].map_bytes) > gpu_device.maxBufferLength)
      return false;
  }
  return d->threads <= UINT32_MAX;
}

// The device object a buffer argument is bound to [GPU-BUFFER-RESIDENT]: the
// one its slot holds, else a new one created from its words, kept in the slot
// of a resident argument and alive only for this dispatch otherwise.
static id<MTLBuffer> object_of(const OspreyGpuArg *arg) {
  if (arg->slot && *arg->slot) return (__bridge id<MTLBuffer>)*arg->slot;
  id<MTLBuffer> buffer =
      arg->map_base ? [gpu_device newBufferWithBytesNoCopy:(void *)(uintptr_t)arg->map_base length:(NSUInteger)arg->map_bytes
                                                   options:MTLResourceStorageModeShared deallocator:nil]
                    : [gpu_device newBufferWithBytes:arg->words length:(NSUInteger)osprey_gpu_arg_bytes(arg)
                                             options:MTLResourceStorageModeShared];
  if (buffer != nil && arg->slot) {
    CFTypeRef retained = CFBridgingRetain(buffer);
    *arg->slot = (void *)(uintptr_t)retained;
  }
  return buffer;
}

// Bind every argument at its index: scalars by value, buffers by their device
// object, kept for the write-back.
static bool bind_arguments(id<MTLComputeCommandEncoder> encoder, const OspreyGpuDispatch *d,
                           id<MTLBuffer> __strong *buffers) {
  for (unsigned i = 0; i < d->count; i++) {
    const OspreyGpuArg *arg = &d->args[i];
    if (arg->kind == OSPREY_GPU_SCALAR) {
      [encoder setBytes:&arg->count length:sizeof arg->count atIndex:i];
      continue;
    }
    buffers[i] = object_of(arg);
    if (buffers[i] == nil) {
      osp_io_error_text(d->kernel, "Metal", "buffer allocation failed");
      return false;
    }
    [encoder setBuffer:buffers[i] offset:(NSUInteger)arg->map_skip atIndex:i];
  }
  return true;
}

// Copy back every output the host asked for; the rest stay on the device.
static void write_back(const OspreyGpuDispatch *d, id<MTLBuffer> __strong *buffers) {
  for (unsigned i = 0; i < d->count; i++) {
    if (d->args[i].kind == OSPREY_GPU_OUTPUT && d->args[i].out)
      memcpy(d->args[i].out, buffers[i].contents, (size_t)(d->args[i].count * d->args[i].elem_bytes));
  }
}

static int flush_locked(void);

// The pending batch is submitted and waited for under the lock, so a
// shared-storage buffer's contents are the device's final words by the time
// they are read. The caller is plain C with no autorelease pool, so the
// bridge gets its own.
int osprey_gpu_metal_download(void *object, void *out, uint64_t count) {
  (void)pthread_mutex_lock(&gpu_lock);
  int status;
  @autoreleasepool {
    status = flush_locked();
    if (status == 1) memcpy(out, ((__bridge id<MTLBuffer>)object).contents, (size_t)count);
  }
  (void)pthread_mutex_unlock(&gpu_lock);
  return status;
}

// Hand the retained object back to ARC, which releases it at once.
void osprey_gpu_metal_release(void *object) {
  @autoreleasepool {
    (void)CFBridgingRelease(object);
  }
}

// [LLM-DISPATCH-BATCH] Dispatches are encoded, in order, into one open command
// buffer and submitted together the first time the host needs a result: a
// write-back, a download, or the batch reaching its cap. A serial compute
// encoder orders them, so a kernel reads what the one before it wrote, and
// the command buffer retains every object it binds, so a transient or
// released buffer outlives the dispatch that reads it. The kernel names are
// kept for the completion report and for naming a failed batch.
#define GPU_BATCH_CAP 1024
typedef struct { char kernel[48]; uint64_t threads; } OspreyGpuQueued;
static id<MTLCommandBuffer> gpu_batch;
static id<MTLComputeCommandEncoder> gpu_encoder;
static OspreyGpuQueued gpu_queued[GPU_BATCH_CAP];
static unsigned gpu_queued_count;

static int flush_locked(void) {
  if (gpu_batch == nil) return 1;
  id<MTLCommandBuffer> command = gpu_batch;
  [gpu_encoder endEncoding];
  gpu_batch = nil;
  gpu_encoder = nil;
  [command commit];
  [command waitUntilCompleted];
  unsigned queued = gpu_queued_count;
  gpu_queued_count = 0;
  if (command.status != MTLCommandBufferStatusCompleted) {
    osp_io_error_text(gpu_queued[0].kernel, "Metal", command.error.localizedDescription.UTF8String);
    return -1;
  }
  for (unsigned i = 0; i < queued; i++) {
    OspreyGpuDispatch done = {gpu_queued[i].kernel, NULL, {{0, NULL, NULL, 0, NULL, 8, NULL, 0, 0}}, 0, gpu_queued[i].threads};
    osprey_gpu_report(gpu_name, atomic_fetch_add(&gpu_completed, 1) + 1, &done);
  }
  // `OSPREY_GPU_TIMING=1` reports each batch's device time, so a program can
  // tell GPU-bound from host-bound ([LLM-SPEED] receipts).
  const char *timing = getenv("OSPREY_GPU_TIMING");
  if (timing != NULL && strcmp(timing, "1") == 0)
    (void)fprintf(stderr, "[gpu] batch of %u dispatches ran %.3f ms on the device\n", queued,
                  (command.GPUEndTime - command.GPUStartTime) * 1000.0);
  return 1;
}

// Whether the host asked for any of this dispatch's outputs.
static bool writes_back(const OspreyGpuDispatch *d) {
  for (unsigned i = 0; i < d->count; i++)
    if (d->args[i].kind == OSPREY_GPU_OUTPUT && d->args[i].out) return true;
  return false;
}

static bool open_batch(void) {
  if (gpu_batch != nil) return true;
  gpu_batch = [gpu_queue commandBuffer];
  gpu_encoder = [gpu_batch computeCommandEncoder];
  if (gpu_batch != nil && gpu_encoder != nil) return true;
  gpu_batch = nil;
  gpu_encoder = nil;
  return false;
}

static int encode(const OspreyGpuDispatch *d, id<MTLComputePipelineState> state) {
  if (!open_batch()) {
    osp_io_error_text(d->kernel, "Metal", "command allocation failed");
    return -1;
  }
  id<MTLBuffer> buffers[OSPREY_GPU_MAX_ARGS] = {nil};
  [gpu_encoder setComputePipelineState:state];
  if (!bind_arguments(gpu_encoder, d, buffers)) return -1;
  NSUInteger group = MIN(state.maxTotalThreadsPerThreadgroup, (NSUInteger)256);
  [gpu_encoder dispatchThreads:MTLSizeMake((NSUInteger)d->threads, 1, 1) threadsPerThreadgroup:MTLSizeMake(group, 1, 1)];
  OspreyGpuQueued *queued = &gpu_queued[gpu_queued_count++];
  (void)snprintf(queued->kernel, sizeof queued->kernel, "%s", d->kernel);
  queued->threads = d->threads;
  if (!writes_back(d) && gpu_queued_count < GPU_BATCH_CAP) return 1;
  int status = flush_locked();
  if (status == 1) write_back(d, buffers);
  return status;
}

static int dispatch_locked(const OspreyGpuDispatch *d) {
  if (!ensure_library(d->source)) {
    osp_io_error_text(d->kernel, "Metal", gpu_failure);
    return -1;
  }
  if (!within_limits(d)) {
    osp_io_error_text(d->kernel, "Metal", "exceeds device buffer or dispatch limits");
    return -1;
  }
  id<MTLComputePipelineState> state = pipeline(d->kernel);
  if (state == nil) {
    osp_io_error_text(d->kernel, "Metal", gpu_failure);
    return -1;
  }
  return encode(d, state);
}

int osprey_gpu_metal_dispatch(const OspreyGpuDispatch *d) {
  (void)pthread_once(&gpu_once, init_device);
  (void)pthread_mutex_lock(&gpu_lock);
  int status;
  @autoreleasepool { status = dispatch_locked(d); }
  (void)pthread_mutex_unlock(&gpu_lock);
  return status;
}
