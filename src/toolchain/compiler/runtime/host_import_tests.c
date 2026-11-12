// Managed engine controls for [IOS-HOST-IMPORTS]; Apple archive ABI is tested
// separately.
#include "memory_hooks.h"
#include "native_bridge_internal.h"
#include "string_runtime.h"
#include "test_alloc.h"
#include <assert.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef HOST_IMPORT_ARC
size_t osp_arc_live_objects(void);
size_t osp_arc_live_bytes(void);
static size_t baseline_objects;
static size_t baseline_bytes;
#else
void osp_collect(void);
size_t osp_gc_live_objects(void);
size_t osp_gc_collections(void);
#endif

static char *saved;
#ifdef OSP_NATIVE_TEST
static bool fail_native_lock, fail_native_unlock;
int osp_native_test_lock(pthread_mutex_t *mutex) {
  return fail_native_lock ? 1 : pthread_mutex_lock(mutex);
}
int osp_native_test_unlock(pthread_mutex_t *mutex) {
  int status = pthread_mutex_unlock(mutex);
  return fail_native_unlock ? 1 : status;
}
#endif
enum { PAYLOAD_BYTES = 8192, SOAK_ROUNDS = 20000 };

static void payload(char *text, int revision) {
  memset(text, 'x', PAYLOAD_BYTES);
  int prefix =
      snprintf(text, PAYLOAD_BYTES, "%d \xc3\xa9\xe2\x86\x92", revision);
  assert(prefix > 0 && prefix < PAYLOAD_BYTES);
  text[prefix] = 'x';
  text[PAYLOAD_BYTES] = '\0';
}

static void check_copy(int revision) {
  char text[PAYLOAD_BYTES + 1];
  char expected[PAYLOAD_BYTES + 1];
  payload(text, revision);
  memcpy(expected, text, sizeof text);
  char *copy = osp_host_string_copy(text);
  assert(copy != NULL && copy != text);
  memset(text, 'Z', PAYLOAD_BYTES);
  assert(memcmp(copy, expected, sizeof text) == 0);
  assert(strcmp(saved, "retained \xc3\xa9\xe2\x86\x92") == 0);
  osp_release(copy);
}

static void check_scalar_inputs(void) {
  assert(osp_host_string_copy(NULL) == NULL);
  char empty[] = "";
  char *copy = osp_host_string_copy(empty);
  assert(copy != NULL && copy != empty && copy[0] == '\0');
  osp_release(copy);
}

static void checkpoint(int revision) {
#ifdef HOST_IMPORT_ARC
  (void)revision;
  assert(osp_arc_live_objects() == baseline_objects);
  assert(osp_arc_live_bytes() == baseline_bytes);
#else
  if (revision % 256 == 0) {
    osp_collect();
    assert(osp_gc_live_objects() <= 16);
  }
#endif
}

static void check_soak(void) {
  for (int revision = 0; revision < SOAK_ROUNDS; revision++) {
    check_copy(revision);
    checkpoint(revision);
  }
#ifndef HOST_IMPORT_ARC
  assert(osp_gc_collections() > 0);
#endif
}

static void retained_seed(void) {
  saved = osp_host_string_copy("retained \xc3\xa9\xe2\x86\x92");
  assert(saved != NULL);
#ifdef HOST_IMPORT_ARC
  baseline_objects = osp_arc_live_objects();
  baseline_bytes = osp_arc_live_bytes();
  assert(baseline_objects == 1 && baseline_bytes > 0);
#endif
}

