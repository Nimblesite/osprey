// Adaptive heap locking, shared by the ARC and GC backends. The heap starts
// single-threaded, where the sole thread already serializes every memory op
// and the mutex is pure overhead (binarytrees alone runs ~19.6M locked
// allocs; pthread_mutex_* was ~17% of its wall). osp_mem_notify_multithreaded()
// (memory_hooks.h) trips g_heap_mt from the fiber/websocket/process runtimes
// *before* the pthread_create that first lets a second thread reach the heap.
// pthread_create is a full barrier, so that write happens-before the child's
// first op: no unlocked op can ever race a locked one. The flag is monotonic
// and only ever transitions single-threaded, so within one op heap_lock and
// heap_unlock always observe the same value. Each backend includes this once.
#ifndef OSPREY_HEAP_LOCK_H
#define OSPREY_HEAP_LOCK_H

#include <pthread.h>
#include <stdatomic.h>

static _Thread_local int g_lock_owned;
#ifndef __wasi__
static pthread_mutex_t g_lock = PTHREAD_MUTEX_INITIALIZER;
static atomic_int g_heap_mt = 0;

static inline void heap_lock(void) {
  if (atomic_load_explicit(&g_heap_mt, memory_order_acquire)) {
    pthread_mutex_lock(&g_lock);
    g_lock_owned = 1;
  }
}

static inline void heap_unlock(void) {
  if (atomic_load_explicit(&g_heap_mt, memory_order_acquire)) {
    g_lock_owned = 0;
    pthread_mutex_unlock(&g_lock);
  }
}
#else
// WASI preview1 has one execution thread; thread creation is rejected upstream.
static inline void heap_lock(void) {}
static inline void heap_unlock(void) {}
#endif

#endif
