// Mock-driver lifecycle coverage; gpu_device_tests separately proves hardware.
#import <Foundation/Foundation.h>
#import <Metal/Metal.h>
#include "gpu_device.h"
#include "io_error.h"
#include <assert.h>
#include <sys/wait.h>
#include <unistd.h>

#define MOCK_MARK 0x2a
static unsigned failure, libraries, functions, bound, creations, deallocs, commits, fresh_outputs;
static NSUInteger expected_threads = 1;
static NSUInteger dense_bytes;
static void *mapped_base;
static NSUInteger mapped_bytes, mapped_skip;
static unsigned no_copies;
// The buffers bound to the encoder since the last commit.
static NSMutableArray *encoded;
// Timing contracts: the common timestamp set yields one deterministic pair per
// dispatch, and the command buffer exposes the segment's GPU start/end.
#define TIMESTAMP_SAMPLES (2 * 1024) // 2 * GPU_BATCH_CAP in gpu_metal.m
static BOOL counter_supported;
static unsigned clock_reads, counter_set_queries, sample_creations, sample_resolves;
static unsigned timestamp_encoders, plain_encoders;

@interface OspreyMockCounterSet : NSObject
@end
@implementation OspreyMockCounterSet
- (NSString *)name { return MTLCommonCounterSetTimestamp; }
@end

// Sample 2*i starts at i ms and sample 2*i+1 ends 0.5 ms later, so every
// dispatch's pair is exactly 500 ticks however the batch is split.
@interface OspreyMockSampleBuffer : NSObject
@end
@implementation OspreyMockSampleBuffer
- (NSData *)resolveCounterRange:(NSRange)range {
  assert(range.location == 0 && range.length == TIMESTAMP_SAMPLES);
  MTLCounterResultTimestamp stamps[TIMESTAMP_SAMPLES];
  for (NSUInteger i = 0; i < TIMESTAMP_SAMPLES; i++)
    stamps[i].timestamp = (i / 2) * 1000 + (i % 2 ? 500 : 0);
  sample_resolves++;
  return [NSData dataWithBytes:stamps length:sizeof stamps];
}
@end

