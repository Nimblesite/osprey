// Channels [CONCURRENCY-CHANNEL] and select [CONCURRENCY-SELECT]: FIFO order
// and wraparound, refused capacities, handle guards, blocking in both
// directions, cleanup of unreceived and rejected managed values, and the
// handle and cleanup guards around running fibers. Part of the
// fiber_runtime_tests suite; see fiber_tests_shared.h.
#include "fiber_tests_shared.h"
#include "test_death.h"

#include <assert.h>
#include <pthread.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

// --- [CONCURRENCY-CHANNEL] -------------------------------------------------
// "Channel(capacity) creates a FIFO channel whose positive integer capacity is
// the number of buffered values"; send blocks while full, recv blocks while
// empty (docs/specs/language/0011-LightweightFibersAndConcurrency.md).

enum {
  CHANNEL_CAP = 2,
  CHANNEL_MESSAGES = 8,
  CHANNEL_FIRST_VALUE = 10,
  CHANNEL_HANDOFF_VALUE = 4242,
  CHANNEL_SETTLE_US = 50000,
  CHANNEL_WRAP_TAG = 1000,
};

static int64_t blocking_channel = 0;

// Sends more values than the buffer holds, so the runtime must park the sender
// on `not_full` until the consumer drains rather than drop or overwrite.
static int64_t channel_overfiller(void) {
  for (int64_t i = 0; i < CHANNEL_MESSAGES; i++) {
    if (channel_send(blocking_channel, CHANNEL_FIRST_VALUE + i, 0) != 1) {
      return -1;
    }
  }
  return CHANNEL_MESSAGES;
}

// Sends exactly once, late, so a recv issued first must park on `not_empty`.
static int64_t channel_late_sender(void) {
  usleep(CHANNEL_SETTLE_US);
  return channel_send(blocking_channel, CHANNEL_HANDOFF_VALUE, 0);
}

// A ring driven well past its capacity: head and tail both wrap, and the
// oldest value still leaves first on every round.
void test_channel_fifo_and_wraparound(void) {
  int64_t ch = channel_create(CHANNEL_CAP);
  int64_t other = channel_create(1);
  assert(ch > 0 && "a positive capacity yields a live handle");
  assert(other > 0 && "a capacity-1 channel is legal");
  assert(ch != other && "each channel gets a distinct handle");
  for (int64_t round = 0; round < CHANNEL_MESSAGES; round++) {
    assert(channel_send(ch, round, 0) == 1);
    assert(channel_send(ch, round + CHANNEL_WRAP_TAG, 0) == 1);
    assert(channel_recv(ch) == round && "FIFO: the oldest value leaves first");
    assert(channel_recv(ch) == round + CHANNEL_WRAP_TAG &&
           "the ring wraps without reordering");
  }
  // Two channels are two buffers, not one shared queue.
  assert(channel_send(other, 7, 0) == 1);
  assert(channel_send(ch, 8, 0) == 1);
  assert(channel_recv(other) == 7 && "handles do not alias");
  assert(channel_recv(ch) == 8 && "handles do not alias");
}

// "Zero and negative capacities are rejected; the runtime does not implement
// rendezvous channels." Oversized capacities are rejected for the same reason
// a wild index is: the buffer must be allocatable before a handle exists.
void test_channel_capacity_rejections(void) {
  assert(channel_create(0) == -1 && "zero capacity is not a rendezvous channel");
  assert(channel_create(-1) == -1 && "negative capacity is rejected");
  assert(channel_create(INT64_MIN) == -1 && "the rejection has no underflow hole");
  assert(channel_create((int64_t)INT32_MAX + 1) == -1 &&
         "capacity above INT32_MAX is rejected, not truncated");
  assert(channel_create(INT64_MAX) == -1 && "the element count cannot overflow");
}

// `Channel(capacity)` answers `Channel<T>`, not a Result, so the LANGUAGE has
// nowhere to put a refusal. `channel_create_checked` is that boundary: it turns
// every negative code into a death rather than a handle. Codegen calls it and
// only it, so a value the rest of the lowering treats as a channel is one this
// function let through. [CONCURRENCY-CHANNEL]
static void death_channel_of_zero_capacity(void) {
  (void)channel_create_checked(0);
}

static void death_channel_of_negative_capacity(void) {
  (void)channel_create_checked(-1);
}

static void death_channel_of_oversized_capacity(void) {
  (void)channel_create_checked(INT64_MAX);
}

// A capacity the compiler cannot see: codegen passes whatever the program computed.
static volatile int64_t runtime_capacity;

static void death_channel_of_runtime_capacity(void) {
  (void)channel_create_checked(runtime_capacity);
}

