// Dense scalar buffers for GPU computation — the host execution backend
// [GPU-BACKEND-HOST] behind toGpu/fromGpu and every gpu* combinator
// (docs/specs/runtime/0034-GPUComputation.md). A buffer is { length, data, device,
// stale, adapter, elem_bytes }: length in elements, data contiguous scalar bytes
// with an explicit element width (1, 2, 4 or 8) — the staging layout a device
// transfer consumes — device the selected adapter's object holding the same
// words once a kernel has used the buffer, and stale whether that object
// holds a result the host has not read yet [GPU-BUFFER-RESIDENT]. Elements
// are scalars only [GPU-BUFFER-ELEM], so the memory backends never walk the
// words (OSP_MEM_RAW data under an OSP_MEM_LIST_HDR_EXTERN header, whose
// finalizer releases the device object), and one object serves every
// backend, native and wasm alike.

#include "memory_hooks.h"
#include "gpu_buffer.h"
#include "gpu_device.h"
#include "io_error.h"
#if defined(OSPREY_GPU_OPENCL)
#include "gpu_opencl_api.h"
#endif

#include <stdatomic.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#if !defined(__wasm__) && !defined(_WIN32) && !defined(OSP_SYMBOLIC)
#include <sys/mman.h>
#endif

#if defined(OSPREY_GPU_METAL)
#define GPU_AUTO_ADAPTER 2
#elif defined(OSPREY_GPU_OPENCL)
#define GPU_AUTO_ADAPTER 3
#else
#define GPU_AUTO_ADAPTER 1
#endif

static int device_selection(void) {
  const char *mode = getenv("OSPREY_GPU");
  if (!mode || !*mode || strcmp(mode, "host") == 0) return 0;
  if (strcmp(mode, "auto") == 0) return GPU_AUTO_ADAPTER;
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
  if (selected == 2) return osprey_gpu_metal_name();
#endif
#if defined(OSPREY_GPU_OPENCL)
  if (selected == 3) return osprey_gpu_opencl_name();
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
  if (selected == 2) return osprey_gpu_metal_dispatch(dispatch);
#endif
#if defined(OSPREY_GPU_OPENCL)
  if (selected == 3) return osprey_gpu_opencl_dispatch(dispatch);
#endif
  return no_adapter(dispatch->kernel, "device selection");
}

// Read a device object's words back through the selected adapter [GPU-BUFFER-RESIDENT].
int osprey_gpu_device_download(void *object, void *out, uint64_t count) {
  int selected = device_selection();
#if defined(OSPREY_GPU_METAL)
  if (selected == 2) return osprey_gpu_metal_download(object, out, count);
#endif
#if defined(OSPREY_GPU_OPENCL)
  if (selected == 3) return osprey_gpu_opencl_download(object, out, count);
#endif
  (void)selected;
  (void)object;
  (void)out;
  (void)count;
  return no_adapter("gpu", "device readback");
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
  uint8_t *words = osp_gpu_stored(b);
  void **slot = words ? (b->shared_mapping ? b->map_slot : &b->device) : NULL;
  if (slot && !*slot) {
    int selected = device_selection();
    b->adapter = selected > 0 ? (uint8_t)selected : 0;
  }
  osprey_gpu_arg_at(dispatch, kind, words, NULL, (uint64_t)osprey_gpu_len(handle), b ? (uint64_t)b->elem_bytes : 8, slot);
  if (b && b->mapped && dispatch->count > 0) {
    const OspreyMappedGpuBuffer *mapped = (const OspreyMappedGpuBuffer *)b;
    OspreyGpuArg *bound = &dispatch->args[dispatch->count - 1];
    bound->map_base = mapped->map_base;
    bound->map_bytes = mapped->map_bytes;
    bound->map_skip = (uint64_t)(b->data - (uint8_t *)mapped->map_base);
  }
}