@interface OspreyMockMetal : NSObject
@property(nonatomic, strong) NSMutableData *data;
@property(nonatomic, assign) void *mapped;
@end
@implementation OspreyMockMetal
- (NSString *)name { return @"mock Metal"; }
- (NSUInteger)maxBufferLength { return failure == 10 ? 1 : 1 << 20; }
// The pool keys reuse on a buffer's exact byte length.
- (NSUInteger)length { return self.data.length; }
- (NSUInteger)maxTotalThreadsPerThreadgroup { return 256; }
- (BOOL)supportsCounterSampling:(MTLCounterSamplingPoint)point {
  assert(point == MTLCounterSamplingPointAtStageBoundary);
  return counter_supported;
}
- (NSArray *)counterSets {
  counter_set_queries++;
  return @[[OspreyMockCounterSet new]];
}
- (id)newCounterSampleBufferWithDescriptor:(MTLCounterSampleBufferDescriptor *)descriptor error:(NSError **)error {
  assert(descriptor.counterSet != nil && [descriptor.counterSet.name isEqualToString:MTLCommonCounterSetTimestamp]);
  assert(descriptor.storageMode == MTLStorageModeShared && descriptor.sampleCount == TIMESTAMP_SAMPLES);
  (void)error;
  sample_creations++;
  return [OspreyMockSampleBuffer new];
}
- (CFTimeInterval)GPUStartTime { clock_reads++; return 1.0; }
- (CFTimeInterval)GPUEndTime { clock_reads++; return 1.0025; }
- (void *)contents { return self.mapped ? self.mapped : self.data.mutableBytes; }
- (void)dealloc { if (self.data) deallocs++; }
- (id)newCommandQueue { return failure == 2 ? nil : [OspreyMockMetal new]; }
- (id)newLibraryWithSource:(NSString *)source options:(id)options error:(NSError **)error {
  assert([source containsString:@"osprey_matmul64"]); (void)error;
  // [GPU-DEVICE-NATIVE]: the driver must request safe math for every library.
  assert([options isKindOfClass:[MTLCompileOptions class]]);
  MTLCompileOptions *compile = options;
#if (defined(__MAC_OS_X_VERSION_MIN_REQUIRED) && __MAC_OS_X_VERSION_MIN_REQUIRED >= 150000) || \
    (defined(__IPHONE_OS_VERSION_MIN_REQUIRED) && __IPHONE_OS_VERSION_MIN_REQUIRED >= 180000)
  assert(compile.mathMode == MTLMathModeSafe);
  assert(compile.mathFloatingPointFunctions == MTLMathFloatingPointFunctionsPrecise);
#else
  if (@available(macOS 15.0, iOS 18.0, *)) {
    assert(compile.mathMode == MTLMathModeSafe);
    assert(compile.mathFloatingPointFunctions == MTLMathFloatingPointFunctionsPrecise);
  } else {
    assert(!compile.fastMathEnabled);
  }
#endif
  assert([source containsString:@"#pragma METAL fp contract(off)"]);
  libraries++;
  return failure == 3 ? nil : [OspreyMockMetal new];
}
- (id)newFunctionWithName:(NSString *)name {
  assert(name.length);
  functions++;
  return failure == 4 ? nil : [OspreyMockMetal new];
}
- (id)newComputePipelineStateWithFunction:(id)function error:(NSError **)error {
  assert(function); (void)error;
  return failure == 5 ? nil : [OspreyMockMetal new];
}
- (id)newBufferWithBytes:(const void *)bytes length:(NSUInteger)length options:(MTLResourceOptions)options {
  assert(bytes && (dense_bytes ? length == dense_bytes : length >= 8) && options == MTLResourceStorageModeShared);
  if (failure == 6) return nil;
  OspreyMockMetal *buffer = [OspreyMockMetal new];
  buffer.data = [NSMutableData dataWithBytes:bytes length:length];
  creations++;
  return buffer;
}
- (id)newBufferWithBytesNoCopy:(void *)bytes length:(NSUInteger)length options:(MTLResourceOptions)options deallocator:(id)deallocator {
  assert(bytes == mapped_base && length == mapped_bytes && options == MTLResourceStorageModeShared && deallocator == nil);
  no_copies++;
  OspreyMockMetal *buffer = [OspreyMockMetal new];
  buffer.mapped = bytes;
  return buffer;
}
- (id)newBufferWithLength:(NSUInteger)length options:(MTLResourceOptions)options {
  fresh_outputs++;
  return [self newBufferWithBytes:[NSMutableData dataWithLength:length].bytes length:length options:options];
}
- (id)commandBuffer { return failure == 7 ? nil : [OspreyMockMetal new]; }
- (id)computeCommandEncoder {
  if (failure == 8) return nil;
  plain_encoders++;
  return [OspreyMockMetal new];
}
- (id)computeCommandEncoderWithDescriptor:(MTLComputePassDescriptor *)descriptor {
  assert(failure != 8);
  MTLComputePassSampleBufferAttachmentDescriptor *attachment = descriptor.sampleBufferAttachments[0];
  assert(attachment != nil && [attachment.sampleBuffer isKindOfClass:[OspreyMockSampleBuffer class]]);
  // Each dispatch in the batch gets the next even/odd timestamp pair.
  assert(attachment.startOfEncoderSampleIndex == 2 * timestamp_encoders);
  assert(attachment.endOfEncoderSampleIndex == 2 * timestamp_encoders + 1);
  timestamp_encoders++;
  return [OspreyMockMetal new];
}
- (void)setComputePipelineState:(id)pipeline { assert(pipeline); }
- (void)setBuffer:(id)buffer offset:(NSUInteger)offset atIndex:(NSUInteger)index {
  OspreyMockMetal *object = buffer;
  assert(buffer && offset == (object.mapped ? mapped_skip : 0) && index < OSPREY_GPU_MAX_ARGS);
  bound++;
  if (!object.mapped) [encoded addObject:buffer];
}
- (void)setBytes:(const void *)bytes length:(NSUInteger)length atIndex:(NSUInteger)index {
  assert(bytes && length == 8 && index < OSPREY_GPU_MAX_ARGS);
  bound++;
}
- (void)dispatchThreads:(MTLSize)size threadsPerThreadgroup:(MTLSize)group {
  assert(size.width == expected_threads && size.height == 1 && size.depth == 1 && group.width == 256);
}
- (void)endEncoding {}
// Synthetic kernels write every output word, including fresh outputs with no upload.
- (void)commit {
  commits++;
  for (OspreyMockMetal *buffer in encoded) {
    if (dense_bytes) memset(buffer.data.mutableBytes, MOCK_MARK, buffer.data.length);
    else {
      int64_t *words = buffer.data.mutableBytes;
      for (NSUInteger i = 0; i < buffer.data.length / sizeof(*words); i++) words[i] = i ? 99 : MOCK_MARK;
    }
  }
  [encoded removeAllObjects];
}
- (void)waitUntilCompleted {}
- (MTLCommandBufferStatus)status { return failure == 9 ? MTLCommandBufferStatusError : MTLCommandBufferStatusCompleted; }
- (NSError *)error { return [NSError errorWithDomain:@"mock" code:7 userInfo:nil]; }
@end

