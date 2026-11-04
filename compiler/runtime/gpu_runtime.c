// Dense scalar buffers for GPU computation — the host execution backend
// [GPU-BACKEND-HOST] behind toGpu/fromGpu and every gpu* combinator
// (docs/specs/runtime/0034-GPUComputation.md). A buffer is { length, data, device,
// stale, elem_bytes }: length in elements, data contiguous scalar bytes
// with an explicit element width (1, 2, 4 or 8) — the staging layout a device
// transfer consumes — device the selected adapter's object holding the same
// words once a kernel has used the buffer, and stale whether that object
// holds a result the host has not read yet [GPU-BUFFER-RESIDENT]. Elements
// are scalars only [GPU-BUFFER-ELEM], so the memory backends never walk the
// words (OSP_MEM_RAW data under an OSP_MEM_LIST_HDR_EXTERN header, whose
// finalizer releases the device object), and one object serves every
// backend, native and wasm alike.

#include "memory_hooks.h"
#include "gpu_device.h"
#include "io_error.h"
#if defined(OSPREY_GPU_OPENCL)
#include "gpu_opencl_api.h"
#endif

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct OspreyGpuBuffer {
  int64_t length;
  uint8_t *data;
  void *device;
  int64_t stale;
  int64_t elem_bytes;
} OspreyGpuBuffer;

int64_t osprey_gpu_len(void *buffer);
int64_t osprey_gpu_elem_bytes(void *buffer);

static int device_selection(void) {
  const char *mode = getenv("OSPREY_GPU");
  if (!mode || !*mode || strcmp(mode, "host") == 0) return 0;
  if (strcmp(mode, "auto") == 0) return 1;
  if (strcmp(mode, "metal") == 0) return 2;
  if (strcmp(mode, "opencl") == 0) return 3;
  osp_io_error_text("gpu", "device selection", "unknown backend; expected host, auto, metal, or opencl");
  return -1;
}

// Implements [GPU-DEVICE]: report the resolved device adapter.
const char *osprey_gpu_device_name(void) {
  int selected = device_selection();
  if (!selected) return "host";
#if defined(OSPREY_GPU_METAL)
  if (selected == 1 || selected == 2) return osprey_gpu_metal_name();
#endif
#if defined(OSPREY_GPU_OPENCL)
  if (selected == 1 || selected == 3) return osprey_gpu_opencl_name();
#endif
  return selected < 0 ? "invalid:device" : "unavailable:device";
}

static int no_adapter(const char *kernel, const char *what) {
  osp_io_error_text(kernel, what, "no GPU adapter in this runtime build");
  return -1;
}

// Route one dispatch to the selected adapter [GPU-BACKEND-DEVICE]. Nothing to
// run completes at once; a device, once selected, never falls back to the host.
int osprey_gpu_device_dispatch(OspreyGpuDispatch *dispatch) {
  int selected = device_selection();
  if (selected <= 0) return selected;
  if (dispatch->count >= OSPREY_GPU_MAX_ARGS) {
    osp_io_error_text(dispatch->kernel, "device dispatch", "too many kernel arguments");
    return -1;
  }
  if (!dispatch->threads) return 1;
#if defined(OSPREY_GPU_METAL)
  if (selected == 1 || selected == 2) return osprey_gpu_metal_dispatch(dispatch);
#endif
#if defined(OSPREY_GPU_OPENCL)
  if (selected == 1 || selected == 3) return osprey_gpu_opencl_dispatch(dispatch);
#endif
  return no_adapter(dispatch->kernel, "device selection");
}

// Read a device object's words back through the selected adapter [GPU-BUFFER-RESIDENT].
int osprey_gpu_device_download(void *object, void *out, uint64_t count) {
  int selected = device_selection();
#if defined(OSPREY_GPU_METAL)
  if (selected == 1 || selected == 2) return osprey_gpu_metal_download(object, out, count);
#endif
#if defined(OSPREY_GPU_OPENCL)
  if (selected == 1 || selected == 3) return osprey_gpu_opencl_download(object, out, count);
#endif
  (void)selected;
  (void)object;
  (void)out;
  (void)count;
  return no_adapter("gpu", "device readback");
}

// Release a device object its buffer no longer needs [GPU-BUFFER-RESIDENT].
void osprey_gpu_device_release(void *object) {
  int selected = device_selection();
#if defined(OSPREY_GPU_METAL)
  if (selected == 1 || selected == 2) {
    osprey_gpu_metal_release(object);
    return;
  }
#endif
#if defined(OSPREY_GPU_OPENCL)
  if (selected == 1 || selected == 3) {
    osprey_gpu_opencl_release(object);
    return;
  }
#endif
  (void)selected;
  (void)object;
}

