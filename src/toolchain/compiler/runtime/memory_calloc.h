#ifndef OSPREY_MEMORY_CALLOC_H
#define OSPREY_MEMORY_CALLOC_H

#include <stdint.h>
#include <string.h>

// [MEM-BACKENDS-SHARED] The backend allocates and registers the block.
// All three backends reject the same overflows and zero the requested bytes.
static inline void *managed_calloc(size_t count, size_t size, void *(*allocate)(size_t)) {
  if (count && size > SIZE_MAX / count) {
    return NULL;
  }
  size_t bytes = count * size;
  void *block = allocate(bytes);
  return block ? memset(block, 0, bytes) : NULL;
}

#endif
