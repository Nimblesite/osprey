// Internal layout of a GPU buffer, shared by gpu_runtime.c (allocation,
// residency, element access) and gpu_map_file.c (file-backed buffers). Not part
// of the runtime ABI: compiled code sees only the opaque handle.
#ifndef OSPREY_GPU_BUFFER_H
#define OSPREY_GPU_BUFFER_H

#include <stddef.h>
#include <stdint.h>

typedef struct OspreyGpuBuffer {
  int64_t length;
  uint8_t *data;
  // A shared mapping borrows its file's slot; an ordinary buffer owns its slot.
  union { void *device; void **map_slot; };
  uint8_t stale;
  uint8_t adapter; // Owning SDK; fits the existing header padding.
  uint8_t shared_mapping; // Discriminates the exclusive slots without growing the header.
  uint8_t mapped;
  int64_t elem_bytes;
} OspreyGpuBuffer;

// [LLM-WEIGHTS-MAPPED] Only mapped allocations carry the page extent.
typedef struct OspreyMappedGpuBuffer {
  OspreyGpuBuffer buffer;
  void *map_base;
  size_t map_bytes;
} OspreyMappedGpuBuffer;

// A zero-filled buffer of `length` scalar words of `elem_bytes` each, with a
// `header_bytes` header (a plain or a mapped buffer). NULL, with the reason on
// the io_error channel, for a bad width or length or a failed allocation.
void *osp_gpu_alloc_buffer(int64_t length, int64_t elem_bytes, size_t header_bytes);

// The stored words of a buffer with any: NULL for an empty, failed or NULL
// buffer, whose length is 0.
static inline uint8_t *osp_gpu_stored(const OspreyGpuBuffer *b) {
  return b && b->length > 0 ? b->data : NULL;
}

int64_t osprey_gpu_len(void *buffer);
int64_t osprey_gpu_elem_bytes(void *buffer);
void osprey_gpu_finalize(void *buffer);

#endif
