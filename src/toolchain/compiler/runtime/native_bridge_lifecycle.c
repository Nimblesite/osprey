// [NATIVE-SHM-PUBLISH] Root acquisition and ownership use the same mutex.
#include "native_bridge_internal.h"

int osp_native_retain(OspNativeBridge *b, const td_OwnedLease *lease,
                      td_OwnedLease *out) {
  if (!out)
    return OSP_NATIVE_ERROR(InvalidOwner);
  int status = native_lock(b);
  if (status)
    return status;
  size_t owner, chunk;
  status = native_lookup_locked(b, lease, &owner, &chunk);
  if (!status && b->closing)
    status = OSP_NATIVE_ERROR(BridgeClosing);
  if (!status && b->chunks[chunk].refs == UINT64_MAX)
    status = OSP_NATIVE_ERROR(CounterExhausted);
  if (!status)
    status = native_issue_locked(b, chunk, out);
  if (!status)
    b->chunks[chunk].refs++;
  return native_unlock(b, status);
}

int osp_native_release(OspNativeBridge *b, const td_OwnedLease *lease) {
  int status = native_lock(b);
  if (status)
    return status;
  size_t owner, chunk, dead = NATIVE_NO_SLOT;
  status = native_lookup_locked(b, lease, &owner, &chunk);
  if (!status) {
    native_owner_remove(b, owner);
    if (--b->chunks[chunk].refs == 0)
      dead = chunk;
  }
  status = native_unlock(b, status);
  return status || dead == NATIVE_NO_SLOT ? status : native_reclaim(b, dead);
}

int osp_native_view(OspNativeBridge *b, const td_OwnedLease *lease,
                    int64_t type, int64_t offset, int64_t length,
                    const void **out) {
  if (!out || offset < 0 || length < 0 || (uint64_t)offset > SIZE_MAX ||
      (uint64_t)length > SIZE_MAX)
    return OSP_NATIVE_ERROR(InvalidRange);
  int status = native_lock(b);
  if (status)
    return status;
  size_t owner, chunk;
  status = native_lookup_locked(b, lease, &owner, &chunk);
  if (!status) {
    NativeChunk *c = &b->chunks[chunk];
    if (c->type != type)
      status = OSP_NATIVE_ERROR(InvalidLayout);
    else if ((!c->leaf && (offset & 7)) || (size_t)offset > c->length ||
             (size_t)length > c->length - (size_t)offset)
      status = OSP_NATIVE_ERROR(InvalidRange);
    else
      *out = c->data + (size_t)offset;
  }
  return native_unlock(b, status);
}

int osp_native_publish(OspNativeBridge *b, const td_OwnedLease *candidate,
                       int64_t *revision) {
  if (!revision)
    return OSP_NATIVE_ERROR(InvalidRange);
  int status = native_lock(b);
  if (status)
    return status;
  size_t owner, chunk, dead = NATIVE_NO_SLOT;
  status = native_lookup_locked(b, candidate, &owner, &chunk);
  if (!status && b->closing)
    status = OSP_NATIVE_ERROR(BridgeClosing);
  if (!status && b->revision == INT64_MAX)
    status = OSP_NATIVE_ERROR(CounterExhausted);
  if (!status) {
    if (b->root != NATIVE_NO_SLOT && --b->chunks[b->root].refs == 0)
      dead = b->root;
    b->root = chunk;
    *revision = ++b->revision;
    native_owner_remove(
        b, owner); // Owner -> internal root, no allocation and no count change.
  }
  status = native_unlock(b, status);
  return status || dead == NATIVE_NO_SLOT ? status : native_reclaim(b, dead);
}

int osp_native_acquire(OspNativeBridge *b, td_OwnedLease *out,
                       int64_t *revision) {
  if (!out || !revision)
    return OSP_NATIVE_ERROR(InvalidRange);
  int status = native_lock(b);
  if (status)
    return status;
  if (b->closing)
    status = OSP_NATIVE_ERROR(BridgeClosing);
  else if (b->root == NATIVE_NO_SLOT)
    status = OSP_NATIVE_ERROR(WouldBlock);
  else if (b->chunks[b->root].refs == UINT64_MAX)
    status = OSP_NATIVE_ERROR(CounterExhausted);
  else {
    status = native_issue_locked(b, b->root, out);
    if (!status) {
      b->chunks[b->root].refs++;
      *revision = b->revision;
    }
  }
  return native_unlock(b, status);
}

int osp_native_usage(OspNativeBridge *b, size_t *bytes, size_t *chunks,
                     size_t *owners) {
  if (!bytes || !chunks || !owners)
    return OSP_NATIVE_ERROR(InvalidRange);
  int status = native_lock(b);
  if (status)
    return status;
  *bytes = b->bytes;
  *chunks = b->chunk_count;
  *owners = b->owner_count;
  return native_unlock(b, 0);
}

int osp_native_close(OspNativeBridge *b) {
  int status = native_lock(b);
  if (status)
    return status;
  size_t dead = NATIVE_NO_SLOT;
  b->closing = true;
  if (b->root != NATIVE_NO_SLOT && --b->chunks[b->root].refs == 0)
    dead = b->root;
  b->root = NATIVE_NO_SLOT;
  status = native_unlock(b, 0);
  if (status)
    return status;
  if (dead != NATIVE_NO_SLOT) {
    status = native_reclaim(b, dead);
    if (status)
      return status;
  }
  status = native_lock(b);
  if (status)
    return status;
  return native_unlock(b, b->chunk_count ? OSP_NATIVE_ERROR(WouldBlock) : 0);
}

int osp_native_destroy(OspNativeBridge *b) {
  int status = osp_native_close(b);
  if (status)
    return status;
  // Caller has joined every executor/adapter before destroying the mutex.
  if (pthread_mutex_destroy(&b->mutex))
    return OSP_NATIVE_ERROR(SynchronizationFailed);
  free(b->owners);
  free(b->chunks);
  free(b);
  return 0;
}
