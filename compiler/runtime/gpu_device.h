#ifndef OSPREY_GPU_DEVICE_H
#define OSPREY_GPU_DEVICE_H
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// The shared device contract [GPU-BACKEND-PORTABILITY] [GPU-BACKEND-DEVICE]:
// one dispatch description every adapter executes. An argument is a buffer of
// bytes with an explicit element width, or one scalar word. A
// buffer argument names the slot its device object lives in
// [GPU-BUFFER-RESIDENT]: the adapter creates the object from the bytes the
// first time and reuses it after, so a buffer is uploaded once however many
// kernels read it, and an output stays on the device until the host asks.
typedef enum { OSPREY_GPU_INPUT = 0, OSPREY_GPU_OUTPUT = 1, OSPREY_GPU_SCALAR = 2 } OspreyGpuArgKind;

typedef struct {
  OspreyGpuArgKind kind;
  // INPUT/OUTPUT: the bytes the device object is created from when its slot
  // holds none yet.
  const void *words;
  // OUTPUT: where the device's bytes are copied after completion; NULL
  // leaves them on the device for a later download.
  void *out;
  // INPUT/OUTPUT: the element count (at least one element is allocated);
  // SCALAR: the value.
  uint64_t count;
  // The buffer's device object, kept across dispatches; NULL for a transient
  // argument whose object the adapter releases after this dispatch.
  void **slot;
  // [GPU-BUFFER-ELEM]: element width; scalar uniforms still carry one raw word.
  uint64_t elem_bytes;
} OspreyGpuArg;

#define OSPREY_GPU_MAX_ARGS 40

typedef struct {
  // The entry point, and the compiled program's kernels appended to the
  // library source (NULL for a library kernel such as the product).
  const char *kernel;
  const char *source;
  OspreyGpuArg args[OSPREY_GPU_MAX_ARGS];
  unsigned count;
  uint64_t threads;
} OspreyGpuDispatch;

// Zero selects the host oracle, one means the device completed the work and
// every OUTPUT argument holds its result, -1 an error on the io_error channel.
int osprey_gpu_device_dispatch(OspreyGpuDispatch *dispatch);
// The builtin product over three buffer handles [GPU-MATMUL]; `out` is left
// on the device.
int osprey_gpu_device_matmul(void *left, void *right, void *out, int64_t rows, int64_t inner, int64_t cols);
// Copy a device object's first `count` bytes to `out`: 1, or -1 with the
// reason on the io_error channel.
int osprey_gpu_device_download(void *object, void *out, uint64_t count);
// Release a device object its buffer no longer needs.
void osprey_gpu_device_release(void *object);
const char *osprey_gpu_device_name(void);

// The fault word a compiled kernel records when an operation cannot reach its
// runtime policy [GPU-DEVICE-FAULT]; mirrors gpu_device_lib.metal.
#define OSPREY_GPU_FAULT_OVERFLOW 1
#define OSPREY_GPU_FAULT_DIVIDE_BY_ZERO 2
#define OSPREY_GPU_FAULT_REMAINDER_BY_ZERO 3

// The first completed dispatch is always reported; `OSPREY_GPU_TRACE=1`
// reports every one.
static inline int osprey_gpu_traced(uint64_t count) {
  const char *trace = getenv("OSPREY_GPU_TRACE");
  return count == 1 || (trace != NULL && strcmp(trace, "1") == 0);
}

static inline void osprey_gpu_report(const char *device, uint64_t count, const OspreyGpuDispatch *dispatch) {
  if (osprey_gpu_traced(count))
    (void)fprintf(stderr, "[gpu] %s: completed dispatch %llu: %s over %llu threads\n", device,
                  (unsigned long long)count, dispatch->kernel, (unsigned long long)dispatch->threads);
}

// Zero storage an empty buffer uploads at its element width; `count` keeps
// the kernel from using it. Eight bytes accommodate every supported element.
static const int64_t osprey_gpu_zero_word = 0;

// Append one argument bound to the device object in `slot` (NULL for a
// transient one); the dispatch is refused (count left at the cap) when full.
// A buffer argument without words has nothing to hold: the zero word is
// transferred with a count of 0, so no thread reads it and nothing comes back.
static inline void osprey_gpu_arg_at(OspreyGpuDispatch *dispatch, OspreyGpuArgKind kind, const void *words,
                                     void *out, uint64_t count, uint64_t elem_bytes, void **slot) {
  if (dispatch->count >= OSPREY_GPU_MAX_ARGS) return;
  const void *uploaded = words ? words : &osprey_gpu_zero_word;
  uint64_t kept = kind != OSPREY_GPU_SCALAR && !words ? 0 : count;
  dispatch->args[dispatch->count++] = (OspreyGpuArg){kind, uploaded, out, kept, slot, elem_bytes};
}

// Append one transient argument: its bytes serve this dispatch only.
static inline void osprey_gpu_arg(OspreyGpuDispatch *dispatch, OspreyGpuArgKind kind, const void *words,
                                  void *out, uint64_t count, uint64_t elem_bytes) {
  osprey_gpu_arg_at(dispatch, kind, words, out, count, elem_bytes, NULL);
}

static inline uint64_t osprey_gpu_arg_bytes(const OspreyGpuArg *arg) {
  return (arg->count ? arg->count : 1) * arg->elem_bytes;
}

#if defined(OSPREY_GPU_METAL)
int osprey_gpu_metal_dispatch(const OspreyGpuDispatch *dispatch);
int osprey_gpu_metal_download(void *object, void *out, uint64_t count);
void osprey_gpu_metal_release(void *object);
const char *osprey_gpu_metal_name(void);
uint64_t osprey_gpu_metal_completed(void);
#endif
#endif
