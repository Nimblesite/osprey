// Managed engine controls for [IOS-HOST-IMPORTS]; Apple archive ABI is tested
// separately.
#include "memory_hooks.h"
#include "native_bridge_internal.h"
#include "string_runtime.h"
#include "test_alloc.h"
#include <assert.h>
#include <math.h>
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

// The bridge's usage accounting as the host reads it: managed bytes in use plus
// the chunk and owner counts a test pins.
typedef struct {
  size_t bytes, chunks, owners;
} NativeUsage;

static NativeUsage native_usage(OspNativeBridge *b) {
  NativeUsage used = {0, 0, 0};
  assert(osp_native_usage(b, &used.bytes, &used.chunks, &used.owners) == 0);
  return used;
}

// The counts a bridge must report after the work under test: asserted, never
// assumed.
static void expect_usage(OspNativeBridge *b, size_t chunks, size_t owners) {
  NativeUsage used = native_usage(b);
  assert(used.chunks == chunks && used.owners == owners);
}

// Refused work changes no accounting: the bytes in use stay where they were.
static void expect_usage_since(OspNativeBridge *b, NativeUsage previous,
                               size_t chunks, size_t owners) {
  NativeUsage used = native_usage(b);
  assert(used.bytes == previous.bytes && used.chunks == chunks &&
         used.owners == owners);
}

// Release clears the owner token, so a second release of the same lease is a
// wrong-owner refusal, never a double free.
static void release_twice(OspNativeBridge *b, td_OwnedLease *lease) {
  assert(osp_native_release(b, lease) == 0);
  assert(osp_native_release(b, lease) == OSP_NATIVE_ERROR(InvalidOwner));
}

// A bridge with a lease still held refuses to close, and every call after that
// refusal reports it closing.
static void expect_close_blocked(OspNativeBridge *b) {
  assert(osp_native_close(b) == OSP_NATIVE_ERROR(WouldBlock));
}

// A lease's bytes read back in place, as the host sees them.
static void expect_view_text(OspNativeBridge *b, const td_OwnedLease *lease,
                             int64_t run, int64_t bytes,
                             const char *expected) {
  const void *view = NULL;
  assert(osp_native_view(b, lease, run, 0, bytes, &view) == 0 &&
         strcmp(view, expected) == 0);
}

