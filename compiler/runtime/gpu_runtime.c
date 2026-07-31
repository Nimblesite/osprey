// Dense scalar buffers for GPU computation — the host execution backend
// [GPU-BACKEND-HOST] behind toGpu/fromGpu/gpuMap/gpuFold/gpuLength
// (docs/specs/0034-GPUComputation.md). A buffer is { length, data }: length
// in elements, data a contiguous array of raw 8-byte scalar words (int bits,
// double bits, or 0/1 bools) — the staging layout a device transfer consumes,
// so device backends can adopt this ABI unchanged [GPU-BUFFER]. Elements are
// scalars only [GPU-BUFFER-ELEM], so the memory backends never walk the words
// (OSP_MEM_RAW / OSP_MEM_LIST_HDR_SCALAR layouts) and one object serves every
// backend, native and wasm alike.

#include "memory_hooks.h"

#include <stddef.h>
#include <stdint.h>

typedef struct OspreyGpuBuffer {
  int64_t length;
  int64_t *data;
} OspreyGpuBuffer;

// { i64 len, ptr data } whose drop releases data only — exactly the
// OSP_MEM_LIST_HDR_SCALAR contract, so the ARC backend reclaims a dead
// buffer with no GPU-specific drop code.
#define GPU_HDR_LAYOUT ((int64_t)OSP_MEM_LIST_HDR_SCALAR)

// Elements are 8-byte words; lengths above this would overflow the byte count.
#define GPU_MAX_LEN (INT64_MAX / 8)

// Allocate a zero-filled buffer of `length` scalar words. Out-of-range
// lengths clamp to zero and allocation failure yields NULL; every accessor
// below treats both as an empty buffer instead of trapping.
void *osprey_gpu_alloc(int64_t length) {
  if (length < 0 || length > GPU_MAX_LEN) {
    length = 0;
  }
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)osp_alloc_tagged(
      (int64_t)sizeof(OspreyGpuBuffer), GPU_HDR_LAYOUT);
  if (!b) {
    return NULL;
  }
  int64_t bytes = (length > 0 ? length : 1) * 8;
  b->data = (int64_t *)osp_alloc_tagged(bytes, OSP_MEM_RAW);
  if (!b->data) {
    b->length = 0;
    return b;
  }
  for (int64_t i = 0; i < length; i++) {
    b->data[i] = 0;
  }
  b->length = length;
  return b;
}

// The element count. Empty/NULL-safe [GPU-BUFFER-LENGTH].
int64_t osprey_gpu_len(void *buffer) {
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)buffer;
  return b ? b->length : 0;
}

// The raw word at `index`, or 0 out of bounds. Codegen's counted loops stay
// in bounds by construction; the guard is the no-trap backstop.
int64_t osprey_gpu_get(void *buffer, int64_t index) {
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)buffer;
  if (!b || !b->data || index < 0 || index >= b->length) {
    return 0;
  }
  return b->data[index];
}

// Store the raw word at `index`; out-of-bounds stores are ignored. Buffers
// are immutable at the language surface — only the combinator loops filling
// a freshly allocated buffer call this.
void osprey_gpu_set(void *buffer, int64_t index, int64_t word) {
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)buffer;
  if (!b || !b->data || index < 0 || index >= b->length) {
    return;
  }
  b->data[index] = word;
}

// Whether `index` is addressable — the gate osprey_gpu_get's Result form
// branches on, so a gather reports out-of-bounds instead of reading 0
// [GPU-GET].
int32_t osprey_gpu_in_bounds(void *buffer, int64_t index) {
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)buffer;
  return (b && b->data && index >= 0 && index < b->length) ? 1 : 0;
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
