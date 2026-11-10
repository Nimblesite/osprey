// Host access to a GPU buffer's elements [GPU-BUFFER-ELEM]: length and width,
// the synchronizing read-out of words a kernel left on the device
// [GPU-BUFFER-RESIDENT], the one bounds predicate every accessor shares, and
// the in-place fills and shrink the combinator loops use.
#include "gpu_buffer.h"
#include "gpu_device.h"

#include <stdint.h>
#include <string.h>

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
  if (b && !b->shared_mapping && b->stale && b->device) {
    if (osprey_gpu_device_download(b->device, b->data, (uint64_t)(b->length * b->elem_bytes)) < 0) return -1;
    b->stale = 0;
  }
  return 1;
}

static OspreyGpuBuffer *current(void *buffer) {
  OspreyGpuBuffer *b = (OspreyGpuBuffer *)buffer;
  if (synchronize(b) < 0) return NULL;
  return b;
}

// The current words themselves, for the host-backend operations in gpu_host.c
// that walk whole buffers (the dense product, the file loaders) and read-outs;
// NULL for an empty buffer or failed synchronization. Read-status gates must
// precede non-empty access; failed readback retains length and stale state for
// retry. Only a buffer no kernel has used yet is written through this pointer.
void *osprey_gpu_data(void *buffer) {
  return osp_gpu_stored(current(buffer));
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
    osprey_gpu_finalize(b);
  }
}

// [GPU-IOTA] Every element its own index, in one pass: an index grid of millions
// of cells (a prompt chunk's attention scores) costs one synchronize and one
// finalize, not one of each per element. Stores the low elem_bytes of each index,
// as osprey_gpu_set does.
void osprey_gpu_iota_fill(void *buffer) {
  OspreyGpuBuffer *b = current(buffer);
  if (!b || !b->data) return;
  for (int64_t i = 0; i < b->length; i++) {
    memcpy(b->data + i * b->elem_bytes, &i, (size_t)b->elem_bytes);
  }
  osprey_gpu_finalize(b);
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