id<MTLDevice> MTLCreateSystemDefaultDevice(void) {
  return failure == 1 ? nil : (id)[OspreyMockMetal new];
}

static OspreyGpuDispatch product_of(const int64_t *in, int64_t *out, void **in_slot, void **out_slot) {
  OspreyGpuDispatch d = {"osprey_matmul64", NULL, {{0, NULL, NULL, 0, NULL, 8, NULL, 0, 0}}, 0, 1};
  osprey_gpu_arg_at(&d, OSPREY_GPU_INPUT, in, NULL, 2, 8, in_slot);
  osprey_gpu_arg_at(&d, OSPREY_GPU_INPUT, in, NULL, 2, 8, in_slot);
  osprey_gpu_arg_at(&d, OSPREY_GPU_OUTPUT, out, out_slot ? NULL : out, 2, 8, out_slot);
  for (uint64_t i = 0; i < 3; i++) osprey_gpu_arg(&d, OSPREY_GPU_SCALAR, NULL, NULL, 1, 8);
  expected_threads = 1;
  return d;
}

static int product(int64_t *out) {
  int64_t in[2] = {0, 0};
  OspreyGpuDispatch d = product_of(in, out, NULL, NULL);
  return osprey_gpu_metal_dispatch(&d);
}

static int compiled(const char *source, int64_t *out, uint64_t threads) {
  int64_t in = 7, fault = 0;
  OspreyGpuDispatch d = {"osprey_kernel_1", source, {{0, NULL, NULL, 0, NULL, 8, NULL, 0, 0}}, 0, threads};
  osprey_gpu_arg(&d, OSPREY_GPU_INPUT, &in, NULL, 1, 8);
  osprey_gpu_arg(&d, OSPREY_GPU_INPUT, &in, NULL, 0, 8);
  osprey_gpu_arg(&d, OSPREY_GPU_OUTPUT, out, out, 2, 8);
  osprey_gpu_arg(&d, OSPREY_GPU_OUTPUT, &fault, &fault, 1, 8);
  osprey_gpu_arg(&d, OSPREY_GPU_SCALAR, NULL, NULL, threads, 8);
  expected_threads = (NSUInteger)threads;
  int status = osprey_gpu_metal_dispatch(&d);
  assert(status != 1 || fault == MOCK_MARK);
  return status;
}

// [LLM-WEIGHTS-MAPPED] A mapped argument becomes one no-copy object over its
// page-aligned mapping, bound at the region's offset, however many dispatches
// read it: nothing is uploaded, and the mapping's pages are the object's.
static void mapped_case(void) {
  mapped_bytes = 2 * (NSUInteger)getpagesize();
  mapped_skip = 32;
  mapped_base = valloc(mapped_bytes);
  assert(mapped_base);
  int64_t out[2] = {99, 99};
  void *slot = NULL;
  for (unsigned round = 0; round < 3; round++) {
    OspreyGpuDispatch d = {"osprey_matmul64", NULL, {{0, NULL, NULL, 0, NULL, 8, NULL, 0, 0}}, 0, 1};
    osprey_gpu_arg_at(&d, OSPREY_GPU_INPUT, (char *)mapped_base + mapped_skip, NULL, 2, 8, &slot);
    d.args[d.count - 1].map_base = mapped_base;
    d.args[d.count - 1].map_bytes = mapped_bytes;
    d.args[d.count - 1].map_skip = mapped_skip;
    osprey_gpu_arg_at(&d, OSPREY_GPU_INPUT, out, NULL, 2, 8, NULL);
    osprey_gpu_arg(&d, OSPREY_GPU_OUTPUT, out, out, 2, 8);
    for (uint64_t i = 0; i < 3; i++) osprey_gpu_arg(&d, OSPREY_GPU_SCALAR, NULL, NULL, 1, 8);
    unsigned before = creations;
    assert(osprey_gpu_metal_dispatch(&d) == 1 && slot);
    assert(no_copies == 1 && creations == before + 2);
  }
  assert(((__bridge OspreyMockMetal *)slot).contents == mapped_base);
  osprey_gpu_metal_release(slot);
  free(mapped_base);
}

