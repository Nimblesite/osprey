// [NATIVE-SHM-OWNERSHIP] One allocator outside GC/ARC, no host refcount copies.
#include "native_bridge_internal.h"

static int native_configuration(int64_t bytes, int64_t chunks, int64_t owners,
                                size_t *base) {
  if (bytes <= 0 || chunks <= 0 || owners <= 0 || (uint64_t)bytes > SIZE_MAX ||
      (uint64_t)chunks > SIZE_MAX / sizeof(NativeChunk) ||
      (uint64_t)owners > SIZE_MAX / sizeof(NativeOwner))
    return OSP_NATIVE_ERROR(InvalidLimit);
  size_t a = (size_t)chunks * sizeof(NativeChunk),
         c = (size_t)owners * sizeof(NativeOwner);
  if (a > SIZE_MAX - sizeof(OspNativeBridge) ||
      c > SIZE_MAX - sizeof(OspNativeBridge) - a)
    return OSP_NATIVE_ERROR(InvalidLimit);
  *base = sizeof(OspNativeBridge) + a + c;
  return *base >= (size_t)bytes ? OSP_NATIVE_ERROR(LimitExceeded) : 0;
}

static void native_initialize(OspNativeBridge *b,
                              const td_ApplicationInstance *instance,
                              size_t base, int64_t bytes, int64_t chunks,
                              int64_t owners) {
  b->instance = *instance;
  b->byte_limit = (size_t)bytes;
  b->chunk_limit = (size_t)chunks;
  b->owner_limit = (size_t)owners;
  b->bytes = base;
  b->base_bytes = base;
  b->next_serial = 1;
  b->root = NATIVE_NO_SLOT;
  atomic_init(&b->faulted, false);
  for (size_t i = 0; i < b->chunk_limit; i++)
    b->chunks[i].next = i + 1;
  for (size_t i = 0; i < b->owner_limit; i++)
    b->owners[i].next = i + 1;
  b->chunks[b->chunk_limit - 1].next = NATIVE_NO_SLOT;
  b->owners[b->owner_limit - 1].next = NATIVE_NO_SLOT;
}

int osp_native_open(const td_ApplicationInstance *instance,
                    const char *fingerprint, int64_t bytes, int64_t chunks,
                    int64_t owners, OspNativeBridge **out) {
  uint16_t endian = 1;
  if (!instance || !out || !fingerprint ||
      strcmp(fingerprint, TD_SHARED_FINGERPRINT) != 0 ||
      sizeof(void *) != TD_SHARED_POINTER_BYTES ||
      *(unsigned char *)&endian != 1)
    return OSP_NATIVE_ERROR(InvalidLayout);
  size_t base = 0;
  int status = native_configuration(bytes, chunks, owners, &base);
  if (status)
    return status;
  OspNativeBridge *b = calloc(1, sizeof *b);
  if (!b)
    return OSP_NATIVE_ERROR(AllocationFailed);
  b->chunks = calloc((size_t)chunks, sizeof *b->chunks);
  b->owners = calloc((size_t)owners, sizeof *b->owners);
  status = (!b->chunks || !b->owners) ? OSP_NATIVE_ERROR(AllocationFailed) : 0;
  if (!status && NATIVE_INIT(&b->mutex))
    status = OSP_NATIVE_ERROR(SynchronizationFailed);
  if (status) {
    free(b->chunks);
    free(b->owners);
    free(b);
    return status;
  }
  native_initialize(b, instance, base, bytes, chunks, owners);
  *out = b;
  return 0;
}

int native_lookup_locked(OspNativeBridge *b, const td_OwnedLease *lease,
                         size_t *owner, size_t *chunk) {
  if (!lease)
    return OSP_NATIVE_ERROR(InvalidOwner);
  const unsigned char *token = lease->bytes + TD_OwnedLease_token_OFFSET;
  const unsigned char *ref = lease->bytes + TD_OwnedLease_reference_OFFSET;
  if (memcmp(token + TD_OwnerToken_appInstance_OFFSET, &b->instance,
             sizeof b->instance) ||
      memcmp(ref + TD_AllocationReference_appInstance_OFFSET, &b->instance,
             sizeof b->instance))
    return OSP_NATIVE_ERROR(WrongInstance);
  int64_t serial = native_integer(token, TD_OwnerToken_serial_OFFSET);
  if (serial <= 0)
    return OSP_NATIVE_ERROR(InvalidOwner);
  size_t slot = (size_t)((uint64_t)serial % b->owner_limit);
  if (b->owners[slot].serial != serial)
    return OSP_NATIVE_ERROR(InvalidOwner);
  size_t index = b->owners[slot].chunk;
  if (native_integer(ref, TD_AllocationReference_allocation_OFFSET) !=
          (int64_t)index + 1 ||
      native_integer(ref, TD_AllocationReference_generation_OFFSET) !=
          b->chunks[index].generation ||
      native_integer(ref, TD_AllocationReference_offset_OFFSET) != 0)
    return OSP_NATIVE_ERROR(StaleAllocation);
  *owner = slot;
  *chunk = index;
  return 0;
}

