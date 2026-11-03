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

// Compile the library when there is none, or when a dispatch brings program
// kernels the current one lacks; the pipelines of the old library are dropped.
static bool ensure_library(const char *source) {
  if (gpu_queue == nil) return false;
  if (gpu_library != nil && (!source || source == gpu_source)) return true;
  NSString *text = [NSString stringWithFormat:@"%s\n%s", osprey_gpu_shader, source ? source : ""];
  NSError *error = nil;
  // [GPU-DEVICE-NATIVE]: preserve IEEE arithmetic independently of driver defaults.
  MTLCompileOptions *options = [MTLCompileOptions new];
  if (@available(macOS 15.0, iOS 18.0, *)) {
    options.mathMode = MTLMathModeSafe;
    options.mathFloatingPointFunctions = MTLMathFloatingPointFunctionsPrecise;
  } else {
    options.fastMathEnabled = NO;
  }
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
    if (d->args[i].kind != OSPREY_GPU_SCALAR && osprey_gpu_arg_bytes(&d->args[i]) > gpu_device.maxBufferLength)
      return false;
  }
  return d->threads <= UINT32_MAX;
}

// The device object a buffer argument is bound to [GPU-BUFFER-RESIDENT]: the
// one its slot holds, else a new one created from its words, kept in the slot
// of a resident argument and alive only for this dispatch otherwise.
static id<MTLBuffer> object_of(const OspreyGpuArg *arg) {
  if (arg->slot && *arg->slot) return (__bridge id<MTLBuffer>)*arg->slot;
  id<MTLBuffer> buffer = [gpu_device newBufferWithBytes:arg->words length:(NSUInteger)osprey_gpu_arg_bytes(arg)
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
    [encoder setBuffer:buffers[i] offset:0 atIndex:i];
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

// Every dispatch waits for completion under the lock, so a shared-storage
// buffer's contents are the device's final words by the time they are read.
// The caller is plain C with no autorelease pool, so the bridge gets its own.
int osprey_gpu_metal_download(void *object, void *out, uint64_t count) {
  (void)pthread_mutex_lock(&gpu_lock);
  @autoreleasepool {
    memcpy(out, ((__bridge id<MTLBuffer>)object).contents, (size_t)count);
  }
  (void)pthread_mutex_unlock(&gpu_lock);
  return 1;
}

// Hand the retained object back to ARC, which releases it at once.
void osprey_gpu_metal_release(void *object) {
  @autoreleasepool {
    (void)CFBridgingRelease(object);
  }
}

static int completed(id<MTLCommandBuffer> command, const OspreyGpuDispatch *d) {
  [command commit];
  [command waitUntilCompleted];
  if (command.status != MTLCommandBufferStatusCompleted) {
    osp_io_error_text(d->kernel, "Metal", command.error.localizedDescription.UTF8String);
    return -1;
  }
  uint64_t count = atomic_fetch_add(&gpu_completed, 1) + 1;
  osprey_gpu_report(gpu_name, count, d);
  return 1;
}

static int encode(const OspreyGpuDispatch *d, id<MTLComputePipelineState> state) {
  id<MTLCommandBuffer> command = [gpu_queue commandBuffer];
  id<MTLComputeCommandEncoder> encoder = [command computeCommandEncoder];
  if (command == nil || encoder == nil) {
    osp_io_error_text(d->kernel, "Metal", "command allocation failed");
    return -1;
  }
  id<MTLBuffer> buffers[OSPREY_GPU_MAX_ARGS] = {nil};
  [encoder setComputePipelineState:state];
  bool bound = bind_arguments(encoder, d, buffers);
  NSUInteger group = MIN(state.maxTotalThreadsPerThreadgroup, (NSUInteger)256);
  if (bound) [encoder dispatchThreads:MTLSizeMake((NSUInteger)d->threads, 1, 1) threadsPerThreadgroup:MTLSizeMake(group, 1, 1)];
  [encoder endEncoding];
  int status = bound ? completed(command, d) : -1;
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