// A resident buffer is created once however many dispatches bind it (twice
// in one dispatch binds one object), an output left on the device is
// downloaded on demand, and a release frees the object [GPU-BUFFER-RESIDENT].
static void resident_case(void) {
  int64_t in[2] = {5, 6}, out[2] = {99, 99};
  void *in_slot = NULL, *out_slot = NULL;
  unsigned initial_fresh_outputs = fresh_outputs;
  for (unsigned round = 0; round < 2; round++) {
    OspreyGpuDispatch d = product_of(in, out, &in_slot, &out_slot);
    unsigned before = creations;
    assert(osprey_gpu_metal_dispatch(&d) == 1 && in_slot && out_slot);
    assert(creations == before + (round ? 0 : 2) && out[0] == 99);
  }
  assert(fresh_outputs == initial_fresh_outputs + 1);
  assert(osprey_gpu_metal_download(out_slot, out, sizeof out) == 1 && out[0] == MOCK_MARK && out[1] == 99);
  unsigned before = deallocs;
  osprey_gpu_metal_release(in_slot);
  osprey_gpu_metal_release(out_slot);
  assert(deallocs == before + 2);
}

// [LLM-DISPATCH-BATCH] Dispatches whose outputs stay on the device are encoded
// into one command buffer, submitted once when the host reads a result, and
// each is reported as completed then; a write-back submits at once.
static void batched_case(void) {
  int64_t in[2] = {5, 6}, out[2] = {99, 99};
  void *in_slot = NULL, *out_slot = NULL;
  unsigned before = commits;
  uint64_t done = osprey_gpu_metal_completed();
  for (unsigned round = 0; round < 3; round++) {
    OspreyGpuDispatch d = product_of(in, out, &in_slot, &out_slot);
    assert(osprey_gpu_metal_dispatch(&d) == 1 && commits == before && osprey_gpu_metal_completed() == done);
  }
  assert(osprey_gpu_metal_download(out_slot, out, sizeof out) == 1 && out[0] == MOCK_MARK);
  assert(commits == before + 1 && osprey_gpu_metal_completed() == done + 3);
  assert(osprey_gpu_metal_download(out_slot, out, sizeof out) == 1 && commits == before + 1);
  OspreyGpuDispatch d = product_of(in, out, &in_slot, NULL);
  assert(osprey_gpu_metal_dispatch(&d) == 1 && commits == before + 2 && osprey_gpu_metal_completed() == done + 4);
  osprey_gpu_metal_release(in_slot);
  osprey_gpu_metal_release(out_slot);
}

// [GPU-BUFFER-ELEM]: exact transfers, including one-element empty allocations.
static void dense_transfers(void) {
  expected_threads = 1;
  for (uint64_t width = 1; width <= 8; width *= 2) {
    for (uint64_t count = 0; count <= 3; count += 3) {
      for (unsigned resident = 0; resident <= 1; resident++) {
        unsigned char input[24] = {0}, output[26];
        void *slot = NULL;
        memset(output, 0xa5, sizeof output);
        dense_bytes = (NSUInteger)((count ? count : 1) * width);
        OspreyGpuDispatch d = {"dense_transfer", "kernel void dense_transfer() {}", {{0}}, 0, 1};
        osprey_gpu_arg(&d, OSPREY_GPU_INPUT, input, NULL, count, width);
        osprey_gpu_arg_at(&d, OSPREY_GPU_OUTPUT, output + 1, resident ? NULL : output + 1,
                          count, width, resident ? &slot : NULL);
        assert(osprey_gpu_metal_dispatch(&d) == 1);
        if (resident) {
          for (size_t i = 0; i < sizeof output; i++) assert(output[i] == 0xa5);
          if (count) assert(osprey_gpu_metal_download(slot, output + 1, count * width) == 1);
          osprey_gpu_metal_release(slot);
        }
        for (size_t i = 0; i < sizeof output; i++)
          assert(output[i] == (i > 0 && i <= count * width ? MOCK_MARK : 0xa5));
      }
    }
  }
  dense_bytes = 0;
}

// A resident result for a synthetic kernel: the pool keys reuse on the byte
// length this shape produces, and the mock commit overwrites every word.
static OspreyGpuDispatch recycled_product(const int64_t *in, int64_t *out, uint64_t count, void **slot) {
  OspreyGpuDispatch d = {"recycled_output", NULL, {{0, NULL, NULL, 0, NULL, 8, NULL, 0, 0}}, 0, 1};
  osprey_gpu_arg(&d, OSPREY_GPU_INPUT, in, NULL, count, 8);
  osprey_gpu_arg_at(&d, OSPREY_GPU_OUTPUT, out, NULL, count, 8, slot);
  expected_threads = 1;
  return d;
}