// [NATIVE-SHM-OWNERSHIP] Real managed allocations and the shared allocator live
// together: collecting/dropping managed values must not invalidate a host
// lease.
static void check_native_leases(void) {
  td_ApplicationInstance instance = {{1}};
  OspNativeBridge *bridge = NULL;
  assert(osp_native_open(&instance, "wrong", 65536, 32, 64, &bridge) ==
         OSP_NATIVE_ERROR(InvalidLayout));
  assert(bridge == NULL);
  assert(osp_native_open(&instance, TD_SHARED_FINGERPRINT, 65536, 32, 64,
                         &bridge) == 0);
  char *managed = osp_host_string_copy("snapshot survives");
  td_OwnedLease original, retained, parent;
  assert(osp_native_create(bridge, 7, true, managed, 18, NULL, 0, &original) ==
         0);
  osp_release(managed);
#ifndef HOST_IMPORT_ARC
  osp_collect();
#endif
  assert(osp_native_retain(bridge, &original, &retained) == 0);
  assert(memcmp(&original, &retained, sizeof original) != 0);
  const void *view = NULL, *other = NULL;
  assert(osp_native_view(bridge, &retained, 7, 0, 18, &view) == 0);
  assert(osp_native_view(bridge, &original, 7, 0, 18, &other) == 0 &&
         view == other);
  assert(strcmp(view, "snapshot survives") == 0);
  assert(osp_native_release(bridge, &original) == 0);
  assert(osp_native_release(bridge, &original) ==
         OSP_NATIVE_ERROR(InvalidOwner));
  assert(osp_native_view(bridge, &retained, 7, -1, 1, &other) ==
         OSP_NATIVE_ERROR(InvalidRange));
  assert(osp_native_view(bridge, &retained, 7, 8, INT64_MAX, &other) ==
         OSP_NATIVE_ERROR(InvalidRange));
  assert(osp_native_view(bridge, &retained, 8, 0, 18, &other) ==
         OSP_NATIVE_ERROR(InvalidLayout));
  td_OwnedLease forged = retained;
  forged.bytes[TD_OwnedLease_reference_OFFSET] ^= 1;
  assert(osp_native_release(bridge, &forged) ==
         OSP_NATIVE_ERROR(WrongInstance));
  assert(osp_native_create(bridge, 9, false, "root", 5, &retained, 1,
                           &parent) == 0);
  assert(osp_native_release(bridge, &retained) == 0);
  int64_t revision = -1;
  assert(osp_native_publish(bridge, &parent, &revision) == 0 && revision == 1);
  assert(osp_native_release(bridge, &parent) == OSP_NATIVE_ERROR(InvalidOwner));
  assert(osp_native_acquire(bridge, &retained, &revision) == 0 &&
         revision == 1);
  assert(osp_native_close(bridge) == OSP_NATIVE_ERROR(WouldBlock));
  assert(osp_native_acquire(bridge, &parent, &revision) ==
         OSP_NATIVE_ERROR(BridgeClosing));
  assert(osp_native_view(bridge, &retained, 9, 0, 5, &view) == 0);
  assert(strcmp(view, "root") == 0);
  assert(osp_native_release(bridge, &retained) == 0);
  size_t bytes, chunks, owners;
  assert(osp_native_usage(bridge, &bytes, &chunks, &owners) == 0 &&
         chunks == 0 && owners == 0);
  assert(osp_native_close(bridge) == 0);
  assert(osp_native_destroy(bridge) == 0);
}

static OspNativeBridge *native_fixture(int64_t bytes, int64_t chunks,
                                       int64_t owners) {
  td_ApplicationInstance instance = {{2}};
  OspNativeBridge *bridge = NULL;
  assert(osp_native_open(&instance, TD_SHARED_FINGERPRINT, bytes, chunks,
                         owners, &bridge) == 0);
  return bridge;
}

static void check_native_limits(void) {
  size_t no_bytes, no_chunks, no_owners;
  assert(osp_native_usage(NULL, &no_bytes, &no_chunks, &no_owners) ==
         OSP_NATIVE_ERROR(BridgeClosed));
  OspNativeBridge *b = native_fixture(16384, 4, 2);
  td_OwnedLease a, second, output = {{99}}, untouched = output;
  assert(osp_native_create(b, 1, true, "", 1, NULL, 0, &a) == 0);
  assert(osp_native_retain(b, &a, &second) == 0);
  assert(osp_native_retain(b, &a, &output) == OSP_NATIVE_ERROR(WouldBlock));
  assert(memcmp(&output, &untouched, sizeof output) == 0);
  size_t baseline, chunks, owners, used;
  assert(osp_native_usage(b, &baseline, &chunks, &owners) == 0 && chunks == 1 &&
         owners == 2);
  assert(osp_native_create(b, 1, false, "", 1, &a, 1, &output) ==
         OSP_NATIVE_ERROR(WouldBlock));
  assert(osp_native_usage(b, &used, &chunks, &owners) == 0 &&
         used == baseline && chunks == 1 && owners == 2);
  assert(osp_native_create(b, 1, true, "", INT64_MAX, NULL, 0, &output) ==
         OSP_NATIVE_ERROR(LimitExceeded));
  assert(osp_native_create(b, 1, false, "", 65537, NULL, 0, &output) ==
         OSP_NATIVE_ERROR(InvalidRange));
  assert(osp_native_create(b, 1, true, "", 1, &a, 1, &output) ==
         OSP_NATIVE_ERROR(InvalidRange));
  assert(osp_native_release(b, &a) == 0 && osp_native_release(b, &second) == 0);
  for (int i = 0; i < 1000; i++) {
    assert(osp_native_create(b, 1, true, NULL, 0, NULL, 0, &output) == 0);
    assert(osp_native_release(b, &a) == OSP_NATIVE_ERROR(InvalidOwner));
    assert(osp_native_release(b, &output) == 0);
  }
  assert(osp_native_destroy(b) == 0);
}

