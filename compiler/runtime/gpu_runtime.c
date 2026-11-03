// Dense scalar buffers for GPU computation — the host execution backend
// [GPU-BACKEND-HOST] behind toGpu/fromGpu and every gpu* combinator
// (docs/specs/runtime/0034-GPUComputation.md). A buffer is { length, data,
// elem_bytes, device, stale }: length in elements, data a contiguous array of
// raw bytes holding each element at its own width [GPU-BUFFER-ELEM] — one byte
// for bool, int8 and uint8, two for int16, uint16 and float16, four for int32,
// uint32 and float32, eight for int and float; the same bytes are what the
// device reads [GPU-BUFFER-RESIDENT] — device the selected adapter's object
// holding them once a kernel has used the buffer, and stale whether that object
// holds a result the host has not read yet. Elements are scalars only
// [GPU-BUFFER-ELEM], so the memory backends never walk the bytes
// (OSP_MEM_RAW data under an OSP_MEM_LIST_HDR_EXTERN header, whose finalizer
// releases the device object), and one object serves every backend, native and
// wasm alike.

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
  unsigned char *data;
  int64_t elem_bytes;
  void *device;
  int64_t stale;
} OspreyGpuBuffer;

// One element's width in bytes [GPU-BUFFER-ELEM]; the numeric table's bits/8,
// one byte for bool. Every element travels a little-endian byte sequence, so
// the loaders' file layout and a buffer's memory layout are the same bytes.
#define GPU_ELEM_MIN 1
#define GPU_ELEM_MAX 8

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

// Read a device object's bytes back through the selected adapter
// [GPU-BUFFER-RESIDENT].
int osprey_gpu_device_download(void *object, void *out, uint64_t bytes) {
  int selected = device_selection();
#if defined(OSPREY_GPU_METAL)
  if (selected == 1 || selected == 2) return osprey_gpu_metal_download(object, out, bytes);
#endif
#if defined(OSPREY_GPU_OPENCL)
  if (selected == 1 || selected == 3) return osprey_gpu_opencl_download(object, out, bytes);
#endif
  (void)selected;
  (void)object;
  (void)out;
  (void)bytes;
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

// The stored bytes of a buffer with any: NULL for an empty, failed or NULL
// buffer, whose length is 0.
static unsigned char *stored(const OspreyGpuBuffer *b) {
  return b && b->length > 0 ? b->data : NULL;
}

// A buffer handle as a dispatch argument [GPU-BUFFER-RESIDENT]: bound to the
// device object in its slot, created from its bytes on first use and reused by
// every later dispatch; an output is left on the device. An empty or NULL
// buffer has no bytes and binds the zero word that `count` keeps every thread
// from reading.
static void buffer_argument(OspreyGpuDispatch *dispatch, OspreyGpuArgKind kind, void *handle) {
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)handle;
  unsigned char *bytes = stored(b);
  osprey_gpu_arg_at(dispatch, kind, bytes, NULL, (uint64_t)osprey_gpu_len(handle),
                    (uint64_t)osprey_gpu_elem_bytes(handle), bytes ? &b->device : NULL);
}

// A completed dispatch leaves its result on the device; the host reads it on demand.
static int settled(int status, void *out) {
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)out;
  if (status > 0 && b && b->device) b->stale = 1;
  return status;
}

// The builtin product as a library-kernel dispatch [GPU-MATMUL]. Only the
// 8-byte element product has a device kernel; a narrower one answers 0 so the
// host runs it (and `osprey_gpu_matmul` asks the declined question first, so a
// selected device is never silently substituted by the host).
int osprey_gpu_device_matmul(void *left, void *right, void *out, int64_t rows, int64_t inner, int64_t cols) {
  if (osprey_gpu_elem_bytes(left) != 8) return 0;
  OspreyGpuDispatch dispatch = {"osprey_matmul64", NULL, {{0, NULL, NULL, 0, 0, NULL}}, 0, (uint64_t)(rows * cols)};
  buffer_argument(&dispatch, OSPREY_GPU_INPUT, left);
  buffer_argument(&dispatch, OSPREY_GPU_INPUT, right);
  buffer_argument(&dispatch, OSPREY_GPU_OUTPUT, out);
  osprey_gpu_arg_scalar(&dispatch, (uint64_t)rows);
  osprey_gpu_arg_scalar(&dispatch, (uint64_t)inner);
  osprey_gpu_arg_scalar(&dispatch, (uint64_t)cols);
  return settled(osprey_gpu_device_dispatch(&dispatch), out);
}