// The stored words of a buffer with any: NULL for an empty, failed or NULL
// buffer, whose length is 0.
static uint8_t *stored(const OspreyGpuBuffer *b) {
  return b && b->length > 0 ? b->data : NULL;
}

static void scalar_argument(OspreyGpuDispatch *dispatch, int64_t word) {
  osprey_gpu_arg(dispatch, OSPREY_GPU_SCALAR, NULL, NULL, (uint64_t)word, 8);
}

// A buffer handle as a dispatch argument [GPU-BUFFER-RESIDENT]: bound to the
// device object in its slot, created from its words on first use and reused
// by every later dispatch; an output is left on the device. An empty or NULL
// buffer has no words and binds the zero word that `count` keeps every
// thread from reading.
static void buffer_argument(OspreyGpuDispatch *dispatch, OspreyGpuArgKind kind, void *handle) {
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)handle;
  uint8_t *words = stored(b);
  osprey_gpu_arg_at(dispatch, kind, words, NULL, (uint64_t)osprey_gpu_len(handle), b ? (uint64_t)b->elem_bytes : 8, words ? &b->device : NULL);
}

// A completed dispatch leaves its result on the device; the host reads it on demand.
static int settled(int status, void *out) {
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)out;
  if (status > 0 && b && b->device) b->stale = 1;
  return status;
}

// The builtin product as a library-kernel dispatch [GPU-MATMUL].
int osprey_gpu_device_matmul(void *left, void *right, void *out, int64_t rows, int64_t inner, int64_t cols) {
  int64_t width = osprey_gpu_elem_bytes(out);
  if (width != 4 && width != 8) {
    osp_io_error_text("gpuMatMul", "result", "missing float or float32 output buffer");
    return -1;
  }
  const char *kernel = width == 4 ? "osprey_matmul32" : "osprey_matmul64";
  OspreyGpuDispatch dispatch = {kernel, NULL, {{0, NULL, NULL, 0, NULL, 8}}, 0, (uint64_t)(rows * cols)};
  buffer_argument(&dispatch, OSPREY_GPU_INPUT, left);
  buffer_argument(&dispatch, OSPREY_GPU_INPUT, right);
  buffer_argument(&dispatch, OSPREY_GPU_OUTPUT, out);
  scalar_argument(&dispatch, rows);
  scalar_argument(&dispatch, inner);
  scalar_argument(&dispatch, cols);
  return settled(osprey_gpu_device_dispatch(&dispatch), out);
}

// One uniform of a compiled kernel: `s` a scalar word, `b` a buffer handle
// bound as its words followed by its length ([GPU-KERNEL-EXTRACT] uniforms).
static void uniform_argument(OspreyGpuDispatch *dispatch, char kind, int64_t word) {
  if (kind == 'b') {
    buffer_argument(dispatch, OSPREY_GPU_INPUT, (void *)(intptr_t)word);
    scalar_argument(dispatch, osprey_gpu_len((void *)(intptr_t)word));
  } else {
    scalar_argument(dispatch, word);
  }
}

// Broken-code quarantine: a failed load, product or kernel launch used to
// become an empty buffer, and GPT-2 generated from fabricated zeros. Every
// handler and launch site aborts through this instead.
void *osprey_gpu_panic(const char *message) {
  (void)fprintf(stderr, "GPU execution cannot continue: %s\n", message ? message : "unknown GPU failure");
  abort();
}

static const char *fault_message(int64_t fault) {
  switch (fault) {
  case OSPREY_GPU_FAULT_OVERFLOW: return "integer overflow, which no device kernel can hand to an Arith policy";
  case OSPREY_GPU_FAULT_DIVIDE_BY_ZERO: return "division by zero, which no device kernel can hand to an Arith policy";
  case OSPREY_GPU_FAULT_REMAINDER_BY_ZERO: return "remainder by zero, which no device kernel can hand to an Arith policy";
  default: return "unknown fault";
  }
}

// Broken-code quarantine: a `gpuMap` has no error channel, so a device failure
// aborts naming the kernel rather than continuing with an unwritten buffer.
static int32_t abort_launch(const char *kernel, const char *reason) {
  char text[512];
  (void)snprintf(text, sizeof text, "kernel %s: %s", kernel, reason ? reason : "device failure");
  (void)osprey_gpu_panic(text);
  return -1;
}