static void native_descriptor(OspNativeBridge *b, size_t chunk, int64_t serial,
                              td_OwnedLease *out) {
  memset(out, 0, sizeof *out);
  memcpy(out->bytes + TD_OwnedLease_token_OFFSET +
             TD_OwnerToken_appInstance_OFFSET,
         &b->instance, sizeof b->instance);
  native_put(out->bytes + TD_OwnedLease_token_OFFSET,
             TD_OwnerToken_serial_OFFSET, serial);
  unsigned char *ref = native_reference(out);
  memcpy(ref + TD_AllocationReference_appInstance_OFFSET, &b->instance,
         sizeof b->instance);
  native_put(ref, TD_AllocationReference_allocation_OFFSET, (int64_t)chunk + 1);
  native_put(ref, TD_AllocationReference_generation_OFFSET,
             b->chunks[chunk].generation);
}

int native_issue_locked(OspNativeBridge *b, size_t chunk, td_OwnedLease *out) {
  if (b->free_owner == NATIVE_NO_SLOT)
    return OSP_NATIVE_ERROR(WouldBlock);
  size_t slot = b->free_owner;
  uint64_t residue = (uint64_t)b->next_serial % b->owner_limit;
  uint64_t gap =
      slot >= residue ? slot - residue : b->owner_limit - residue + slot;
  if (gap >= (uint64_t)(INT64_MAX - b->next_serial))
    return OSP_NATIVE_ERROR(CounterExhausted);
  int64_t serial = b->next_serial + (int64_t)gap;
  b->next_serial = serial + 1;
  b->free_owner = b->owners[slot].next;
  b->owners[slot].serial = serial;
  b->owners[slot].chunk = chunk;
  b->owner_count++;
  native_descriptor(b, chunk, serial, out);
  return 0;
}

static int native_charge(int64_t length, bool leaf, size_t children,
                         size_t *capacity, size_t *charge) {
  if (length < 0 || (uint64_t)length > SIZE_MAX ||
      (!leaf && length > NATIVE_RECORD_MAX) || (leaf && children != 0))
    return OSP_NATIVE_ERROR(InvalidRange);
  size_t cap = 8;
  if ((uint64_t)length > NATIVE_RECORD_MAX)
    cap = (size_t)length;
  else
    while (cap < (size_t)length)
      cap *= 2;
  if (children > (SIZE_MAX - cap) / sizeof(size_t))
    return OSP_NATIVE_ERROR(InvalidRange);
  *capacity = cap;
  *charge = cap + children * sizeof(size_t);
  return 0;
}

static int native_reserve_locked(OspNativeBridge *b, size_t charge,
                                 size_t *slot) {
  if (b->closing)
    return OSP_NATIVE_ERROR(BridgeClosing);
  if (charge > b->byte_limit - b->bytes)
    return OSP_NATIVE_ERROR(LimitExceeded);
  while (b->free_chunk != NATIVE_NO_SLOT) {
    size_t index = b->free_chunk;
    NativeChunk *c = &b->chunks[index];
    b->free_chunk = c->next;
    if (c->generation == INT64_MAX)
      continue; // Exhausted slots are permanently retired.
    c->generation++;
    c->charge = charge;
    c->refs = 1;
    c->sealed = false;
    b->bytes += charge;
    b->chunk_count++;
    *slot = index;
    return 0;
  }
  return OSP_NATIVE_ERROR(WouldBlock);
}

static int native_children_locked(OspNativeBridge *b, NativeChunk *c,
                                  const td_OwnedLease *children, size_t count) {
  size_t *indices = count ? (size_t *)(void *)(c->data + c->capacity) : NULL;
  for (size_t i = 0; i < count; i++) {
    size_t owner, child;
    int status = native_lookup_locked(b, &children[i], &owner, &child);
    if (status)
      return status;
    if (!b->chunks[child].sealed || b->chunks[child].refs > UINT64_MAX - count)
      return OSP_NATIVE_ERROR(CounterExhausted);
    indices[i] = child;
  }
  for (size_t i = 0; i < count; i++)
    b->chunks[indices[i]].refs++;
  c->child_count = count;
  return 0;
}

