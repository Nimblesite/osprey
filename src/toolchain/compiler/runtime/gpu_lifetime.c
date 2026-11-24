// [MEM-OWNERSHIP-STATIC] CPU-only programs can link typed drops without
// linking a device SDK. Only the GPU allocator supplies the destructor.
#include "gpu_buffer.h"
#include "memory_hooks.h"
#include <pthread.h>
#include <stdatomic.h>

static _Atomic(OspGpuDrop) gpu_drop;
static pthread_mutex_t bind_lock = PTHREAD_MUTEX_INITIALIZER;

void osp_gpu_bind_drop(OspGpuDrop drop) {
  if (atomic_load_explicit(&gpu_drop, memory_order_acquire)) return;
  pthread_mutex_lock(&bind_lock);
  if (!atomic_load_explicit(&gpu_drop, memory_order_relaxed)) {
    osp_mem_set_finalizer(drop); // GC and ARC run the external-header callback.
    atomic_store_explicit(&gpu_drop, drop, memory_order_release);
  }
  pthread_mutex_unlock(&bind_lock);
}

void osp_gpu_drop_header(void *body) {
  OspGpuDrop drop = atomic_load_explicit(&gpu_drop, memory_order_acquire);
  if (drop) drop(body);
}
