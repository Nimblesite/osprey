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

int osp_native_length(OspNativeBridge *b, const td_OwnedLease *lease,
                      int64_t *length) {
  if (!length)
    return OSP_NATIVE_ERROR(InvalidRange);
  int status = native_lock(b);
  if (status)
    return status;
  size_t owner, chunk;
  status = native_lookup_locked(b, lease, &owner, &chunk);
  if (!status)
    *length = (int64_t)b->chunks[chunk].length;
  return native_unlock(b, status);
}

// Seals one taken run as a leaf chunk, in place. The storage is always
// consumed, so a failed call leaves none behind.
static int native_seal_run(OspNativeBridge *b, bool complete, int64_t run,
                           void *data, int64_t length, int64_t capacity,
                           td_OwnedLease *out) {
  if (!data && complete && (data = calloc(1, 8)))
    capacity = 8;
  if (data && complete && b)
    return osp_native_adopt(b, run + 1, data, length, capacity, out);
  free(data);
  return b ? OSP_NATIVE_ERROR(AllocationFailed) : OSP_NATIVE_ERROR(BridgeClosed);
}

// [NATIVE-SHM-SCENE] [NATIVE-SHM-VIEW] Every run is taken whatever happens.
int osp_native_frame_commit(OspNativeBridge *b, int64_t *revision) {
  void *data[OSP_NATIVE_RUNS];
  int64_t length[OSP_NATIVE_RUNS], capacity[OSP_NATIVE_RUNS];
  td_OwnedLease runs[OSP_NATIVE_RUNS], frame;
  int sealed[OSP_NATIVE_RUNS], status = 0;
  bool complete = osp_native_runs_take(data, length, capacity);
  for (int64_t run = 0; run < OSP_NATIVE_RUNS; run++) {
    sealed[run] = native_seal_run(b, complete, run, data[run], length[run],
                                  capacity[run], &runs[run]);
    if (sealed[run] && !status)
      status = sealed[run];
  }
  if (!status)
    status = osp_native_create(b, OSP_NATIVE_FRAME, false, NULL, 0, runs,
                               OSP_NATIVE_RUNS, &frame);
  for (int64_t run = 0; run < OSP_NATIVE_RUNS; run++)
    if (!sealed[run])
      (void)osp_native_release(b, &runs[run]);
  if (!status && (status = osp_native_publish(b, &frame, revision)))
    (void)osp_native_release(b, &frame);
  return status;
}

int osp_native_child(OspNativeBridge *b, const td_OwnedLease *lease,
                     int64_t index, td_OwnedLease *out) {
  if (!out)
    return OSP_NATIVE_ERROR(InvalidRange);
  size_t owner, chunk;
  int status = native_lock(b);
  if (status)
    return status;
  if (!(status = native_lookup_locked(b, lease, &owner, &chunk))) {
    NativeChunk *parent = &b->chunks[chunk];
    size_t *children = (size_t *)(void *)(parent->data + parent->capacity);
    if (index < 0 || (uint64_t)index >= parent->child_count)
      status = OSP_NATIVE_ERROR(InvalidRange);
    else if (!(status = native_issue_locked(b, children[index], out)))
      b->chunks[children[index]].refs++;
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
  if (NATIVE_DESTROY(&b->mutex))
    return OSP_NATIVE_ERROR(SynchronizationFailed);
  free(b->owners);
  free(b->chunks);
  free(b);
  return 0;
}