// A completed dispatch leaves its result on the device; the host reads it on demand.
static int settled(int status, void *out) {
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)out;
  if (status > 0 && b && !b->shared_mapping && b->device) b->stale = 1;
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
  OspreyGpuDispatch dispatch = {kernel, NULL, {{0, NULL, NULL, 0, NULL, 8, NULL, 0, 0}}, 0, (uint64_t)(rows * cols)};
  buffer_argument(&dispatch, OSPREY_GPU_INPUT, left);
  buffer_argument(&dispatch, OSPREY_GPU_INPUT, right);
  buffer_argument(&dispatch, OSPREY_GPU_OUTPUT, out);
  scalar_argument(&dispatch, rows);
  scalar_argument(&dispatch, inner);
  scalar_argument(&dispatch, cols);
  return settled(osprey_gpu_device_dispatch(&dispatch), out);
}

// [LLM-SIGMOID] The elementwise logistic function as one library-kernel dispatch.
int osprey_gpu_device_sigmoid(void *x, void *out, int64_t count) {
  OspreyGpuDispatch sigmoid = {"osprey_sigmoid", NULL, {{0, NULL, NULL, 0, NULL, 8, NULL, 0, 0}}, 0, (uint64_t)count};
  buffer_argument(&sigmoid, OSPREY_GPU_INPUT, x);
  buffer_argument(&sigmoid, OSPREY_GPU_OUTPUT, out);
  scalar_argument(&sigmoid, count);
  return settled(osprey_gpu_device_dispatch(&sigmoid), out);
}

// [LLM-DELTA-RULE] `steps` tokens of the delta rule as one library-kernel dispatch.
int osprey_gpu_device_delta_rule(void *const operands[6], void *out, int64_t heads, int64_t dim, int64_t steps) {
  OspreyGpuDispatch rule = {"osprey_delta_rule", NULL, {{0, NULL, NULL, 0, NULL, 8, NULL, 0, 0}}, 0, (uint64_t)(heads * dim)};
  for (unsigned i = 0; i < 6; i++) buffer_argument(&rule, OSPREY_GPU_INPUT, operands[i]);
  buffer_argument(&rule, OSPREY_GPU_OUTPUT, out);
  scalar_argument(&rule, heads);
  scalar_argument(&rule, dim);
  scalar_argument(&rule, steps);
  return settled(osprey_gpu_device_dispatch(&rule), out);
}