// One uniform of a compiled kernel: `s` a scalar word, `b` a buffer handle
// bound as its bytes followed by its length ([GPU-KERNEL-EXTRACT] uniforms).
static void uniform_argument(OspreyGpuDispatch *dispatch, char kind, int64_t word) {
  if (kind == 'b') {
    buffer_argument(dispatch, OSPREY_GPU_INPUT, (void *)(intptr_t)word);
    osprey_gpu_arg_scalar(dispatch, (uint64_t)osprey_gpu_len((void *)(intptr_t)word));
  } else {
    osprey_gpu_arg_scalar(dispatch, (uint64_t)word);
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

// The fault word a launch writes instead of aborting mid-kernel.
static OspreyGpuDispatch empty_dispatch(const char *kernel, const char *source, uint64_t threads) {
  OspreyGpuDispatch dispatch;
  memset(&dispatch, 0, sizeof dispatch);
  dispatch.kernel = kernel;
  dispatch.source = source;
  dispatch.threads = threads;
  return dispatch;
}

// A compiled kernel's launch site [GPU-BACKEND-DEVICE]: 1 when the device ran
// the kernel, 0 when the host loop must run. A map runs one thread per element
// into `out`, a buffer left on the device [GPU-BUFFER-RESIDENT]; a fold runs
// one thread over `count` elements seeded from and reducing into `acc`, the
// host's accumulator. The `kinds` string names each uniform's kind.
int32_t osprey_gpu_launch(const char *kernel, const char *source, const char *kinds, const int64_t *uniforms,
                          void *in0, void *in1, void *out, int64_t *acc, int64_t count) {
  int64_t fault = 0;
  OspreyGpuDispatch dispatch = empty_dispatch(kernel, source, acc ? 1 : (uint64_t)count);
  buffer_argument(&dispatch, OSPREY_GPU_INPUT, in0);
  buffer_argument(&dispatch, OSPREY_GPU_INPUT, in1);
  if (acc) osprey_gpu_arg(&dispatch, OSPREY_GPU_OUTPUT, acc, acc, 1, 8);
  else buffer_argument(&dispatch, OSPREY_GPU_OUTPUT, out);
  osprey_gpu_arg(&dispatch, OSPREY_GPU_OUTPUT, &fault, &fault, 1, 8);
  osprey_gpu_arg_scalar(&dispatch, (uint64_t)count);
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

// { i64 len, ptr data, i64 elem, ptr device, i64 stale }: the drop runs
// osprey_gpu_finalize over the body, then releases data only — the
// OSP_MEM_LIST_HDR_EXTERN contract, so a dead buffer's device object goes
// with it under every memory backend that reclaims. The ARC drop reads `len`
// at offset 0 and `data` at offset 8, so those two fields come first.
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

// A requested length made total at `elem` bytes per element: negative lengths
// and lengths whose byte count would overflow become zero, so every allocation
// below is for a representable size.
static int64_t clamped_len(int64_t length, int64_t elem) {
  if (length < 0 || length > INT64_MAX / elem) return 0;
  return length;
}

// `length` zero-filled elements of `elem` bytes (at least one element, so the
// empty buffer still owns a payload), or NULL when the data allocation fails.
static unsigned char *zeroed_bytes(int64_t length, int64_t elem) {
  int64_t bytes = (length > 0 ? length : 1) * elem;
  unsigned char *data = (unsigned char *)osp_alloc_tagged(bytes, OSP_MEM_RAW);
  if (data) memset(data, 0, (size_t)bytes);
  return data;
}

// Allocate a zero-filled buffer of `length` elements of `elem` bytes each
// [GPU-BUFFER-ELEM] — the one allocator entry point; the element width is an
// argument, never a second table. Out-of-range lengths clamp to zero, an
// out-of-range width is treated as the word, and allocation failure yields
// NULL; every accessor below treats all three as an empty buffer instead of
// trapping.
void *osprey_gpu_alloc(int64_t length, int64_t elem) {
  register_finalizer();
  if (elem < GPU_ELEM_MIN || elem > GPU_ELEM_MAX) elem = 8;
  length = clamped_len(length, elem);
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)osp_alloc_tagged((int64_t)sizeof(OspreyGpuBuffer), GPU_HDR_LAYOUT);
  if (!b) {
    return NULL;
  }
  b->elem_bytes = elem;
  b->data = zeroed_bytes(length, elem);
  b->length = b->data ? length : 0;
  b->device = NULL;
  b->stale = 0;
  return b;
}

// The element count. Empty/NULL-safe [GPU-BUFFER-LENGTH].
int64_t osprey_gpu_len(void *buffer) {
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)buffer;
  return b ? b->length : 0;
}

// One element's width in bytes [GPU-BUFFER-ELEM]; the word for a NULL buffer,
// which is the empty buffer.
int64_t osprey_gpu_elem_bytes(void *buffer) {
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)buffer;
  return b && b->elem_bytes >= GPU_ELEM_MIN && b->elem_bytes <= GPU_ELEM_MAX ? b->elem_bytes : 8;
}

