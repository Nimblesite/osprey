// Direct Metal compute adapter [GPU-BACKEND-PORTABILITY] [GPU-BACKEND-DEVICE]:
// one library holding the shader source plus the running program's compiled
// kernels, one pipeline per kernel name, and one dispatch path for the builtin
// product and every compiled kernel alike.
#import <Foundation/Foundation.h>
#import <Metal/Metal.h>
#include <os/lock.h>
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

// A dead buffer's device object waits here for the next result of its exact
// size, so a token's hundreds of results skip the device allocation and the copy
// of zeros [LLM-DISPATCH-BATCH]. Reuse is ordered: the encoder is serial, so a
// queued kernel that reads the old words runs before the kernel that overwrites
// them. A result needs no upload because every kernel writes each element of
// its output, or faults, and a faulted run hands back no buffer. A trylock keeps
// a finalizer on a stopped thread's collection from ever waiting on this pool;
// on contention the object is simply released.
#define GPU_POOL_CAP 64
#define GPU_POOL_BYTES ((NSUInteger)1 << 28)
static id<MTLBuffer> gpu_pool[GPU_POOL_CAP];
static unsigned gpu_pool_count;
static NSUInteger gpu_pool_bytes;
static os_unfair_lock gpu_pool_lock = OS_UNFAIR_LOCK_INIT;

static id<MTLBuffer> pooled(NSUInteger bytes) {
  id<MTLBuffer> found = nil;
  if (!os_unfair_lock_trylock(&gpu_pool_lock)) return nil;
  for (unsigned i = gpu_pool_count; i-- > 0 && found == nil;) {
    if (gpu_pool[i].length != bytes) continue;
    found = gpu_pool[i];
    gpu_pool[i] = gpu_pool[--gpu_pool_count];
    gpu_pool[gpu_pool_count] = nil;
    gpu_pool_bytes -= bytes;
  }
  os_unfair_lock_unlock(&gpu_pool_lock);
  return found;
}

// The new object behind an argument: a mapped region shared without a copy, a
// fresh result from the pool or a new object (every kernel writes each element of
// its output, so a result needs no upload), else a copy of the host words.
static id<MTLBuffer> created(const OspreyGpuArg *arg) {
  NSUInteger bytes = (NSUInteger)osprey_gpu_arg_bytes(arg);
  if (arg->map_base)
    return [gpu_device newBufferWithBytesNoCopy:(void *)(uintptr_t)arg->map_base length:(NSUInteger)arg->map_bytes
                                        options:MTLResourceStorageModeShared deallocator:nil];
  if (arg->kind != OSPREY_GPU_OUTPUT || !arg->slot)
    return [gpu_device newBufferWithBytes:arg->words length:bytes options:MTLResourceStorageModeShared];
  id<MTLBuffer> reused = pooled(bytes);
  return reused != nil ? reused : [gpu_device newBufferWithLength:bytes options:MTLResourceStorageModeShared];
}

