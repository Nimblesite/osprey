// File-backed GPU buffers [LLM-WEIGHTS-MAPPED]: `gpuMapFile` maps a region of
// a weights file read-only instead of copying it, and large files are mapped
// whole once so every region shares one device object. Targets without mmap
// (wasm32, Windows, the symbolic runtime) refuse with a typed reason.
#include "gpu_buffer.h"
#include "io_error.h"
#include "memory_hooks.h"

#include <stddef.h>
#include <stdint.h>
#if !defined(__wasm__) && !defined(_WIN32) && !defined(OSP_SYMBOLIC)
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>
#endif

#if !defined(__wasm__) && !defined(_WIN32) && !defined(OSP_SYMBOLIC)
// The reason a region cannot be mapped, or NULL when it can.
static const char *unmappable(int64_t offset, int64_t count, int64_t elem_bytes, int64_t file_bytes) {
  if (offset < 0) return "a negative offset";
  if (count < 0) return "a negative element count";
  if (elem_bytes != 1 && elem_bytes != 2 && elem_bytes != 4 && elem_bytes != 8) return "element width must be 1, 2, 4 or 8 bytes";
  if (offset % elem_bytes != 0) return "offset is not a multiple of the element width";
  if (offset > file_bytes || count > (file_bytes - offset) / elem_bytes) return "the file ends before the region does";
  return NULL;
}

// A weights file is mapped whole, once, and every region of it shares one device
// object: the driver wires each buffer a command buffer binds on every
// submission, and a model's 700 tensors as 700 objects cost seconds per token.
// The table holds a process's few large files and is never freed, so a region
// outlives any buffer; smaller files keep a mapping of their own, which
// releases with its buffer.
#define GPU_MAPPED_FILES 4
#define GPU_MAPPED_WHOLE ((off_t)1 << 30)
typedef struct {
  dev_t device;
  ino_t inode;
  off_t size;
  void *base;
  size_t bytes;
  void *slot;
} MappedFile;
static MappedFile mapped_files[GPU_MAPPED_FILES];
static pthread_mutex_t mapped_lock = PTHREAD_MUTEX_INITIALIZER;

static const MappedFile *mapped_whole(int fd, const struct stat *status) {
  if (status->st_size < GPU_MAPPED_WHOLE) return NULL;
  const MappedFile *found = NULL;
  (void)pthread_mutex_lock(&mapped_lock);
  for (unsigned i = 0; i < GPU_MAPPED_FILES && !found; i++) {
    MappedFile *entry = &mapped_files[i];
    if (entry->base && entry->device == status->st_dev && entry->inode == status->st_ino && entry->size == status->st_size) {
      found = entry;
    } else if (!entry->base) {
      size_t page = (size_t)sysconf(_SC_PAGESIZE);
      size_t bytes = ((size_t)status->st_size + page - 1) / page * page;
      void *base = mmap(NULL, bytes, PROT_READ, MAP_SHARED, fd, 0);
      if (base != MAP_FAILED) {
        *entry = (MappedFile){status->st_dev, status->st_ino, status->st_size, base, bytes, NULL};
        found = entry;
      }
      break;
    }
  }
  (void)pthread_mutex_unlock(&mapped_lock);
  return found;
}

// [LLM-WEIGHTS-MAPPED] `count` elements of `path` from byte `offset` as an
// immutable buffer over a read-only mapping: pages load on first touch and the
// file stays the only copy. Only input arguments can name it, so no kernel
// writes it. A file shortened while mapped is outside this contract.
void *osprey_gpu_map_file(const char *path, int64_t offset, int64_t count, int64_t elem_bytes) {
  static const char OP[] = "gpuMapFile";
  if (path == NULL) {
    osp_io_error_text(OP, NULL, "no path");
    return NULL;
  }
  struct stat status;
  int fd = open(path, O_RDONLY | O_CLOEXEC);
  if (fd < 0 || fstat(fd, &status) != 0) {
    osp_io_error_set(OP, path, errno);
    if (fd >= 0) (void)close(fd);
    return NULL;
  }
  const char *reason = S_ISREG(status.st_mode) ? unmappable(offset, count, elem_bytes, status.st_size) : "not a regular file";
  if (reason != NULL) {
    osp_io_error_text(OP, path, reason);
    (void)close(fd);
    return NULL;
  }
  void *b = osp_gpu_alloc_buffer(0, elem_bytes, count == 0 ? sizeof(OspreyGpuBuffer) : sizeof(OspreyMappedGpuBuffer));
  if (b == NULL || count == 0) {
    (void)close(fd);
    return b;
  }
  OspreyMappedGpuBuffer *mapping = (OspreyMappedGpuBuffer *)b;
  OspreyGpuBuffer *buffer = &mapping->buffer;
  const MappedFile *shared = mapped_whole(fd, &status);
  if (shared) {
    (void)close(fd);
    osp_release(buffer->data);
    buffer->data = (uint8_t *)shared->base + offset;
    mapping->map_base = shared->base;
    mapping->map_bytes = shared->bytes;
    buffer->map_slot = &((MappedFile *)(uintptr_t)shared)->slot;
    buffer->shared_mapping = 1;
    buffer->mapped = 1;
    buffer->length = count;
    return b;
  }
  int64_t page = sysconf(_SC_PAGESIZE);
  int64_t skip = offset % page;
  size_t mapped = (size_t)(((skip + count * elem_bytes - 1) / page + 1) * page);
  void *base = mmap(NULL, mapped, PROT_READ, MAP_SHARED, fd, (off_t)(offset - skip));
  int failure = errno;
  (void)close(fd);
  if (base == MAP_FAILED) {
    osp_release(b);
    osp_io_error_set(OP, path, failure);
    return NULL;
  }
  osp_release(buffer->data);
  buffer->data = (uint8_t *)base + skip;
  mapping->map_base = base;
  mapping->map_bytes = mapped;
  buffer->mapped = 1;
  buffer->length = count;
  return b;
}
#else
void *osprey_gpu_map_file(const char *path, int64_t offset, int64_t count, int64_t elem_bytes) {
  (void)offset;
  (void)count;
  (void)elem_bytes;
  osp_io_error_text("gpuMapFile", path, "memory-mapped files are not available on this target");
  return NULL;
}
#endif