// [LLM-DISPATCH-BATCH] A recycled result's object is pooled under its exact
// byte length and stays alive there: a later result of that length reuses the
// same object without allocating or uploading zeros, a result of another
// length allocates afresh and leaves the pooled object alone, and the reused
// words are the kernel's, not the words the freed result left behind.
static void recycled_case(void) {
  int64_t in[3] = {5, 6, 7}, out[3] = {99, 99, 99};
  void *slot = NULL;
  unsigned start_fresh = fresh_outputs, start_creations = creations;
  unsigned start_deallocs = deallocs, start_commits = commits;
  // A three-word result fills the pool when it is recycled.
  OspreyGpuDispatch first = recycled_product(in, out, 3, &slot);
  assert(osprey_gpu_metal_dispatch(&first) == 1 && slot);
  assert(fresh_outputs == start_fresh + 1 && creations == start_creations + 2);
  assert(osprey_gpu_metal_download(slot, out, sizeof out) == 1 && commits == start_commits + 1);
  assert(out[0] == MOCK_MARK && out[1] == 99 && out[2] == 99);
  void *recycled = slot;
  slot = NULL;
  osprey_gpu_metal_recycle(recycled);
  assert(deallocs == start_deallocs + 1); // transient input released at commit
  // A two-word result cannot use the pooled three-word object.
  void *small_slot = NULL;
  int64_t small[2] = {99, 99};
  OspreyGpuDispatch mismatch = recycled_product(in, small, 2, &small_slot);
  assert(osprey_gpu_metal_dispatch(&mismatch) == 1 && small_slot && small_slot != recycled);
  assert(fresh_outputs == start_fresh + 2 && creations == start_creations + 4);
  assert(osprey_gpu_metal_download(small_slot, small, sizeof small) == 1 && commits == start_commits + 2);
  assert(small[0] == MOCK_MARK && small[1] == 99);
  // A three-word result reuses the pooled object itself, and the commit's
  // words replace stale ones written into it while it sat in the pool.
  memset(((__bridge OspreyMockMetal *)recycled).contents, 0x5a, 3 * sizeof(int64_t));
  void *reuse_slot = NULL;
  OspreyGpuDispatch reuse = recycled_product(in, out, 3, &reuse_slot);
  assert(osprey_gpu_metal_dispatch(&reuse) == 1 && reuse_slot == recycled);
  assert(fresh_outputs == start_fresh + 2 && creations == start_creations + 5);
  assert(osprey_gpu_metal_download(reuse_slot, out, sizeof out) == 1 && commits == start_commits + 3);
  assert(out[0] == MOCK_MARK && out[1] == 99 && out[2] == 99);
  assert(deallocs == start_deallocs + 3); // one transient input per commit
  osprey_gpu_metal_release(small_slot);
  assert(deallocs == start_deallocs + 4);
  // Recycling the reused object again keeps it alive for the pool, not freed.
  osprey_gpu_metal_recycle(reuse_slot);
  assert(deallocs == start_deallocs + 4);
}

// The success path: the product, then a program's kernels, which recompile
// the library once and reuse their pipeline afterwards.
static void success_case(void) {
  static const char source[] = "kernel void osprey_kernel_1() {}";
  int64_t out[2] = {99, 99};
  assert(product(out) == 1 && out[0] == MOCK_MARK && osprey_gpu_metal_completed() == 1);
  assert(libraries == 1 && functions == 1 && bound == 6);
  assert(compiled(source, out, 3) == 1 && libraries == 2 && functions == 2 && out[1] == 99);
  assert(compiled(source, out, 5) == 1 && libraries == 2 && functions == 2);
  assert(product(out) == 1 && libraries == 2 && functions == 3 && osprey_gpu_metal_completed() == 4);
  assert(strcmp(osprey_gpu_metal_name(), "metal:0 (mock Metal)") == 0);
  resident_case();
  mapped_case();
  dense_transfers();
  batched_case();
  recycled_case();
}

static FILE *captured;
static int saved_stderr = -1;

// Capture this child's receipts without touching the parent's stderr.
static void capture_begin(void) {
  captured = tmpfile();
  assert(captured != NULL);
  fflush(stderr);
  saved_stderr = dup(STDERR_FILENO);
  assert(saved_stderr >= 0 && dup2(fileno(captured), STDERR_FILENO) >= 0);
}