static int native_attach(OspNativeBridge *b, size_t slot, unsigned char *data,
                         size_t capacity, const td_OwnedLease *children,
                         size_t count) {
  int status = native_lock(b);
  if (status) {
    free(data);
    return status;
  }
  NativeChunk *c = &b->chunks[slot];
  c->data = data;
  c->capacity = capacity;
  c->child_count = 0;
  status = !data ? OSP_NATIVE_ERROR(AllocationFailed)
                 : native_children_locked(b, c, children, count);
  return native_unlock(b, status);
}

static int native_finish(OspNativeBridge *b, size_t slot, int status,
                         td_OwnedLease *out) {
  if (status == OSP_NATIVE_ERROR(SynchronizationFailed))
    return status;
  int locked = native_lock(b);
  if (locked)
    return locked;
  if (!status && b->closing)
    status = OSP_NATIVE_ERROR(BridgeClosing);
  if (!status)
    status = native_issue_locked(b, slot, out);
  b->chunks[slot].sealed = status == 0;
  if (status)
    b->chunks[slot].refs = 0;
  int result = native_unlock(b, status);
  if (status) {
    int reclaimed = native_reclaim(b, slot);
    if (reclaimed)
      return reclaimed;
  }
  return result;
}

int osp_native_create(OspNativeBridge *b, int64_t type, bool leaf,
                      const void *data, int64_t length,
                      const td_OwnedLease *children, size_t count,
                      td_OwnedLease *out) {
  if (!out || (length != 0 && !data) || (count && !children) || type < 0)
    return OSP_NATIVE_ERROR(InvalidRange);
  size_t capacity, charge, slot;
  int status = native_charge(length, leaf, count, &capacity, &charge);
  if (status)
    return status;
  status = native_lock(b);
  if (status)
    return status;
  status = native_reserve_locked(b, charge, &slot);
  status = native_unlock(b, status);
  if (status)
    return status;
  unsigned char *storage =
      calloc(1, charge); // Reserved first; construction is outside the mutex.
  status = native_attach(b, slot, storage, capacity, children, count);
  if (!status) {
    if (length)
      memcpy(storage, data,
             (size_t)length); // Named managed/import construction copy.
    b->chunks[slot].length = (size_t)length;
    b->chunks[slot].type = type;
    b->chunks[slot].leaf = leaf;
  }
  return native_finish(b, slot, status, out);
}

int osp_native_adopt(OspNativeBridge *b, int64_t type, void *data,
                     int64_t length, int64_t capacity, td_OwnedLease *out) {
  size_t slot = 0;
  int status = !out || !data || type < 0 || length < 0 || capacity < 8 ||
                       length > capacity || (uint64_t)capacity > SIZE_MAX
                   ? OSP_NATIVE_ERROR(InvalidRange)
                   : native_lock(b);
  if (status) {
    free(data);
    return status;
  }
  status = native_unlock(b, native_reserve_locked(b, (size_t)capacity, &slot));
  if (status) {
    free(data);
    return status;
  }
  status = native_attach(b, slot, data, (size_t)capacity, NULL, 0);
  if (!status) {
    b->chunks[slot].length = (size_t)length;
    b->chunks[slot].type = type;
    b->chunks[slot].leaf = true;
  }
  return native_finish(b, slot, status, out);
}

int native_reclaim(OspNativeBridge *b, size_t first) {
  size_t pending = first;
  while (pending != NATIVE_NO_SLOT) {
    int status = native_lock(b);
    if (status)
      return status;
    size_t slot = pending;
    NativeChunk *c = &b->chunks[slot];
    pending = slot == first ? NATIVE_NO_SLOT : c->next;
    first = NATIVE_NO_SLOT;
    size_t *children =
        c->child_count ? (size_t *)(void *)(c->data + c->capacity) : NULL;
    for (size_t i = 0; i < c->child_count; i++) {
      NativeChunk *child = &b->chunks[children[i]];
      if (--child->refs == 0) {
        child->next = pending;
        pending = children[i];
      }
    }
    unsigned char *data = c->data;
    size_t charge = c->charge;
    c->data = NULL;
    c->child_count = 0;
    status = native_unlock(b, 0);
    if (status)
      return status;
    free(data); // Never free storage or run host code while holding metadata
                // lock.
    status = native_lock(b);
    if (status)
      return status;
    b->bytes -= charge;
    b->chunk_count--;
    c->next = b->free_chunk;
    b->free_chunk = slot;
    status = native_unlock(b, 0);
    if (status)
      return status;
  }
  return 0;
}