static void check_refusal(OspDeathBody body, const char *what) {
  int signalled = osp_death_signal(body);
  if (signalled != SIGABRT) {
    fprintf(stderr, "expected %s to be refused, got %d\n", what, signalled);
  }
  assert(signalled == SIGABRT);
}

void test_unusable_capacities_are_refused_not_returned(void) {
  check_refusal(death_channel_of_zero_capacity, "a zero capacity");
  check_refusal(death_channel_of_negative_capacity, "a negative capacity");
  check_refusal(death_channel_of_oversized_capacity, "an unallocatable capacity");
  const int64_t unusable[] = {0, -1, INT64_MIN, (int64_t)INT32_MAX + 1, INT64_MAX};
  for (size_t i = 0; i < sizeof(unusable) / sizeof(unusable[0]); i++) {
    runtime_capacity = unusable[i];
    check_refusal(death_channel_of_runtime_capacity, "an unusable runtime capacity");
  }
  // And the capacity that IS usable still is: a refusal that refused
  // everything would satisfy every assertion above.
  int64_t ok = channel_create_checked(1);
  assert(ok >= 1 && "the smallest positive capacity is a real handle");
  assert(channel_send(ok, 7, 0) == 1 && channel_recv(ok) == 7 &&
         "and it carries a value end to end");
}

// An id inside the array that names no channel is a failure, never a wild
// dereference; an id outside it is rejected before the array is touched.
void test_channel_handle_guards(void) {
  const int64_t unassigned = 999;
  assert(channel_send(unassigned, 1, 0) == 0 && "no channel at this id");
  assert(channel_recv(unassigned) == -1 && "no channel at this id");
  assert(channel_send(0, 1, 0) == 0 && "id 0 is below the first handle");
  assert(channel_recv(0) == -1 && "id 0 is below the first handle");
  assert(channel_send(-1, 1, 0) == 0 && "negative ids are rejected");
  assert(channel_recv(-1) == -1 && "negative ids are rejected");
  assert(channel_send(1000, 1, 0) == 0 && "the array bound is exclusive");
  assert(channel_recv(1000) == -1 && "the array bound is exclusive");
  assert(channel_send(INT64_MAX, 1, 0) == 0);
  assert(channel_recv(INT64_MIN) == -1);
}

// send blocks while the buffer is full and recv blocks while it is empty —
// the two halves of the [CONCURRENCY-CHANNEL] table, each observed from a
// real pthread-backed fiber.
void test_channel_blocks_both_directions(void) {
  assert(fiber_set_deterministic_mode(false) == 0);
  blocking_channel = channel_create(CHANNEL_CAP);
  assert(blocking_channel > 0);
  int64_t producer = fiber_spawn(channel_overfiller);
  assert(producer > 0);
  usleep(CHANNEL_SETTLE_US);
  assert(fiber_done(producer) == 0 &&
         "with CHANNEL_MESSAGES > CHANNEL_CAP the sender must still be parked");
  for (int64_t i = 0; i < CHANNEL_MESSAGES; i++) {
    assert(channel_recv(blocking_channel) == CHANNEL_FIRST_VALUE + i &&
           "a parked producer resumes in FIFO order, losing nothing");
  }
  assert(fiber_await(producer) == CHANNEL_MESSAGES &&
         "every send reported success");

  int64_t sender = fiber_spawn(channel_late_sender);
  assert(sender > 0);
  assert(channel_recv(blocking_channel) == CHANNEL_HANDOFF_VALUE &&
         "an empty buffer parks the receiver until a value arrives");
  assert(fiber_await(sender) == 1);
  assert(fiber_done(sender) == 1);
}

// In range but never handed out: the slot is NULL and both probes must say so
// instead of reading it as a fiber. [CONCURRENCY-SPAWN-AWAIT]
void test_unassigned_fiber_handles(void) {
  const int64_t unassigned[] = {INT32_MAX, INT64_C(1) << 40, INT64_MAX};
  for (unsigned i = 0; i < sizeof(unassigned) / sizeof(unassigned[0]); i++) {
    assert(fiber_await(unassigned[i]) == -1 &&
           "awaiting an unassigned handle is an error, not a read of NULL");
    assert(fiber_done(unassigned[i]) == -1 &&
           "probing an unassigned handle is an error, not a read of NULL");
  }
}

// "It MUST NOT release a cached result while another fiber can still await
// it." A still-running fiber therefore makes cleanup a no-op, and the value it
// eventually produces survives.
void test_cleanup_defers_while_a_fiber_runs(void) {
  int64_t slow = fiber_spawn(slow_function);
  assert(slow > 0);
  assert(fiber_done(slow) == 0 && "the fiber must still be running");
  fiber_cleanup_results();
  assert(fiber_done(slow) == 0 && "cleanup must not have joined the fiber");
  assert(fiber_await(slow) == 999 && "the pending result survived cleanup");
  fiber_cleanup_results();
  assert(fiber_await(slow) == 999 &&
         "a released root still answers the cached value");
}