static size_t capture_end(char *text, size_t size) {
  fflush(stderr);
  assert(saved_stderr >= 0 && dup2(saved_stderr, STDERR_FILENO) >= 0);
  close(saved_stderr);
  saved_stderr = -1;
  rewind(captured);
  size_t length = fread(text, 1, size - 1, captured);
  text[length] = '\0';
  fclose(captured);
  captured = NULL;
  return length;
}

static unsigned occurrences(const char *text, const char *needle) {
  unsigned count = 0;
  for (const char *at = strstr(text, needle); at != NULL; at = strstr(at + 1, needle)) count++;
  return count;
}

// [LLM-SPEED] Mode 1 reports segment time; mode 2 uses timestamp pairs when
// supported and otherwise falls back to normal encoders without fake samples.
static void timing_receipts(int mode, int supported, unsigned start_resolves, const char *report) {
  if (mode == 2 && supported) {
    assert(counter_set_queries == 1 && sample_creations == 1 && timestamp_encoders == 3);
    assert(plain_encoders == 0 && sample_resolves == start_resolves + 1);
    assert(occurrences(report, "over 1 threads ran 0.5000 ms on the device") == 3);
  } else {
    assert(counter_set_queries == 0 && sample_creations == 0 && timestamp_encoders == 0);
    assert(sample_resolves == start_resolves && plain_encoders == (mode == 1 ? 1 : 3));
    assert(occurrences(report, "over 1 threads ran ") == 0);
  }
  assert(strstr(report, "[gpu] batch of 3 dispatches ran 2.500 ms on the device") != NULL);
}

static void timing_success(int mode, int supported) {
  static char report[4096];
  int64_t in[2] = {5, 6}, out[2] = {99, 99};
  void *in_slot = NULL, *out_slot = NULL;
  unsigned start_commits = commits, start_resolves = sample_resolves;
  capture_begin();
  for (unsigned round = 0; round < 3; round++) {
    OspreyGpuDispatch d = product_of(in, out, &in_slot, &out_slot);
    assert(osprey_gpu_metal_dispatch(&d) == 1 && in_slot && out_slot);
    assert(commits == start_commits && osprey_gpu_metal_completed() == 0);
  }
  assert(osprey_gpu_metal_download(out_slot, out, sizeof out) == 1 && out[0] == MOCK_MARK && out[1] == 99);
  assert(commits == start_commits + 1 && osprey_gpu_metal_completed() == 3);
  assert(clock_reads == 2); // GPUStartTime and GPUEndTime on the one segment
  assert(capture_end(report, sizeof report) > 0);
  timing_receipts(mode, supported, start_resolves, report);
  osprey_gpu_metal_release(in_slot);
  osprey_gpu_metal_release(out_slot);
}

static void timing_case(int mode, int supported) {
  pid_t child = fork();
  assert(child >= 0);
  if (!child) {
    char text[2] = {(char)('0' + mode), '\0'};
    // Set before any dispatch: timing_mode reads the environment once per process.
    assert(setenv("OSPREY_GPU_TIMING", text, 1) == 0);
    failure = 0;
    counter_supported = supported != 0;
    encoded = [NSMutableArray new];
    timing_success(mode, supported);
    exit(0);
  }
  int status = 0;
  assert(waitpid(child, &status, 0) == child);
  assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
}

static void test_case(unsigned mode) {
  pid_t child = fork();
  assert(child >= 0);
  if (!child) {
    failure = mode;
    encoded = [NSMutableArray new];
    int64_t out = 99;
    if (!mode) success_case();
    else {
      assert(product(&out) == -1 && osp_io_error() != NULL && out == 99);
      assert(strcmp(osprey_gpu_metal_name(), mode <= 3 ? "metal:unavailable" : "metal:0 (mock Metal)") == 0);
    }
    exit(0); // Flush gcov from each fresh initialization state.
  }
  int status = 0;
  assert(waitpid(child, &status, 0) == child);
  assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
}

int main(void) {
  for (unsigned i = 0; i <= 10; i++) test_case(i);
  timing_case(1, 0);
  timing_case(2, 0);
  timing_case(2, 1);
  puts("[ok] Metal driver: product and compiled-kernel dispatch, library recompilation, pipeline reuse, resident buffers created once and read back on demand, batched submission, batch and per-dispatch timing with counter fallback, every initialization/allocation/launch/readback error");
  return 0;
}