// Releasing the last lease and destroying the bridge leaves nothing behind.
static void release_and_destroy(OspNativeBridge *b, td_OwnedLease *lease) {
  assert(osp_native_release(b, lease) == 0);
  assert(osp_native_destroy(b) == 0);
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
  release_twice(bridge, &original);
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
  expect_close_blocked(bridge);
  assert(osp_native_acquire(bridge, &parent, &revision) ==
         OSP_NATIVE_ERROR(BridgeClosing));
  assert(osp_native_view(bridge, &retained, 9, 0, 5, &view) == 0);
  assert(strcmp(view, "root") == 0);
  assert(osp_native_release(bridge, &retained) == 0);
  expect_usage(bridge, 0, 0);
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
  NativeUsage baseline = native_usage(b);
  assert(baseline.chunks == 1 && baseline.owners == 2);
  assert(osp_native_create(b, 1, false, "", 1, &a, 1, &output) ==
         OSP_NATIVE_ERROR(WouldBlock));
  expect_usage_since(b, baseline, 1, 2);
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
  expect_view_text(b, &root, 1, 9, "original");
  release_and_destroy(b, &root);
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
  expect_usage(b, 1, 1);
  expect_view_text(b, &pinned, 1, 5, "leaf");
  release_and_destroy(b, &pinned);
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

static void push_shapes(int64_t first, int64_t count) {
  for (int64_t i = first; i < first + count; i++)
    assert(osp_native_scene_shape(i % 2, i, (double)i, 0.5, 2, 3, 4, i * 7,
                                  -i, NULL) == 0);
}

// A new owner of one run of the published frame, which must be `expected`.
static void leased_run(OspNativeBridge *b, int64_t run, int64_t expected,
                       td_OwnedLease *lease) {
  int64_t revision;
  td_OwnedLease frame;
  assert(osp_native_acquire(b, &frame, &revision) == 0 && revision == expected);
  assert(osp_native_child(b, &frame, run, lease) == 0);
  assert(osp_native_child(b, &frame, OSP_NATIVE_RUNS, lease) ==
             OSP_NATIVE_ERROR(InvalidRange) &&
         osp_native_child(b, &frame, -1, lease) ==
             OSP_NATIVE_ERROR(InvalidRange) &&
         osp_native_child(b, &frame, run, NULL) ==
             OSP_NATIVE_ERROR(InvalidRange) &&
         osp_native_child(b, lease, 0, &frame) ==
             OSP_NATIVE_ERROR(InvalidRange));
  assert(osp_native_release(b, &frame) == 0);
}

static const void *run_bytes(OspNativeBridge *b, const td_OwnedLease *lease,
                             int64_t run, int64_t bytes) {
  int64_t length;
  const void *view = NULL;
  assert(osp_native_length(b, lease, &length) == 0 && length == bytes);
  assert(osp_native_view(b, lease, 11, 0, length, &view) ==
             OSP_NATIVE_ERROR(InvalidLayout) &&
         !view);
  assert(osp_native_view(b, lease, run + 1, 0, length + 1, &view) ==
         OSP_NATIVE_ERROR(InvalidRange));
  assert(osp_native_view(b, lease, run + 1, 0, length, &view) == 0);
  return view;
}

static const OspNativeShape *leased_shapes(OspNativeBridge *b,
                                           td_OwnedLease *lease,
                                           int64_t expected, int64_t count) {
  leased_run(b, OSP_NATIVE_SHAPES, expected, lease);
  return run_bytes(b, lease, OSP_NATIVE_SHAPES,
                   count * (int64_t)sizeof(OspNativeShape));
}

static void check_shape_rows(const OspNativeShape *rows, int64_t first,
                             int64_t count) {
  for (int64_t i = 0; i < count; i++) {
    int64_t n = first + i;
    assert(rows[i].code == n % 2 && rows[i].index == n &&
           rows[i].values[0] == (double)n && rows[i].values[1] == 0.5 &&
           rows[i].values[2] == 2 && rows[i].values[3] == 3 &&
           rows[i].values[4] == 4 && rows[i].colour == n * 7 &&
           rows[i].depth == -n);
  }
}

// A committed frame's run and the text beside it, read in place.
typedef struct {
  const void *rows;
  const char *text;
  int64_t revision, text_bytes;
} PublishedRun;

// Commits the frame under test, leases its `run` and the text beside it at that
// revision, and reads both in place.
static PublishedRun commit_and_read(OspNativeBridge *b, int64_t run,
                                    int64_t rows_bytes, td_OwnedLease *rows,
                                    td_OwnedLease *text) {
  PublishedRun read = {NULL, NULL, 0, 0};
  assert(osp_native_frame_commit(b, &read.revision) == 0);
  leased_run(b, run, read.revision, rows);
  leased_run(b, OSP_NATIVE_TEXT, read.revision, text);
  assert(osp_native_length(b, text, &read.text_bytes) == 0);
  read.rows = run_bytes(b, rows, run, rows_bytes);
  read.text = run_bytes(b, text, OSP_NATIVE_TEXT, read.text_bytes);
  return read;
}

// The three view rows `push_view` builds, committed and read in place.
static PublishedRun commit_view_rows(OspNativeBridge *b, td_OwnedLease *rows,
                                     td_OwnedLease *text) {
  return commit_and_read(b, OSP_NATIVE_VIEWS,
                         3 * (int64_t)sizeof(OspNativeView), rows, text);
}

// [NATIVE-SHM-SCENE] A frame's shapes are sealed where they were built and
// read in place; a host-held frame survives its replacement.
static void check_native_scene_frames(void) {
  OspNativeBridge *b = native_fixture(1 << 20, 10, 8);
  int64_t revision;
  td_OwnedLease first, second, empty;
  assert(native_usage(b).chunks == 0);
  push_shapes(0, 3);
  assert(osp_native_frame_commit(b, &revision) == 0 && revision == 1);
  const OspNativeShape *held = leased_shapes(b, &first, 1, 3);
  check_shape_rows(held, 0, 3);
  push_shapes(100, 5000);
  assert(osp_native_frame_commit(b, &revision) == 0 && revision == 2);
  check_shape_rows(held, 0, 3);
  check_shape_rows(leased_shapes(b, &second, 2, 5000), 100, 5000);
  expect_usage(b, 6, 2);
  release_twice(b, &first);
  assert(native_usage(b).chunks == 5);
  assert(osp_native_frame_commit(b, &revision) == 0 && revision == 3);
  (void)leased_shapes(b, &empty, 3, 0);
  assert(osp_native_release(b, &second) == 0 &&
         osp_native_release(b, &empty) == 0);
  assert(osp_native_destroy(b) == 0);
}

// A run the budget or the row limit refuses is dropped whole: the published
// frame, the accounting and the next run are untouched.
static void check_native_scene_refusals(void) {
  OspNativeBridge *b = native_fixture(65536, 10, 8);
  int64_t revision = 0;
  td_OwnedLease lease;
  push_shapes(0, 2);
  assert(osp_native_frame_commit(b, &revision) == 0 && revision == 1);
  NativeUsage before = native_usage(b);
  push_shapes(0, 4096);
  assert(osp_native_frame_commit(b, &revision) ==
             OSP_NATIVE_ERROR(LimitExceeded) &&
         revision == 1);
  for (int64_t i = 0; i < OSP_NATIVE_SCENE_ROWS; i++)
    assert(osp_native_scene_shape(0, i, 0, 0, 0, 0, 0, 0, 0, NULL) == 0);
  assert(osp_native_scene_shape(0, 0, 0, 0, 0, 0, 0, 0, 0, NULL) != 0);
  assert(osp_native_scene_shape(0, 0, 0, 0, 0, 0, 0, 0, 0, NULL) != 0);
  assert(osp_native_frame_commit(b, &revision) ==
             OSP_NATIVE_ERROR(AllocationFailed) &&
         revision == 1);
  assert(osp_native_frame_commit(NULL, &revision) ==
         OSP_NATIVE_ERROR(BridgeClosed));
  expect_usage_since(b, before, 5, 0);
  check_shape_rows(leased_shapes(b, &lease, 1, 2), 0, 2);
  assert(osp_native_length(b, &lease, NULL) == OSP_NATIVE_ERROR(InvalidRange));
  assert(osp_native_release(b, &lease) == 0);
  push_shapes(7, 1);
  assert(osp_native_frame_commit(b, &revision) == 0 && revision == 2);
  check_shape_rows(leased_shapes(b, &lease, 2, 1), 7, 1);
  release_and_destroy(b, &lease);
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
  expect_close_blocked(b);
  assert(osp_native_retain(b, &a, &extra) == OSP_NATIVE_ERROR(BridgeClosing));
  assert(osp_native_publish(b, &a, &revision) ==
         OSP_NATIVE_ERROR(BridgeClosing));
  release_and_destroy(b, &a);
  b = native_fixture(16384, 1, 1);
  b->chunks[0].generation = INT64_MAX;
  assert(osp_native_create(b, 1, true, "", 1, NULL, 0, &a) ==
         OSP_NATIVE_ERROR(WouldBlock));
  assert(osp_native_destroy(b) == 0);
}

// A view of three controls: a column holding a label and a button.
static void push_view(void) {
  assert(osp_native_view_row(0, -1, -1, "", 1, 2, 3, 4, 5, 6, 7) == 0);
  assert(osp_native_view_row(2, 0, -1, "Count 3", 0xff, 0, 17, 0, 0, 0, 400) ==
         1);
  assert(osp_native_view_row(3, 0, 0, "A\xc3\xb1" "adir", 0, 0xff00, 15, 8, 0,
                             6, 600) == 2);
}

static void check_view_rows(const OspNativeView *rows, const char *text) {
  assert(rows[0].kind == 0 && rows[0].parent == -1 && rows[0].action == -1 &&
         rows[0].text_length == 0 && rows[0].colour == 1 &&
         rows[0].background == 2 && rows[0].size == 3 &&
         rows[0].padding == 4 && rows[0].spacing == 5 && rows[0].radius == 6 &&
         rows[0].weight == 7);
  assert(rows[1].kind == 2 && rows[1].parent == 0 && rows[1].action == -1 &&
         rows[1].text_offset == 0 && rows[1].text_length == 7 &&
         rows[1].colour == 0xff && rows[1].size == 17 &&
         rows[1].weight == 400);
  assert(rows[2].kind == 3 && rows[2].parent == 0 && rows[2].action == 0 &&
         rows[2].text_offset == 7 && rows[2].text_length == 7 &&
         rows[2].background == 0xff00 && rows[2].radius == 6);
  assert(memcmp(text, "Count 3A\xc3\xb1" "adir", 14) == 0);
}

// A refused frame publishes nothing: the revision stays where it was.
static void expect_refused_frame(OspNativeBridge *b, int64_t *revision,
                                 int64_t kept) {
  assert(osp_native_frame_commit(b, revision) ==
             OSP_NATIVE_ERROR(AllocationFailed) &&
         *revision == kept);
}

// A refused row is dropped whole: it is not published and the frame stays
// uncommitted.
static void expect_row_refused(OspNativeBridge *b, int64_t kind, int64_t parent,
                               const char *text, int64_t *revision) {
  assert(osp_native_view_row(kind, parent, -1, text, 0, 0, 0, 0, 0, 0, 0) ==
         -1);
  expect_refused_frame(b, revision, 1);
}

// Each refused view publishes nothing and leaves the next one clean.
static void check_view_refusals(OspNativeBridge *b) {
  int64_t revision = 1;
  assert(osp_native_view_row(0, 0, -1, "x", 0, 0, 0, 0, 0, 0, 0) == -1);
  assert(osp_native_view_row(0, -1, -1, "x", 0, 0, 0, 0, 0, 0, 0) == -1);
  expect_refused_frame(b, &revision, 1);
  expect_row_refused(b, 0, -2, NULL, &revision);
  for (int64_t i = 0; i < OSP_NATIVE_VIEW_LIMIT; i++)
    assert(osp_native_view_row(5, i - 1, -1, NULL, 0, 0, 0, 0, 0, 0, 0) == i);
  expect_row_refused(b, 5, 0, NULL, &revision);
}

// [NATIVE-SHM-VIEW] A view's controls and text are sealed where they were
// written and read in place beside the frame's shapes.
static void check_native_view_frames(void) {
  OspNativeBridge *b = native_fixture(1 << 20, 10, 8);
  int64_t revision;
  td_OwnedLease rows, text, shapes;
  push_view();
  push_shapes(4, 2);
  PublishedRun run = commit_view_rows(b, &rows, &text);
  assert(run.revision == 1);
  assert(run.text_bytes == 14);
  const OspNativeView *held = run.rows;
  check_view_rows(held, run.text);
  check_shape_rows(leased_shapes(b, &shapes, 1, 2), 4, 2);
  check_view_refusals(b);
  push_view();
  push_shapes(0, 9);
  osp_native_frame_discard();
  assert(osp_native_view_row(1, -1, 4, NULL, 0, 0, 0, 0, 0, 0, 0) == 0);
  assert(osp_native_frame_commit(b, &revision) == 0 && revision == 2);
  check_view_rows(held, run_bytes(b, &text, OSP_NATIVE_TEXT, 14));
  expect_usage(b, 8, 3);
  assert(osp_native_release(b, &rows) == 0 && osp_native_release(b, &text) == 0 &&
         osp_native_release(b, &shapes) == 0);
  leased_run(b, OSP_NATIVE_VIEWS, 2, &rows);
  held = run_bytes(b, &rows, OSP_NATIVE_VIEWS, sizeof(OspNativeView));
  assert(held[0].kind == 1 && held[0].action == 4 && held[0].text_length == 0);
  release_and_destroy(b, &rows);
}

// [NATIVE-SHM-SAFETY] Fibers are OS threads, so safe Osprey code can write
// rows from several at once: every row must still land whole, with its own
// text, and never corrupt the run.
enum { VIEW_WRITERS = 4, VIEW_WRITES = 2000 };
static void *view_writer(void *context) {
  char words[24];
  for (int64_t i = 0; i < VIEW_WRITES; i++) {
    int64_t action = (int64_t)(intptr_t)context * VIEW_WRITES + i;
    snprintf(words, sizeof words, "%lld", (long long)action);
    assert(osp_native_view_row(2, -1, action, words, 0, 0, 0, 0, 0, 0, 0) >= 0);
  }
  return NULL;
}
static void check_view_writers_from_many_threads(OspNativeBridge *b) {
  pthread_t writers[VIEW_WRITERS];
  int64_t count = VIEW_WRITERS * VIEW_WRITES;
  td_OwnedLease rows, text;
  char words[24];
  bool seen[VIEW_WRITERS * VIEW_WRITES] = {false};
  for (intptr_t i = 0; i < VIEW_WRITERS; i++)
    assert(pthread_create(&writers[i], NULL, view_writer, (void *)i) == 0);
  for (int i = 0; i < VIEW_WRITERS; i++)
    assert(pthread_join(writers[i], NULL) == 0);
  PublishedRun run = commit_and_read(
      b, OSP_NATIVE_VIEWS, count * (int64_t)sizeof(OspNativeView), &rows, &text);
  const OspNativeView *held = run.rows;
  const char *chars = run.text;
  for (int64_t i = 0; i < count; i++) {
    int length = snprintf(words, sizeof words, "%lld", (long long)held[i].action);
    assert(held[i].action >= 0 && held[i].action < count &&
           !seen[held[i].action] && held[i].text_length == length &&
           held[i].text_offset + length <= run.text_bytes &&
           memcmp(chars + held[i].text_offset, words, (size_t)length) == 0);
    seen[held[i].action] = true;
  }
  assert(osp_native_release(b, &rows) == 0 && osp_native_release(b, &text) == 0);
}

// [NATIVE-SHM-SAFETY] A host lays out what it reads, so a number no layout can
// use (not finite, or a negative extent) refuses the frame at the writer.
static void check_unusable_numbers_are_refused(OspNativeBridge *b) {
  const double bad[] = {NAN, INFINITY, -INFINITY, -1};
  int64_t revision = 7;
  for (int field = 0; field < 4; field++)
    for (int i = 0; i < 4; i++) {
      double v[4] = {1, 1, 1, 1};
      v[field] = bad[i];
      assert(osp_native_view_row(2, -1, -1, "x", 0, 0, v[0], v[1], v[2], v[3],
                                 400) == -1);
      assert(osp_native_frame_commit(b, &revision) ==
                 OSP_NATIVE_ERROR(AllocationFailed) &&
             revision == 7);
    }
  for (int field = 0; field < 5; field++)
    for (int i = 0; i < 3; i++) {
      double v[5] = {1, 1, 1, 1, 1};
      v[field] = bad[i];
      assert(osp_native_scene_shape(0, 0, v[0], v[1], v[2], v[3], v[4], 0, 0,
                                    "x") ==
             1);
      assert(osp_native_frame_commit(b, &revision) ==
                 OSP_NATIVE_ERROR(AllocationFailed) &&
             revision == 7);
    }
  assert(osp_native_scene_shape(0, 0, -5, -6, 1, 1, -3, 0, 0, NULL) == 0);
  osp_native_frame_discard();
}

// [NATIVE-SHM-VIEW] A control's extents, flags, border and second text land on
// its own row; a row that does not exist, or an unusable number, refuses.
static void check_view_extension(OspNativeBridge *b) {
  int64_t revision = 0;
  td_OwnedLease rows, text;
  push_view();
  assert(osp_native_view_extend(2, 90, 44, 0, 4, 0, "Old") == 0 &&
         osp_native_view_extend(2, 120, 0, 1, 1, 0x112233ff, "Name") == 0 &&
         osp_native_view_extend(0, 0, 0, 2, 0, 0, NULL) == 0);
  PublishedRun run = commit_view_rows(b, &rows, &text);
  assert(run.text_bytes == 21);
  const OspNativeView *held = run.rows;
  const char *chars = run.text;
  check_view_rows(held, chars);
  assert(held[2].width == 120 && held[2].height == 44 && held[2].grow == 1 &&
         held[2].flags == 5 && held[2].border == 0x112233ff &&
         held[2].hint_length == 4 &&
         memcmp(chars + held[2].hint_offset, "Name", 4) == 0);
  assert(held[0].grow == 2 && held[0].hint_length == 0 && held[1].grow == 0 &&
         held[1].flags == 0 && held[1].hint_length == 0);
  assert(osp_native_release(b, &rows) == 0 && osp_native_release(b, &text) == 0);
  const double bad[] = {NAN, INFINITY, -1};
  for (int64_t row = -1; row < 2; row += 2)
    for (int field = 0; field < 3; field++)
      for (int i = 0; i < 3; i++) {
        double v[3] = {0, 0, 0};
        v[field] = row < 0 ? 0 : bad[i];
        push_view();
        assert(osp_native_view_extend(row < 0 ? (i ? 3 : -1) : row, v[0], v[1],
                                      v[2], 0, 0, "h") == -1);
        assert(osp_native_frame_commit(b, &revision) ==
               OSP_NATIVE_ERROR(AllocationFailed));
      }
}

// [NATIVE-SHM-ASK] An ask's fields land in order beside the view's text, a
// negative field refuses the whole frame, and the next frame starts clean.
static void check_ask_rows(OspNativeBridge *b) {
  int64_t revision = 0;
  td_OwnedLease asks, text;
  assert(osp_native_scene_shape(6, 2, 1, 2, 14, 700, 0, 255, 3, "Play") == 0);
  assert(osp_native_ask(0, 9, "store:1") == 0 &&
         osp_native_ask(1, 0, "notes.txt") == 0 &&
         osp_native_ask(2, 4096, NULL) == 0 && osp_native_ask(0, 100, "k") == 0 &&
         osp_native_ask(1, 0, "") == 0);
  PublishedRun run = commit_and_read(b, OSP_NATIVE_ASKS,
                                     5 * (int64_t)sizeof(OspNativeAsk), &asks,
                                     &text);
  assert(run.text_bytes == 21);
  td_OwnedLease letters;
  leased_run(b, OSP_NATIVE_SHAPES, run.revision, &letters);
  const OspNativeShape *shape =
      run_bytes(b, &letters, OSP_NATIVE_SHAPES, sizeof(OspNativeShape));
  const OspNativeAsk *held = run.rows;
  const char *chars = run.text;
  assert(shape->code == 6 && shape->index == 2 && shape->values[3] == 700 &&
         shape->text_offset == 0 && shape->text_length == 4 &&
         memcmp(chars, "Play", 4) == 0);
  assert(osp_native_release(b, &letters) == 0);
  assert(held[0].field == 0 && held[0].number == 9 && held[0].text_length == 7 &&
         memcmp(chars + held[0].text_offset, "store:1", 7) == 0);
  assert(held[1].field == 1 && held[1].text_length == 9 &&
         memcmp(chars + held[1].text_offset, "notes.txt", 9) == 0);
  assert(held[2].field == 2 && held[2].number == 4096 &&
         held[2].text_length == 0 && held[3].number == 100 &&
         held[4].text_length == 0);
  assert(osp_native_release(b, &asks) == 0 && osp_native_release(b, &text) == 0);
  assert(osp_native_ask(0, 1, "t") == 0 && osp_native_ask(-1, 0, "x") == 1 &&
         osp_native_ask(1, 0, "y") == 1);
  assert(osp_native_frame_commit(b, &revision) != 0);
  assert(osp_native_ask(0, 1, "fresh") == 0);
  assert(osp_native_frame_commit(b, &revision) == 0);
}

static void check_native_view_safety(void) {
  OspNativeBridge *b = native_fixture(1 << 22, 16, 16);
  check_unusable_numbers_are_refused(b);
  check_view_extension(b);
  check_ask_rows(b);
  check_view_writers_from_many_threads(b);
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

// Construction takes the same mutex: a closed, failing or faulted bridge
// refuses it. The over-limit length keeps the reservation refused, so a
// failed unlock leaves nothing held that the untrusted mutex cannot undo.
static int oversized_create(OspNativeBridge *b) {
  td_OwnedLease lease = {{0}};
  return osp_native_create(b, 1, true, "", INT64_MAX, NULL, 0, &lease);
}
static void check_native_create_sync_failure(void) {
  assert(oversized_create(NULL) == OSP_NATIVE_ERROR(BridgeClosed));
  for (int failed_unlock = 0; failed_unlock < 2; failed_unlock++) {
    OspNativeBridge *b = native_fixture(16384, 8, 8);
    fail_native_lock = !failed_unlock;
    fail_native_unlock = failed_unlock != 0;
    assert(oversized_create(b) == OSP_NATIVE_ERROR(SynchronizationFailed));
    fail_native_lock = false;
    fail_native_unlock = false;
    assert(oversized_create(b) == OSP_NATIVE_ERROR(SynchronizationFailed));
    atomic_store(&b->faulted, false);
    assert(oversized_create(b) == OSP_NATIVE_ERROR(LimitExceeded));
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
  check_native_scene_frames();
  check_native_scene_refusals();
  check_native_view_frames();
  check_native_view_safety();
#ifdef OSP_NATIVE_TEST
  check_native_sync_failure();
  check_native_create_sync_failure();
#endif
  osp_release(saved);
  saved = NULL;
#ifdef HOST_IMPORT_ARC
  assert(osp_arc_live_objects() == 0 && osp_arc_live_bytes() == 0);
#endif
  puts("[ok] managed host imports: 20000 distinct 8192-byte copies");
  return 0;
}
