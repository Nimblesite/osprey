// [MEM-OWNERSHIP-STATIC] CPU-only programs can link typed drops without
// linking a device SDK. Only the GPU allocator supplies the destructor.
#include "gpu_buffer.h"
#include "memory_hooks.h"
#include <stdatomic.h>

static _Atomic(OspGpuDrop) gpu_drop;

// Publishes the destructor after the backend's finalizer can run it.
static void bind_first(OspGpuDrop drop) {
  if (!atomic_load_explicit(&gpu_drop, memory_order_relaxed)) {
    osp_mem_set_finalizer(OSP_MEM_GPU_BUFFER, drop); // GC and ARC run the external-header callback.
    atomic_store_explicit(&gpu_drop, drop, memory_order_release);
  }
}

#ifdef __wasi__
// WASI preview1 runs one thread and links no pthreads. [WASM-TARGET]
void osp_gpu_bind_drop(OspGpuDrop drop) { bind_first(drop); }
#else
#include <pthread.h>
static pthread_mutex_t bind_lock = PTHREAD_MUTEX_INITIALIZER;

void osp_gpu_bind_drop(OspGpuDrop drop) {
  if (atomic_load_explicit(&gpu_drop, memory_order_acquire)) return;
  pthread_mutex_lock(&bind_lock);
  bind_first(drop);
  pthread_mutex_unlock(&bind_lock);
}
#endif

void osp_gpu_drop_header(void *body) {
  OspGpuDrop drop = atomic_load_explicit(&gpu_drop, memory_order_acquire);
  if (drop) drop(body);
}