static void check_native_allocation_failure(void) {
  td_ApplicationInstance instance = {{3}};
  for (long stage = 0; stage < 3; stage++) {
    OspNativeBridge *b = NULL;
    long before = osp_alloc_live();
    osp_alloc_fail_after(stage);
    int status =
        osp_native_open(&instance, TD_SHARED_FINGERPRINT, 16384, 8, 8, &b);
    osp_alloc_fail_off();
    assert(status == OSP_NATIVE_ERROR(AllocationFailed) && b == NULL);
    assert(osp_alloc_live() == before);
  }
  OspNativeBridge *b = native_fixture(16384, 8, 8);
  td_OwnedLease root, output = {{42}}, untouched = output;
  int64_t revision;
  assert(osp_native_create(b, 1, true, "original", 9, NULL, 0, &root) == 0);
  assert(osp_native_publish(b, &root, &revision) == 0 && revision == 1);
  long before = osp_alloc_live();
  osp_alloc_fail_next();
  int status =
      osp_native_create(b, 1, true, "replacement", 12, NULL, 0, &output);
  osp_alloc_fail_off();
  assert(status == OSP_NATIVE_ERROR(AllocationFailed) &&
         osp_alloc_live() == before);
  assert(memcmp(&output, &untouched, sizeof output) == 0);
  assert(osp_native_acquire(b, &root, &revision) == 0 && revision == 1);
  const void *view;
  assert(osp_native_view(b, &root, 1, 0, 9, &view) == 0 &&
         strcmp(view, "original") == 0);
  assert(osp_native_release(b, &root) == 0 && osp_native_destroy(b) == 0);
}

static void check_native_dag(void) {
  OspNativeBridge *b = native_fixture(8388608, 20002, 8);
  td_OwnedLease child, next;
  assert(osp_native_create(b, 1, true, "leaf", 5, NULL, 0, &child) == 0);
  td_OwnedLease pinned;
  assert(osp_native_retain(b, &child, &pinned) == 0);
  for (int i = 0; i < 20000; i++) {
    td_OwnedLease edges[] = {child,
                             child}; // Shared DAG, two edges to the same child.
    assert(osp_native_create(b, 2, false, &i, sizeof i, edges, 2, &next) == 0);
    assert(osp_native_release(b, &child) == 0);
    child = next;
  }
  assert(osp_native_release(b, &child) == 0); // Must not recurse 20000 times.
  size_t bytes, chunks, owners;
  assert(osp_native_usage(b, &bytes, &chunks, &owners) == 0 && chunks == 1 &&
         owners == 1);
  const void *view;
  assert(osp_native_view(b, &pinned, 1, 0, 5, &view) == 0 &&
         strcmp(view, "leaf") == 0);
  assert(osp_native_release(b, &pinned) == 0 && osp_native_destroy(b) == 0);
}

static void *native_reader(void *context) {
  OspNativeBridge *b = context;
  for (int i = 0; i < 8000; i++) {
    td_OwnedLease lease;
    int64_t revision;
    assert(osp_native_acquire(b, &lease, &revision) == 0);
    const void *view;
    assert(osp_native_view(b, &lease, 11, 0, 24, &view) == 0);
    const int64_t *values = view;
    assert(values[0] == revision - 1 && values[1] == ~values[0] &&
           values[2] == values[0] * 3);
    assert(osp_native_release(b, &lease) == 0);
  }
  return NULL;
}

static void publish_native_frame(OspNativeBridge *b, int64_t frame) {
  int64_t values[] = {frame, ~frame, frame * 3};
  td_OwnedLease lease;
  assert(osp_native_create(b, 11, false, values, sizeof values, NULL, 0,
                           &lease) == 0);
  int64_t revision;
  assert(osp_native_publish(b, &lease, &revision) == 0 &&
         revision == frame + 1);
}

static void check_native_publication(void) {
  OspNativeBridge *b = native_fixture(65536, 16, 32);
  publish_native_frame(b, 0);
  pthread_t readers[3];
  for (size_t i = 0; i < 3; i++)
    assert(pthread_create(&readers[i], NULL, native_reader, b) == 0);
  for (int64_t frame = 1; frame <= 2000; frame++)
    publish_native_frame(b, frame);
  for (size_t i = 0; i < 3; i++)
    assert(pthread_join(readers[i], NULL) == 0);
  assert(osp_native_destroy(b) == 0);
}