// The device object a buffer argument is bound to [GPU-BUFFER-RESIDENT]: the
// one its slot holds, else a new one, kept in the slot of a resident argument
// and alive only for this dispatch otherwise.
static id<MTLBuffer> object_of(const OspreyGpuArg *arg) {
  if (arg->slot && *arg->slot) return (__bridge id<MTLBuffer>)*arg->slot;
  id<MTLBuffer> buffer = created(arg);
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

// A dead buffer's object goes to the pool when there is room; a mapped region's
// object takes osprey_gpu_metal_release instead, since it wraps memory the
// runtime unmaps or frees.
void osprey_gpu_metal_recycle(void *object) {
  @autoreleasepool {
    id<MTLBuffer> buffer = (__bridge_transfer id<MTLBuffer>)object;
    if (os_unfair_lock_trylock(&gpu_pool_lock)) {
      if (gpu_pool_count < GPU_POOL_CAP && gpu_pool_bytes + buffer.length <= GPU_POOL_BYTES) {
        gpu_pool_bytes += buffer.length;
        gpu_pool[gpu_pool_count++] = buffer;
      }
      os_unfair_lock_unlock(&gpu_pool_lock);
    }
  }
}

// [LLM-DISPATCH-BATCH] Dispatches are encoded, in order, into an open command
// buffer. Every GPU_SEGMENT dispatches the buffer is committed without
// waiting and a new one opened, so the device runs one segment while the host
// encodes the next; the host waits only when it needs a result (a write-back
// or a download) or when the ring of queued dispatches is full, and then only
// for the oldest segment. One queue runs committed buffers in order and each
// segment's serial compute encoder orders its dispatches, so a kernel reads
// what the one before it wrote. The command buffer retains every object it
// binds, so a transient or released buffer outlives the dispatch that reads
// it. The kernel names are kept, in a ring, for the completion report and for
// naming a failed segment.
#define GPU_BATCH_CAP 1024
#define GPU_SEGMENT 64
#define GPU_SEGMENTS (GPU_BATCH_CAP / GPU_SEGMENT)
typedef struct { char kernel[48]; uint64_t threads; } OspreyGpuQueued;
typedef struct { id<MTLCommandBuffer> command; unsigned first, count; } OspreyGpuSegment;
static id<MTLCommandBuffer> gpu_batch;
static id<MTLComputeCommandEncoder> gpu_encoder;
static OspreyGpuQueued gpu_queued[GPU_BATCH_CAP];
static unsigned gpu_head, gpu_count, gpu_open_first, gpu_open_count;
static OspreyGpuSegment gpu_segments[GPU_SEGMENTS];
static unsigned gpu_segment_head, gpu_segment_count;

static unsigned ring(unsigned index) { return index % GPU_BATCH_CAP; }

// `OSPREY_GPU_TIMING`: 1 reports each batch's device time; 2 reports every
// dispatch's device time inside its batch ([LLM-SPEED] receipts). Apple GPUs
// sample timestamp counters at stage boundaries only, so mode 2 gives each
// dispatch its own encoder in the same command buffer and reads the pair of
// timestamps around it. Encoder boundaries cost a few microseconds each, so
// a profiled batch runs a little longer than a plain one.
static id<MTLCounterSampleBuffer> gpu_samples;

static int timing_mode(void) {
  static int mode = -1;
  if (mode < 0) {
    const char *text = getenv("OSPREY_GPU_TIMING");
    mode = text == NULL ? 0 : atoi(text);
  }
  return mode;
}

static id<MTLCounterSampleBuffer> sample_buffer(void) {
  if (gpu_samples != nil || ![gpu_device supportsCounterSampling:MTLCounterSamplingPointAtStageBoundary]) return gpu_samples;
  for (id<MTLCounterSet> set in gpu_device.counterSets) {
    if (![set.name isEqualToString:MTLCommonCounterSetTimestamp]) continue;
    MTLCounterSampleBufferDescriptor *samples = [MTLCounterSampleBufferDescriptor new];
    samples.counterSet = set;
    samples.storageMode = MTLStorageModeShared;
    samples.sampleCount = 2 * GPU_BATCH_CAP;
    gpu_samples = [gpu_device newCounterSampleBufferWithDescriptor:samples error:nil];
  }
  return gpu_samples;
}

// A fresh encoder whose start and end timestamps land at 2 * index and 2 * index + 1.
static id<MTLComputeCommandEncoder> timed_encoder(unsigned index) {
  id<MTLCounterSampleBuffer> samples = sample_buffer();
  if (samples == nil) return [gpu_batch computeCommandEncoder];
  MTLComputePassDescriptor *pass = [MTLComputePassDescriptor computePassDescriptor];
  MTLComputePassSampleBufferAttachmentDescriptor *attachment = pass.sampleBufferAttachments[0];
  attachment.sampleBuffer = samples;
  attachment.startOfEncoderSampleIndex = 2 * index;
  attachment.endOfEncoderSampleIndex = 2 * index + 1;
  return [gpu_batch computeCommandEncoderWithDescriptor:pass];
}

// Each dispatch's share of its segment's device time, from its timestamp
// pair, scaled so the pairs span the segment's own GPUStartTime..GPUEndTime.
static void report_dispatch_times(unsigned first, unsigned count, double batch_ms) {
  if (gpu_samples == nil || count == 0) return;
  NSData *data = [gpu_samples resolveCounterRange:NSMakeRange(0, 2 * (NSUInteger)GPU_BATCH_CAP)];
  if (data == nil || data.length < 2 * GPU_BATCH_CAP * sizeof(MTLCounterResultTimestamp)) return;
  const MTLCounterResultTimestamp *stamps = data.bytes;
  uint64_t span = stamps[2 * ring(first + count - 1) + 1].timestamp - stamps[2 * ring(first)].timestamp;
  double per_tick = span == 0 ? 0.0 : batch_ms / (double)span;
  for (unsigned i = first; i < first + count; i++)
    (void)fprintf(stderr, "[gpu] %s over %llu threads ran %.4f ms on the device\n", gpu_queued[ring(i)].kernel,
                  (unsigned long long)gpu_queued[ring(i)].threads,
                  (double)(stamps[2 * ring(i) + 1].timestamp - stamps[2 * ring(i)].timestamp) * per_tick);
}

// The open command buffer goes to the device now; the host does not wait.
static void commit_open(void) {
  if (gpu_batch == nil) return;
  [gpu_encoder endEncoding];
  [gpu_batch commit];
  gpu_segments[(gpu_segment_head + gpu_segment_count) % GPU_SEGMENTS] = (OspreyGpuSegment){gpu_batch, gpu_open_first, gpu_open_count};
  gpu_segment_count++;
  gpu_batch = nil;
  gpu_encoder = nil;
}

static void report_segment(const OspreyGpuSegment *segment) {
  for (unsigned i = segment->first; i < segment->first + segment->count; i++) {
    OspreyGpuQueued *q = &gpu_queued[ring(i)];
    OspreyGpuDispatch done = {q->kernel, NULL, {{0, NULL, NULL, 0, NULL, 8, NULL, 0, 0}}, 0, q->threads};
    osprey_gpu_report(gpu_name, atomic_fetch_add(&gpu_completed, 1) + 1, &done);
  }
  if (timing_mode() < 1) return;
  double batch_ms = (segment->command.GPUEndTime - segment->command.GPUStartTime) * 1000.0;
  if (timing_mode() == 2) report_dispatch_times(segment->first, segment->count, batch_ms);
  (void)fprintf(stderr, "[gpu] batch of %u dispatches ran %.3f ms on the device\n", segment->count, batch_ms);
}

// Waits for the oldest committed segment, reports it and frees its ring slots.
static int complete_oldest(void) {
  OspreyGpuSegment *segment = &gpu_segments[gpu_segment_head % GPU_SEGMENTS];
  [segment->command waitUntilCompleted];
  gpu_segment_head++;
  gpu_segment_count--;
  gpu_head += segment->count;
  gpu_count -= segment->count;
  int status = 1;
  if (segment->command.status != MTLCommandBufferStatusCompleted) {
    osp_io_error_text(gpu_queued[ring(segment->first)].kernel, "Metal", segment->command.error.localizedDescription.UTF8String);
    status = -1;
  } else {
    report_segment(segment);
  }
  segment->command = nil;
  return status;
}

static int flush_locked(void) {
  commit_open();
  int status = 1;
  while (gpu_segment_count > 0)
    if (complete_oldest() < 0) status = -1;
  return status;
}

// Whether the host asked for any of this dispatch's outputs.
static bool writes_back(const OspreyGpuDispatch *d) {
  for (unsigned i = 0; i < d->count; i++)
    if (d->args[i].kind == OSPREY_GPU_OUTPUT && d->args[i].out) return true;
  return false;
}

static bool open_batch(void) {
  if (gpu_batch != nil) return true;
  gpu_open_first = gpu_head + gpu_count;
  gpu_open_count = 0;
  gpu_batch = [gpu_queue commandBuffer];
  gpu_encoder = timing_mode() == 2 ? timed_encoder(ring(gpu_open_first)) : [gpu_batch computeCommandEncoder];
  if (gpu_batch != nil && gpu_encoder != nil) return true;
  gpu_batch = nil;
  gpu_encoder = nil;
  return false;
}

// Room in the ring and the segment list for one more dispatch: the oldest
// segment is completed when either is full.
static int make_room(void) {
  if (gpu_count < GPU_BATCH_CAP && gpu_segment_count < GPU_SEGMENTS) return 1;
  if (gpu_segment_count == 0) commit_open();
  return complete_oldest();
}

static int encode(const OspreyGpuDispatch *d, id<MTLComputePipelineState> state) {
  if (make_room() < 0) return -1;
  if (!open_batch()) {
    osp_io_error_text(d->kernel, "Metal", "command allocation failed");
    return -1;
  }
  if (timing_mode() == 2 && gpu_open_count > 0) {
    [gpu_encoder endEncoding];
    gpu_encoder = timed_encoder(ring(gpu_open_first + gpu_open_count));
  }
  id<MTLBuffer> buffers[OSPREY_GPU_MAX_ARGS] = {nil};
  [gpu_encoder setComputePipelineState:state];
  if (!bind_arguments(gpu_encoder, d, buffers)) return -1;
  // Whole 32-thread lanes per group, at most QUANT_GROUP (gpu_device_lib.metal).
  NSUInteger group = MIN(state.maxTotalThreadsPerThreadgroup, (NSUInteger)256) / 32 * 32;
  if (group == 0) {
    osp_io_error_text(d->kernel, "Metal", "threadgroups narrower than 32 threads");
    return -1;
  }
  [gpu_encoder dispatchThreads:MTLSizeMake((NSUInteger)d->threads, 1, 1) threadsPerThreadgroup:MTLSizeMake(group, 1, 1)];
  OspreyGpuQueued *queued = &gpu_queued[ring(gpu_open_first + gpu_open_count++)];
  gpu_count++;
  (void)snprintf(queued->kernel, sizeof queued->kernel, "%s", d->kernel);
  queued->threads = d->threads;
  if (writes_back(d)) {
    int status = flush_locked();
    if (status == 1) write_back(d, buffers);
    return status;
  }
  if (gpu_open_count >= GPU_SEGMENT) commit_open();
  return 1;
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