// A value sent and never received holds the reference `send` transferred to the
// channel: nothing else can hand it back, because the runtime's completed-result
// cleanup walks FIBERS, not channels. Teardown must release it, and must drain
// the buffer so a value it dropped can never reach a later receiver.
// [CONCURRENCY-CHANNEL]
void test_cleanup_drains_unreceived_values(void) {
  int64_t ch = channel_create(4);
  assert(ch > 0);
  assert(channel_send(ch, 111, 1) == 1);
  assert(channel_send(ch, 222, 1) == 1);
  channel_cleanup();
  // The buffer is empty now, so the next send is exactly what the next recv
  // sees — a dropped value that stayed queued would come out first.
  assert(channel_send(ch, 333, 0) == 1);
  assert(channel_recv(ch) == 333 &&
         "cleanup drops unreceived values rather than queueing them");
  // Idempotent, and the channel is still usable afterwards.
  channel_cleanup();
  channel_cleanup();
  assert(channel_send(ch, 444, 0) == 1);
  assert(channel_recv(ch) == 444);
  // A channel that never carried a managed value is untouched by cleanup.
  int64_t plain = channel_create(2);
  assert(plain > 0);
  assert(channel_send(plain, 55, 0) == 1);
  channel_cleanup();
  assert(channel_recv(plain) == 55 &&
         "cleanup must not drain a channel that owns no references");
}

// A send the runtime REJECTS still consumed the caller's transfer: ownership
// moved at the call, so the value is released again on every rejection path
// rather than left with a reference no receiver will ever adopt.
void test_rejected_managed_send_releases_its_value(void) {
  const int64_t unassigned = 998;
  assert(channel_send(unassigned, 1, 1) == 0 && "no channel at this id");
  assert(channel_send(0, 1, 1) == 0 && "id 0 is below the first handle");
  assert(channel_send(-1, 1, 1) == 0 && "negative ids are rejected");
  assert(channel_send(1000, 1, 1) == 0 && "the array bound is exclusive");
  assert(channel_send(INT64_MIN, 1, 1) == 0);
  // ...and a live channel still accepts the same managed shape.
  int64_t ch = channel_create(1);
  assert(ch > 0);
  assert(channel_send(ch, 77, 1) == 1);
  assert(channel_recv(ch) == 77);
}

static void *select_late_sender(void *channel) {
  usleep(20000);
  channel_send(*(int64_t *)channel, 77, 0);
  return NULL;
}

// [CONCURRENCY-SELECT]: choice, capacity, timeouts, invalid channels and wakeups.
void test_channel_select(void) {
  int64_t first = channel_create(1);
  int64_t second = channel_create(1);
  int64_t received = 0;
  int64_t recv_kinds[] = {0, 0};
  int64_t ids[] = {first, second};
  int64_t words[] = {0, 0};
  // Nothing ready: a zero wait answers `count` at once, and a bounded one expires.
  assert(channel_select(2, recv_kinds, ids, words, &received, 0) == 2);
  assert(channel_select(2, recv_kinds, ids, words, &received, 5) == 2);
  // Both ready: source order, and only the chosen arm's value leaves its channel.
  channel_send(first, 1, 0);
  channel_send(second, 2, 0);
  assert(channel_select(2, recv_kinds, ids, words, &received, -1) == 0 && received == 1);
  assert(channel_select(2, recv_kinds, ids, words, &received, -1) == 1 && received == 2);
  // A send arm performs only with room; the full channel falls to the next arm.
  channel_send(first, 3, 0);
  int64_t send_kinds[] = {1, 1};
  int64_t send_words[] = {30, 40};
  assert(channel_select(2, send_kinds, ids, send_words, &received, 0) == 1);
  assert(channel_select(2, send_kinds, ids, send_words, &received, 0) == 2);
  assert(channel_recv(first) == 3);
  assert(channel_recv(second) == 40);
  // Out-of-range and unknown channels are never ready, and never fault.
  int64_t invalid[] = {0, 999};
  assert(channel_select(2, recv_kinds, invalid, words, &received, 0) == 2);
  // A blocked select wakes for a send from another thread.
  pthread_t sender;
  assert(pthread_create(&sender, NULL, select_late_sender, &second) == 0);
  assert(channel_select(2, recv_kinds, ids, words, &received, -1) == 1 && received == 77);
  assert(pthread_join(sender, NULL) == 0);
  printf("  ok  channel select\n");
}