// A compiled kernel's launch site [GPU-BACKEND-DEVICE]: 1 when the device ran
// the kernel, 0 when the host loop must run. A map runs one thread per
// element into `out`, a buffer left on the device [GPU-BUFFER-RESIDENT]; a
// fold runs one thread over `count` elements seeded from and reducing into
// `acc`, the host's accumulator. The `kinds` string names each uniform's kind.
int32_t osprey_gpu_launch(const char *kernel, const char *source, const char *kinds, const int64_t *uniforms,
                          void *in0, void *in1, void *out, int64_t *acc, int64_t count) {
  int64_t fault = 0;
  OspreyGpuDispatch dispatch = {kernel, source, {{0, NULL, NULL, 0, NULL, 8}}, 0, acc ? 1 : (uint64_t)count};
  buffer_argument(&dispatch, OSPREY_GPU_INPUT, in0);
  buffer_argument(&dispatch, OSPREY_GPU_INPUT, in1);
  if (acc && !out) osprey_gpu_arg(&dispatch, OSPREY_GPU_OUTPUT, acc, acc, 1, 8);
  else buffer_argument(&dispatch, OSPREY_GPU_OUTPUT, out);
  osprey_gpu_arg(&dispatch, OSPREY_GPU_OUTPUT, &fault, &fault, 1, 8);
  scalar_argument(&dispatch, count);
  if (acc && out) osprey_gpu_arg(&dispatch, OSPREY_GPU_OUTPUT, acc, acc, 1, 8);
  for (size_t i = 0; kinds[i]; i++) uniform_argument(&dispatch, kinds[i], uniforms[i]);
  int status = settled(osprey_gpu_device_dispatch(&dispatch), out);
  if (status < 0) return abort_launch(kernel, osp_io_error());
  if (status > 0 && fault) return abort_launch(kernel, fault_message(fault));
  return status;
}

// A launch site whose kernel has no device form [GPU-BACKEND-DEVICE]: the host
// runs it when the host is selected, and a selected device rejects it by name
// rather than silently running it on the CPU.
int32_t osprey_gpu_declined(const char *what) {
  int selected = device_selection();
  if (!selected) return 0;
  if (selected < 0) return abort_launch(what, osp_io_error());
  char text[512];
  (void)snprintf(text, sizeof text, "%s; a selected device cannot run it", what);
  (void)osprey_gpu_panic(text);
  return -1;
}

// { i64 len, ptr data, ptr device, i64 stale, i64 elem_bytes }: the drop runs
// osprey_gpu_finalize over the body, then releases data only — the
// OSP_MEM_LIST_HDR_EXTERN contract, so a dead buffer's device object goes
// with it under every memory backend that reclaims.
#define GPU_HDR_LAYOUT ((int64_t)OSP_MEM_LIST_HDR_EXTERN)

// The memory backends' drop of a buffer [GPU-BUFFER-RESIDENT]: its device
// object is released with it.
void osprey_gpu_finalize(void *buffer) {
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)buffer;
  if (b->device) osprey_gpu_device_release(b->device);
  b->device = NULL;
}

static void register_finalizer(void) {
  static int registered;
  if (!registered) {
    osp_mem_set_finalizer(osprey_gpu_finalize);
    registered = 1;
  }
}

// `length` zero-filled scalar words (at least one, so the empty buffer still
// owns a payload), or NULL when the data allocation fails.
static uint8_t *zeroed_words(int64_t length, int64_t elem_bytes) {
  int64_t bytes = (length > 0 ? length : 1) * elem_bytes;
  uint8_t *data = osp_alloc_tagged(bytes, OSP_MEM_RAW);
  if (data) memset(data, 0, (size_t)bytes);
  return data;
}

// Allocate a zero-filled buffer of `length` scalar words. A negative or
// byte-overflowing length and a failed allocation are NULL with the reason on
// the io_error channel, never a fabricated empty buffer ([FAILURE-EXPLICIT]);
// a zero length is the one valid empty buffer.
void *osprey_gpu_alloc(int64_t length, int64_t elem_bytes) {
  if (elem_bytes != 1 && elem_bytes != 2 && elem_bytes != 4 && elem_bytes != 8) {
    osp_io_error_text("gpu", "buffer allocation", "element width must be 1, 2, 4 or 8 bytes");
    return NULL;
  }
  if (length < 0) {
    osp_io_error_text("gpu", "buffer allocation", "a negative element count");
    return NULL;
  }
  if (length > INT64_MAX / elem_bytes) {
    osp_io_error_text("gpu", "buffer allocation", "element count overflows the byte size");
    return NULL;
  }
  register_finalizer();
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)osp_alloc_tagged((int64_t)sizeof(OspreyGpuBuffer), GPU_HDR_LAYOUT);
  if (!b) {
    osp_io_error_text("gpu", "buffer allocation", "out of memory");
    return NULL;
  }
  b->length = 0;
  b->data = NULL;
  b->device = NULL;
  b->stale = 0;
  b->elem_bytes = elem_bytes;
  b->data = zeroed_words(length, elem_bytes);
  if (!b->data) {
    osp_release(b);
    osp_io_error_text("gpu", "buffer allocation", "out of memory");
    return NULL;
  }
  b->length = length;
  return b;
}