// Isolation is necessary for unreachable-in-practice signed-64 exhaustion and
// OS mutex failure. These controls never exist in the shipping runtime API.
static void check_native_exhaustion(void) {
  OspNativeBridge *b = native_fixture(16384, 8, 8);
  td_OwnedLease a, extra, forged;
  int64_t revision;
  assert(osp_native_acquire(b, &a, &revision) == OSP_NATIVE_ERROR(WouldBlock));
  b->next_serial = (INT64_C(1) << 53) + 1;
  assert(osp_native_create(b, 1, true, "valid", 6, NULL, 0, &a) == 0);
  assert(osp_native_retain(b, &a, &extra) == 0);
  assert(native_integer(a.bytes, TD_OwnerToken_serial_OFFSET) >
         (INT64_C(1) << 53));
  assert(memcmp(a.bytes, extra.bytes, TD_OwnedLease_token_BYTES) != 0);
  assert(osp_native_release(b, &extra) == 0);
  forged = a;
  native_put(native_reference(&forged),
             TD_AllocationReference_generation_OFFSET, -1);
  assert(osp_native_release(b, &forged) == OSP_NATIVE_ERROR(StaleAllocation));
  assert(osp_native_create(b, 2, false, "", 1, &forged, 1, &extra) ==
         OSP_NATIVE_ERROR(StaleAllocation));
  assert(osp_native_retain(b, &a, NULL) == OSP_NATIVE_ERROR(InvalidOwner));
  b->next_serial = INT64_MAX;
  assert(osp_native_retain(b, &a, &extra) ==
         OSP_NATIVE_ERROR(CounterExhausted));
  b->revision = INT64_MAX;
  assert(osp_native_publish(b, &a, &revision) ==
         OSP_NATIVE_ERROR(CounterExhausted));
  assert(osp_native_close(b) == OSP_NATIVE_ERROR(WouldBlock));
  assert(osp_native_retain(b, &a, &extra) == OSP_NATIVE_ERROR(BridgeClosing));
  assert(osp_native_publish(b, &a, &revision) ==
         OSP_NATIVE_ERROR(BridgeClosing));
  assert(osp_native_release(b, &a) == 0 && osp_native_destroy(b) == 0);
  b = native_fixture(16384, 1, 1);
  b->chunks[0].generation = INT64_MAX;
  assert(osp_native_create(b, 1, true, "", 1, NULL, 0, &a) ==
         OSP_NATIVE_ERROR(WouldBlock));
  assert(osp_native_destroy(b) == 0);
}

#ifdef OSP_NATIVE_TEST
static void check_native_sync_failure(void) {
  for (int failed_unlock = 0; failed_unlock < 2; failed_unlock++) {
    OspNativeBridge *b = native_fixture(16384, 8, 8);
    size_t bytes, chunks, owners;
    fail_native_lock = !failed_unlock;
    fail_native_unlock = failed_unlock != 0;
    assert(osp_native_usage(b, &bytes, &chunks, &owners) ==
           OSP_NATIVE_ERROR(SynchronizationFailed));
    fail_native_lock = false;
    fail_native_unlock = false;
    assert(osp_native_usage(b, &bytes, &chunks, &owners) ==
           OSP_NATIVE_ERROR(SynchronizationFailed));
    assert(osp_native_acquire(b, NULL, NULL) == OSP_NATIVE_ERROR(InvalidRange));
    td_OwnedLease lease = {{0}};
    int64_t revision;
    const void *view;
    assert(osp_native_retain(b, &lease, &lease) ==
           OSP_NATIVE_ERROR(SynchronizationFailed));
    assert(osp_native_release(b, &lease) ==
           OSP_NATIVE_ERROR(SynchronizationFailed));
    assert(osp_native_view(b, &lease, 1, 0, 0, &view) ==
           OSP_NATIVE_ERROR(SynchronizationFailed));
    assert(osp_native_publish(b, &lease, &revision) ==
           OSP_NATIVE_ERROR(SynchronizationFailed));
    assert(osp_native_acquire(b, &lease, &revision) ==
           OSP_NATIVE_ERROR(SynchronizationFailed));
    assert(osp_native_destroy(b) == OSP_NATIVE_ERROR(SynchronizationFailed));
    // Test injector left the real mutex healthy; only the fixture may reset it.
    atomic_store(&b->faulted, false);
    assert(osp_native_destroy(b) == 0);
  }
}
#endif

int main(void) {
  assert(setenv("OSPREY_ARC_DEBUG", "1", 1) == 0);
  osp_mem_boot();
  check_scalar_inputs();
  retained_seed();
  check_soak();
  check_native_leases();
  check_native_limits();
  check_native_allocation_failure();
  check_native_dag();
  check_native_publication();
  check_native_exhaustion();
#ifdef OSP_NATIVE_TEST
  check_native_sync_failure();
#endif
  osp_release(saved);
  saved = NULL;
#ifdef HOST_IMPORT_ARC
  assert(osp_arc_live_objects() == 0 && osp_arc_live_bytes() == 0);
#endif
  puts("[ok] managed host imports: 20000 distinct 8192-byte copies");
  return 0;
}