// The quantized product as one library-kernel dispatch ([LLM-MATMUL-QUANT]): `x`
// is [rows, inner] float32, `w` the quantized weight rows, and `out` [rows, cols]
// float32 is left on the device. On Metal a product of QUANT_MM_ROWS rows or more
// (a prompt chunk) runs the kernel's `_mm` form on the SIMD-group matrix units,
// held to the oracle within a tolerance; fewer rows run the bit-exact form.
int osprey_gpu_device_matmul_quant(const char *kernel, void *x, void *w, void *out, int64_t rows, int64_t inner,
                                   int64_t cols, const uint64_t threads[2]) {
  int selected = device_selection();
  if (selected <= 0) return selected;
  char tiled[64];
  bool many = selected == 2 && rows >= QUANT_MM_ROWS;
  (void)snprintf(tiled, sizeof tiled, "%s_mm", kernel);
  OspreyGpuDispatch product = {many ? tiled : kernel, NULL, {{0, NULL, NULL, 0, NULL, 8, NULL, 0, 0}}, 0, threads[many]};
  buffer_argument(&product, OSPREY_GPU_INPUT, x);
  buffer_argument(&product, OSPREY_GPU_INPUT, w);
  buffer_argument(&product, OSPREY_GPU_OUTPUT, out);
  scalar_argument(&product, rows);
  scalar_argument(&product, inner);
  scalar_argument(&product, cols);
  return settled(osprey_gpu_device_dispatch(&product), out);
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

static const char *fault_message(int64_t fault) {
  switch (fault) {
  case OSPREY_GPU_FAULT_OVERFLOW: return "integer overflow, which no device kernel can hand to an Arith policy";
  case OSPREY_GPU_FAULT_DIVIDE_BY_ZERO: return "division by zero, which no device kernel can hand to an Arith policy";
  case OSPREY_GPU_FAULT_REMAINDER_BY_ZERO: return "remainder by zero, which no device kernel can hand to an Arith policy";
  default: return "unknown fault";
  }
}

// [FAILURE-EXPLICIT] Preserve the kernel name and the operation's reason.
static int32_t failed_launch(const char *kernel, const char *reason) {
  char text[512];
  (void)snprintf(text, sizeof text, "%s", reason ? reason : "device failure");
  osp_io_error_text("kernel", kernel, text);
  return -1;
}

// Whether a compiled kernel can record a fault: its text names `fault` once in
// the signature, and only a checked integer helper mentions it again. A kernel
// that cannot fault leaves its fault word on the device, so the launch joins
// the open batch instead of submitting and waiting for one word
// [LLM-DISPATCH-BATCH]; an unknown kernel is treated as able to fault.
static int scanned_may_fault(const char *kernel, const char *source) {
  char head[128];
  if (!source || snprintf(head, sizeof head, "kernel void %s(", kernel) >= (int)sizeof head) return 1;
  const char *at = strstr(source, head);
  if (!at) return 1;
  const char *end = strstr(at + 1, "\nkernel void ");
  int mentions = 0;
  for (const char *p = strstr(at, "fault"); p && (!end || p < end); p = strstr(p + 5, "fault")) mentions++;
  return mentions > 1;
}

// The scan's answer per kernel, since a token launches each kernel hundreds of
// times over a megabyte of source. A kernel's name is one constant of its
// program, so its address keys the entry; one atomic word holds the address and
// the answer together, so a racing launch reads a whole entry or none.
#define GPU_FAULT_CACHE 512
static _Atomic uint64_t fault_answers[GPU_FAULT_CACHE];

static int may_fault(const char *kernel, const char *source) {
  uint64_t key = (uint64_t)(uintptr_t)kernel << 1;
  _Atomic uint64_t *entry = &fault_answers[(key >> 4) % GPU_FAULT_CACHE];
  uint64_t known = atomic_load_explicit(entry, memory_order_relaxed);
  if ((known & ~(uint64_t)1) == key) return (int)(known & 1);
  int answer = scanned_may_fault(kernel, source);
  atomic_store_explicit(entry, key | (uint64_t)answer, memory_order_relaxed);
  return answer;
}

// A compiled kernel's launch site [GPU-BACKEND-DEVICE]: 1 when the device ran
// the kernel, 0 when the host loop must run, -1 with a recorded failure. A map runs one thread per
// element into `out`, a buffer left on the device [GPU-BUFFER-RESIDENT]; a
// fold runs one thread over `count` elements seeded from and reducing into
// `acc`, the host's accumulator. The `kinds` string names each uniform's kind.
int32_t osprey_gpu_launch(const char *kernel, const char *source, const char *kinds, const int64_t *uniforms,
                          void *in0, void *in1, void *out, int64_t *acc, int64_t count) {
  int64_t fault = 0;
  OspreyGpuDispatch dispatch = {kernel, source, {{0, NULL, NULL, 0, NULL, 8, NULL, 0, 0}}, 0, acc ? 1 : (uint64_t)count};
  buffer_argument(&dispatch, OSPREY_GPU_INPUT, in0);
  buffer_argument(&dispatch, OSPREY_GPU_INPUT, in1);
  if (acc && !out) osprey_gpu_arg(&dispatch, OSPREY_GPU_OUTPUT, acc, acc, 1, 8);
  else buffer_argument(&dispatch, OSPREY_GPU_OUTPUT, out);
  osprey_gpu_arg(&dispatch, OSPREY_GPU_OUTPUT, &fault, may_fault(kernel, source) ? &fault : NULL, 1, 8);
  scalar_argument(&dispatch, count);
  if (acc && out) osprey_gpu_arg(&dispatch, OSPREY_GPU_OUTPUT, acc, acc, 1, 8);
  for (size_t i = 0; kinds[i]; i++) uniform_argument(&dispatch, kinds[i], uniforms[i]);
  int status = settled(osprey_gpu_device_dispatch(&dispatch), out);
  if (status < 0) return failed_launch(kernel, osp_io_error());
  if (status > 0 && fault) return failed_launch(kernel, fault_message(fault));
  return status;
}

// A launch site whose kernel has no device form [GPU-BACKEND-DEVICE]: the host
// runs it when the host is selected, and a selected device rejects it by name
// rather than silently running it on the CPU.
int32_t osprey_gpu_declined(const char *what) {
  int selected = device_selection();
  if (!selected) return 0;
  if (selected < 0) return failed_launch(what, osp_io_error());
  char text[512];
  (void)snprintf(text, sizeof text, "%s; a selected device cannot run it", what);
  osp_io_error_text("gpu", "kernel decline", text);
  return -1;
}

// { i64 len, ptr data, ptr device, i64 stale, i64 elem_bytes }: the drop runs
// osprey_gpu_finalize over the body, then releases data only — the
// OSP_MEM_LIST_HDR_EXTERN contract, so a dead buffer's device object goes
// with it under every memory backend that reclaims.
#define GPU_HDR_LAYOUT ((int64_t)OSP_MEM_LIST_HDR_EXTERN)

// Unmap a mapped buffer's pages once nothing can reach them. Clearing `data`
// keeps the memory backend from releasing a pointer it never allocated.
static void release_mapping(OspreyGpuBuffer *b) {
#if !defined(__wasm__) && !defined(_WIN32) && !defined(OSP_SYMBOLIC)
  if (!b->mapped) return;
  OspreyMappedGpuBuffer *mapped = (OspreyMappedGpuBuffer *)b;
  if (b->shared_mapping) {
    b->shared_mapping = 0;
    b->map_slot = NULL;
  } else {
    (void)munmap(mapped->map_base, mapped->map_bytes);
  }
  b->mapped = 0;
  b->data = NULL;
#else
  (void)b;
#endif
}

// The memory backends' drop of a buffer [GPU-BUFFER-RESIDENT]: its device
// object is released with it through its owner [GPU-BUFFER-RESIDENT-OWNER].
void osprey_gpu_finalize(void *buffer) {
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)buffer;
#if defined(OSPREY_GPU_METAL)
  if (!b->shared_mapping && b->device && b->adapter == 2) {
    // A queued kernel may still read a mapping the unmap below takes away, so
    // the batch finishes first; a plain object is kept alive by the batch itself.
    if (b->mapped) {
      (void)osprey_gpu_metal_download(b->device, b->data, 0);
      osprey_gpu_metal_release(b->device);
    } else {
      osprey_gpu_metal_recycle(b->device);
    }
  }
#endif
#if defined(OSPREY_GPU_OPENCL)
  if (!b->shared_mapping && b->device && b->adapter == 3) osprey_gpu_opencl_release(b->device);
#endif
  b->device = NULL;
  b->stale = 0;
  b->adapter = 0;
  release_mapping(b);
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
void *osp_gpu_alloc_buffer(int64_t length, int64_t elem_bytes, size_t header_bytes) {
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
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)osp_alloc_tagged((int64_t)header_bytes, GPU_HDR_LAYOUT);
  if (!b) {
    osp_io_error_text("gpu", "buffer allocation", "out of memory");
    return NULL;
  }
  b->length = 0;
  b->data = NULL;
  b->device = NULL;
  b->stale = 0;
  b->adapter = 0;
  b->shared_mapping = 0;
  b->mapped = 0;
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

void *osprey_gpu_alloc(int64_t length, int64_t elem_bytes) {
  return osp_gpu_alloc_buffer(length, elem_bytes, sizeof(OspreyGpuBuffer));
}