// The buffer with its host bytes current [GPU-BUFFER-RESIDENT]: a result the
// device holds and the host has not read yet is downloaded first, once. A
// download that fails cannot become zeros, so it aborts (broken-code quarantine).
static OspreyGpuBuffer *current(void *buffer) {
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)buffer;
  if (b && b->stale && b->device) {
    uint64_t bytes = (uint64_t)(b->length > 0 ? b->length : 1) * (uint64_t)b->elem_bytes;
    if (osprey_gpu_device_download(b->device, b->data, bytes) < 0) (void)osprey_gpu_panic(osp_io_error());
    b->stale = 0;
  }
  return b;
}

// Forget the device copy of a buffer whose host bytes are about to diverge.
static void forget_device(OspreyGpuBuffer *b) {
  if (b->device) osprey_gpu_device_release(b->device);
  b->device = NULL;
  b->stale = 0;
}

// The current bytes themselves, for the host-backend operations in gpu_host.c
// that walk whole buffers (the dense product, the file loaders) and read-outs;
// NULL for an empty or failed buffer, whose length is 0, so a caller bounding
// its loops by osprey_gpu_len never dereferences it. Only a buffer no kernel
// has used yet is written through this pointer. The payload allocation is
// max-aligned, so a caller may read its elements as words.
void *osprey_gpu_bytes(void *buffer) {
  return stored(current(buffer));
}

// The little-endian `elem` bytes at `at`, zero-extended to a word.
static int64_t load_le(const unsigned char *at, int64_t elem) {
  uint64_t word = 0;
  for (int64_t i = elem - 1; i >= 0; i--) word = (word << 8) | at[i];
  return (int64_t)word;
}

// The low `elem` bytes of `word`, little-endian, at `at`.
static void store_le(unsigned char *at, int64_t elem, int64_t word) {
  uint64_t wide = (uint64_t)word;
  for (int64_t i = 0; i < elem; i++) {
    at[i] = (unsigned char)(wide & 0xff);
    wide >>= 8;
  }
}

// Whether `index` names a stored element: the ONE bounds predicate every accessor
// below shares, so a read, a write and the Result gate can never disagree
// about which indices exist. NULL, a data-less (failed) allocation and the
// published-length tail are all out of bounds.
static int addressable(const OspreyGpuBuffer *b, int64_t index) {
  return b && b->data && index >= 0 && index < b->length;
}

// The zero-extended bytes of the element at `index`, or 0 out of bounds.
// Codegen's counted loops stay in bounds by construction; the guard is the
// no-trap backstop. A caller that knows the element's type narrows the word:
// an int8's byte sign-extends back when it is read at int8.
int64_t osprey_gpu_get(void *buffer, int64_t index) {
  const OspreyGpuBuffer *b = current(buffer);
  return addressable(b, index) ? load_le(b->data + index * b->elem_bytes, b->elem_bytes) : 0;
}

// Store the low bytes of `word` at `index`; out-of-bounds stores are ignored.
// Buffers are immutable at the language surface — only the combinator loops
// filling a freshly allocated buffer call this — so a device copy, if any, is
// forgotten.
void osprey_gpu_set(void *buffer, int64_t index, int64_t word) {
  OspreyGpuBuffer *b = current(buffer);
  if (addressable(b, index)) {
    store_le(b->data + index * b->elem_bytes, b->elem_bytes, word);
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
