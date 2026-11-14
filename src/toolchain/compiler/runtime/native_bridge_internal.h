// Private registry metadata, never a cross-language data model.
#ifndef OSPREY_NATIVE_BRIDGE_INTERNAL_H
#define OSPREY_NATIVE_BRIDGE_INTERNAL_H
#include "native_bridge.h"
#include <limits.h>
#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>

#define NATIVE_NO_SLOT SIZE_MAX
#define NATIVE_RECORD_MAX 65536u
// WASI Preview 1 without atomics has one executor and cannot link pthreads.
// Detect re-entry instead of reporting a successful second acquisition.
#if defined(__wasi__) && !defined(__wasm_atomics__)
typedef bool NativeMutex;
static inline int native_mutex_init(NativeMutex *mutex) {
  *mutex = false;
  return 0;
}
static inline int native_mutex_lock(NativeMutex *mutex) {
  if (*mutex)
    return 1;
  *mutex = true;
  return 0;
}
static inline int native_mutex_unlock(NativeMutex *mutex) {
  if (!*mutex)
    return 1;
  *mutex = false;
  return 0;
}
static inline int native_mutex_destroy(NativeMutex *mutex) {
  return *mutex ? 1 : 0;
}
#define NATIVE_INIT(mutex) native_mutex_init(mutex)
#define NATIVE_DESTROY native_mutex_destroy
#define NATIVE_LOCK native_mutex_lock
#define NATIVE_UNLOCK native_mutex_unlock
#else
#include <pthread.h>
typedef pthread_mutex_t NativeMutex;
#define NATIVE_INIT(mutex) pthread_mutex_init(mutex, NULL)
#define NATIVE_DESTROY pthread_mutex_destroy
#ifdef OSP_NATIVE_TEST
// Deterministic OS-error injection in the existing host-import integration
// suite. Production uses pthreads directly, with no configurable callbacks.
int osp_native_test_lock(pthread_mutex_t *mutex);
int osp_native_test_unlock(pthread_mutex_t *mutex);
#define NATIVE_LOCK osp_native_test_lock
#define NATIVE_UNLOCK osp_native_test_unlock
#else
#define NATIVE_LOCK pthread_mutex_lock
#define NATIVE_UNLOCK pthread_mutex_unlock
#endif
#endif
typedef struct {
  unsigned char *data;
  size_t length, capacity, charge, child_count, next;
  uint64_t refs;
  int64_t generation, type;
  bool sealed, leaf;
} NativeChunk;
typedef struct {
  int64_t serial;
  size_t chunk, next;
} NativeOwner;
struct OspNativeBridge {
  NativeMutex mutex;
  td_ApplicationInstance instance;
  NativeChunk *chunks;
  NativeOwner *owners;
  size_t chunk_limit, owner_limit, byte_limit, bytes, base_bytes;
  size_t free_chunk, free_owner, chunk_count, owner_count, root;
  int64_t next_serial, revision;
  bool closing;
  _Atomic bool faulted;
};

static inline int native_lock(OspNativeBridge *b) {
  if (!b)
    return OSP_NATIVE_ERROR(BridgeClosed);
  if (atomic_load(&b->faulted))
    return OSP_NATIVE_ERROR(SynchronizationFailed);
  if (NATIVE_LOCK(&b->mutex) == 0)
    return 0;
  atomic_store(&b->faulted, true);
  return OSP_NATIVE_ERROR(SynchronizationFailed);
}
static inline int native_unlock(OspNativeBridge *b, int result) {
  if (NATIVE_UNLOCK(&b->mutex) == 0)
    return result;
  atomic_store(&b->faulted, true);
  return OSP_NATIVE_ERROR(SynchronizationFailed);
}
static inline int64_t native_integer(const unsigned char *bytes,
                                     size_t offset) {
  int64_t result;
  memcpy(&result, bytes + offset, sizeof result);
  return result;
}
static inline void native_put(unsigned char *bytes, size_t offset,
                              int64_t value) {
  memcpy(bytes + offset, &value, sizeof value);
}
static inline unsigned char *native_reference(td_OwnedLease *lease) {
  return lease->bytes + TD_OwnedLease_reference_OFFSET;
}
static inline void native_owner_remove(OspNativeBridge *b, size_t slot) {
  b->owners[slot].serial = 0;
  b->owners[slot].next = b->free_owner;
  b->free_owner = slot;
  b->owner_count--;
}
// All *_locked helpers require this instance's sole metadata mutex.
int native_lookup_locked(OspNativeBridge *b, const td_OwnedLease *lease,
                         size_t *owner, size_t *chunk);
int native_issue_locked(OspNativeBridge *b, size_t chunk, td_OwnedLease *out);
int native_reclaim(OspNativeBridge *b, size_t first);
#endif