// The element count. Empty/NULL-safe [GPU-BUFFER-LENGTH].
int64_t osprey_gpu_len(void *buffer) {
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)buffer;
  return b ? b->length : 0;
}

// [GPU-BUFFER-ELEM]: allocation and file/product operations share this width.
int64_t osprey_gpu_elem_bytes(void *buffer) {
  const OspreyGpuBuffer *b = buffer;
  return b ? b->elem_bytes : 0;
}

// The buffer with its host words current [GPU-BUFFER-RESIDENT]: a result the
// device holds and the host has not read yet is downloaded first, once. A
// download that fails keeps the buffer stale and records its reason. Language
// gpuGet checks this status before loading; other raw callers still quarantine
// failures until their Result migration is complete [FAILURE-EXPLICIT].
static int synchronize(OspreyGpuBuffer *b) {
  if (b && b->stale && b->device) {
    if (osprey_gpu_device_download(b->device, b->data, (uint64_t)(b->length * b->elem_bytes)) < 0) return -1;
    b->stale = 0;
  }
  return 1;
}

static OspreyGpuBuffer *current(void *buffer) {
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)buffer;
  if (synchronize(b) < 0) (void)osprey_gpu_panic(osp_io_error());
  return b;
}

// Forget the device copy of a buffer whose host words are about to diverge.
static void forget_device(OspreyGpuBuffer *b) {
  if (b->device) osprey_gpu_device_release(b->device);
  b->device = NULL;
  b->stale = 0;
}

// The current words themselves, for the host-backend operations in gpu_host.c
// that walk whole buffers (the dense product, the file loaders) and read-outs;
// NULL for an empty or failed buffer, whose length is 0, so a caller bounding
// its loops by osprey_gpu_len never dereferences it. Only a buffer no kernel
// has used yet is written through this pointer.
void *osprey_gpu_data(void *buffer) {
  return stored(current(buffer));
}

// Whether `index` names a stored word: the ONE bounds predicate every accessor
// below shares, so a read, a write and the Result gate can never disagree
// about which indices exist. NULL, a data-less (failed) allocation and the
// published-length tail are all out of bounds.
static int addressable(const OspreyGpuBuffer *b, int64_t index) {
  return b && b->data && index >= 0 && index < b->length;
}

// [FAILURE-EXPLICIT] The Result-returning language read checks this before
// loading any word: 0 is out of bounds, -1 carries a download error, 1 is ready.
int32_t osprey_gpu_readable(void *buffer, int64_t index) {
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)buffer;
  return addressable(b, index) ? synchronize(b) : 0;
}

// The raw word at `index`, or 0 out of bounds. Codegen's counted loops stay
// in bounds by construction; the guard is the no-trap backstop.
int64_t osprey_gpu_get(void *buffer, int64_t index) {
  const OspreyGpuBuffer *b = current(buffer);
  uint64_t word = 0;
  if (addressable(b, index)) memcpy(&word, b->data + index * b->elem_bytes, (size_t)b->elem_bytes);
  return (int64_t)word;
}

// Store the raw word at `index`; out-of-bounds stores are ignored. Buffers
// are immutable at the language surface — only the combinator loops filling
// a freshly allocated buffer call this — so a device copy, if any, is forgotten.
void osprey_gpu_set(void *buffer, int64_t index, int64_t word) {
  OspreyGpuBuffer *b = current(buffer);
  if (addressable(b, index)) {
    memcpy(b->data + index * b->elem_bytes, &word, (size_t)b->elem_bytes);
    forget_device(b);
  }
}

// The gate osprey_gpu_get's Result form branches on, so a gather reports
// out-of-bounds instead of reading 0 [GPU-GET].
int32_t osprey_gpu_in_bounds(void *buffer, int64_t index) {
  return addressable((const OspreyGpuBuffer *)buffer, index) ? 1 : 0;
}

// Shrink a compaction scratch buffer to its first `count` elements in place.
// Stream compaction [GPU-FILTER] cannot know its output length before running
// the predicate, so it over-allocates to the source length, fills a prefix,
// then publishes the exact length here. The surplus tail stays allocated
// until the buffer dies — no reallocation, no copy.
void osprey_gpu_take(void *buffer, int64_t count) {
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)buffer;
  if (!b) {
    return;
  }
  if (count < 0) {
    count = 0;
  }
  if (count < b->length) {
    b->length = count;
  }
}
