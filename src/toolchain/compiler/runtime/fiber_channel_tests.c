// Channels [CONCURRENCY-CHANNEL] and select [CONCURRENCY-SELECT]: FIFO order
// and wraparound, refused capacities, handle guards, blocking in both
// directions, cleanup of unreceived and rejected managed values, and the
// handle and cleanup guards around running fibers. [CONCURRENCY-CHANNEL-LIFETIME] Part of the
// fiber_runtime_tests suite; see fiber_tests_shared.h.
#include "fiber_tests_shared.h"
#include "memory_hooks.h"
#include "test_death.h"

#include <assert.h>
#include <pthread.h>
#include <stdbool.h>
#include <stddef.h>
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
  CHANNEL_WRAP_TAG = 1000,
};

static void *blocking_channel;

// Parks on an empty channel until a test releases it: "still running" is
// then a fact about blocking, not about a timer racing a sleep body, which a
// loaded runner can outrun. fiber_runtime_tests.c's fiber_done case shares it.
void *gating_channel;

int64_t channel_gated_function(void *env) {
  (void)env;
  return channel_recv(gating_channel);
}

// Sends more values than the buffer holds, so the runtime must park the sender
// on `not_full` until the consumer drains rather than drop or overwrite.
static int64_t channel_overfiller(void *env) {
  (void)env;
  for (int64_t i = 0; i < CHANNEL_MESSAGES; i++) {
    if (channel_send(blocking_channel, CHANNEL_FIRST_VALUE + i, 0) != 1) {
      return -1;
    }
  }
  return CHANNEL_MESSAGES;
}

// fiber_runtime.c: how many fibers are parked on a channel right now.
int fiber_channel_waits(void);

// Waits until the runtime's channel-wait count reads `expected`: a fiber has
// then parked on, or left, its channel — a signal, never a timer a loaded
// runner can outrun. The deadline is a generous bound against a hang only.
// The verdict is the count the loop saw: a parked fiber that wakes and parks
// again (a spurious wakeup, a collection) dips it for an instant, so reading
// it a second time could catch that dip after the waiters had settled.
enum { WAIT_HANDSHAKE_TRIES = 30000, WAIT_HANDSHAKE_POLL_US = 1000 };

static void wait_for_channel_waits(int expected) {
  int seen = fiber_channel_waits();
  for (int i = 0; i < WAIT_HANDSHAKE_TRIES && seen != expected; i++) {
    usleep(WAIT_HANDSHAKE_POLL_US);
    seen = fiber_channel_waits();
  }
  assert(seen == expected && "the channel waiters never settled");
}

// Sends exactly once the receiving end has parked on `not_empty`, so a recv
// issued first always meets the blocking side of the handshake, whatever the
// scheduler does with either thread.
static int64_t channel_late_sender(void *env) {
  (void)env;
  wait_for_channel_waits(1);
  return channel_send(blocking_channel, CHANNEL_HANDOFF_VALUE, 0);
}

// A ring driven well past its capacity: head and tail both wrap, and the
// oldest value still leaves first on every round.
void test_channel_fifo_and_wraparound(void) {
  void *ch = channel_create(CHANNEL_CAP);
  void *other = channel_create(1);
  assert(ch != NULL && "a positive capacity yields a live handle");
  assert(other != NULL && "a capacity-1 channel is legal");
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
  assert(channel_create(0) == NULL && "zero capacity is not a rendezvous channel");
  assert(channel_create(-1) == NULL && "negative capacity is rejected");
  assert(channel_create(INT64_MIN) == NULL && "the rejection has no underflow hole");
  assert(channel_create((int64_t)INT32_MAX + 1) == NULL &&
         "capacity above INT32_MAX is rejected, not truncated");
  assert(channel_create(INT64_MAX) == NULL && "the element count cannot overflow");
}

// A handle that holds no channel, as one whose creation was refused does.
static void *empty_handle(void) {
  void **handle = osp_alloc_tagged(3 * sizeof(void *), OSP_MEM_CHANNEL_HANDLE);
  assert(handle != NULL);
  memset(handle, 0, 3 * sizeof(void *)); // the collector hands out unzeroed blocks
  return handle;
}

// No handle, and a handle that holds no channel, are failures, never a wild
// dereference, on every operation.
void test_channel_handle_guards(void) {
  void *empty = empty_handle();
  int64_t received = 0;
  int64_t kinds[] = {0, 1};
  int64_t words[] = {0, 9};
  void *none[] = {NULL, empty};
  assert(channel_send(NULL, 1, 0) == 0 && "no handle");
  assert(channel_recv(NULL) == -1 && "no handle");
  assert(channel_send(empty, 1, 0) == 0 && "a handle holding no channel");
  assert(channel_recv(empty) == -1 && "a handle holding no channel");
  assert(channel_select(2, kinds, none, words, &received, 0) == 2 && received == 0 &&
         "neither is ever ready");
  assert(channel_send(empty, 1, 0) == 0 && channel_recv(NULL) == -1 &&
         "and stays a failure however often it is asked");
}

// send blocks while the buffer is full and recv blocks while it is empty —
// the two halves of the [CONCURRENCY-CHANNEL] table, each observed from a
// real pthread-backed fiber.
void test_channel_blocks_both_directions(void) {
  assert(fiber_set_deterministic_mode(false) == 0);
  blocking_channel = channel_create(CHANNEL_CAP);
  assert(blocking_channel != NULL);
  void *producer = spawn_fiber(channel_overfiller, NULL);
  assert(producer != NULL);
  // The sender is parked on `not_full` (2 < 8 messages), so "still parked"
  // is a fact about blocking, not about a 50 ms head start.
  wait_for_channel_waits(1);
  assert(fiber_done(producer) == 0 &&
         "with CHANNEL_MESSAGES > CHANNEL_CAP the sender must still be parked");
  for (int64_t i = 0; i < CHANNEL_MESSAGES; i++) {
    assert(channel_recv(blocking_channel) == CHANNEL_FIRST_VALUE + i &&
           "a parked producer resumes in FIFO order, losing nothing");
  }
  assert(fiber_await(producer) == CHANNEL_MESSAGES &&
         "every send reported success");

  void *sender = spawn_fiber(channel_late_sender, NULL);
  assert(sender != NULL);
  assert(channel_recv(blocking_channel) == CHANNEL_HANDOFF_VALUE &&
         "an empty buffer parks the receiver until a value arrives");
  assert(fiber_await(sender) == 1);
  assert(fiber_done(sender) == 1);
}

// "It MUST NOT release a cached result while another fiber can still await
// it." A still-running fiber keeps its runner's hold through cleanup, and the
// value it eventually produces survives every later cleanup while held.
void test_cleanup_defers_while_a_fiber_runs(void) {
  gating_channel = channel_create(1);
  assert(gating_channel != NULL);
  void *slow = spawn_fiber(channel_gated_function, NULL);
  assert(slow != NULL);
  assert(fiber_done(slow) == 0 && "the fiber must still be running");
  fiber_cleanup_results();
  assert(fiber_done(slow) == 0 && "cleanup must not have joined the fiber");
  assert(channel_send(gating_channel, 999, 0) == 1 && "the fiber is released");
  assert(fiber_await(slow) == 999 && "the pending result survived cleanup");
  fiber_cleanup_results();
  assert(fiber_await(slow) == 999 &&
         "a released root still answers the cached value");
}

// A value sent and never received holds the reference `send` transferred to the
// channel, which owns it for as long as anything holds the channel: cleanup
// drops only what the program let go of, so a channel still held keeps every
// value, in order, through any number of cleanups. The values a dropped channel
// still buffered go with it (fiber_handle_tests.c).
// [CONCURRENCY-CHANNEL] [CONCURRENCY-CHANNEL-LIFETIME]
void test_cleanup_keeps_what_a_held_channel_buffers(void) {
  void *ch = channel_create(4);
  assert(ch != NULL);
  assert(channel_send(ch, 111, 1) == 1);
  assert(channel_send(ch, 222, 1) == 1);
  fiber_cleanup_results();
  assert(channel_send(ch, 333, 1) == 1);
  assert(channel_recv(ch) == 111 && "a held channel keeps its oldest value");
  // Idempotent, and the channel is still usable afterwards.
  fiber_cleanup_results();
  fiber_cleanup_results();
  assert(channel_send(ch, 444, 1) == 1);
  assert(channel_recv(ch) == 222 && channel_recv(ch) == 333 && channel_recv(ch) == 444);
  // The same for a channel that never carried a managed value.
  void *plain = channel_create(2);
  assert(plain != NULL);
  assert(channel_send(plain, 55, 0) == 1);
  fiber_cleanup_results();
  assert(channel_recv(plain) == 55 && "cleanup must not drain a channel still held");
}

// A send the runtime REJECTS still consumed the caller's transfer: ownership
// moved at the call, so the value is released again on every rejection path
// rather than left with a reference no receiver will ever adopt.
void test_rejected_managed_send_releases_its_value(void) {
  void *empty = empty_handle();
  assert(channel_send(NULL, 1, 1) == 0 && "no handle");
  assert(channel_send(empty, 1, 1) == 0 && "a handle holding no channel");
  assert(channel_send(empty, INT64_MIN, 1) == 0 && channel_send(NULL, INT64_MAX, 1) == 0);
  // ...and a live channel still accepts the same managed shape.
  void *ch = channel_create(1);
  assert(ch != NULL);
  assert(channel_send(ch, 77, 1) == 1);
  assert(channel_recv(ch) == 77);
}

// --- [CONCURRENCY-DEADLOCK] ---------------------------------------------------
// A fiber waiting on a channel nothing but waiting fibers can reach is blocked
// for ever, and a collection counts it, once. One whose channel the program
// can still name is never counted, and is woken as usual.

enum { WAITING_FIBERS = 40, SCRUBBED_WORDS = 4096 };
static void *named_channels[WAITING_FIBERS];

static int64_t receives_when_sent(void *channel) { return channel_recv(channel); }

// Spawns a fiber that waits on a fresh channel, in a frame of its own, so no
// word naming either handle outlives the call unless the channel is kept.
__attribute__((noinline)) static void spawn_waiting(size_t index, bool keep) {
  void *channel = channel_create(1);
  assert(channel != NULL && spawn_fiber(receives_when_sent, channel) != NULL);
  named_channels[index] = keep ? channel : NULL;
}

// Overwrites the stack the spawning frames used: a conservative collector reads
// a stale word there as a reference.
__attribute__((noinline)) static uintptr_t scrub_stack(void) {
  volatile uintptr_t words[SCRUBBED_WORDS];
  for (size_t i = 0; i < SCRUBBED_WORDS; i++) {
    words[i] = 0;
  }
  return words[SCRUBBED_WORDS / 2];
}

size_t osp_gc_collections(void); // the tracing collector this suite links

// Collects once every fiber has had time to block; a verdict needs a collection.
static size_t stuck_after_collecting(int channel_waits) {
  size_t collections = osp_gc_collections();
  wait_for_channel_waits(channel_waits);
  osp_collect();
  assert(osp_gc_collections() == collections + 1 && "the collection ran");
  return osp_mem_stuck_waits();
}

// Spawns WAITING_FIBERS waiting fibers, lets them block, and collects.
static size_t stuck_after_spawning(bool keep) {
  for (size_t i = 0; i < WAITING_FIBERS; i++) {
    spawn_waiting(i, keep);
  }
  (void)scrub_stack();
  return stuck_after_collecting(WAITING_FIBERS);
}

void test_fibers_blocked_for_ever_are_found(void) {
  assert(fiber_set_deterministic_mode(false) == 0);
  size_t before = osp_mem_stuck_waits();
  assert(stuck_after_spawning(true) == before && "a channel the program names can still wake its fiber");
  for (size_t i = 0; i < WAITING_FIBERS; i++) {
    assert(channel_send(named_channels[i], (int64_t)i, 0) == 1);
    named_channels[i] = NULL;
  }
  assert(stuck_after_collecting(0) == before && "a woken fiber was never stuck");
  size_t found = stuck_after_spawning(false) - before;
  printf("  %zu of %d fibers found blocked for ever\n", found, WAITING_FIBERS);
  // A stale stack word can still name a channel, so a few may go unfound;
  // none is ever found twice, and none that could be woken is found at all.
  assert(found >= WAITING_FIBERS * 3 / 4 && found <= WAITING_FIBERS);
  osp_collect();
  osp_collect();
  assert(osp_mem_stuck_waits() - before == found && "each is counted once");
  fiber_cleanup_results();
  assert(osp_mem_stuck_waits() - before >= found);
}

static void *select_late_sender(void *channel) {
  usleep(20000);
  channel_send(*(void **)channel, 77, 0);
  return NULL;
}

// [CONCURRENCY-SELECT]: choice, capacity, timeouts, invalid channels and wakeups.
void test_channel_select(void) {
  void *first = channel_create(1);
  void *second = channel_create(1);
  int64_t received = 0;
  int64_t recv_kinds[] = {0, 0};
  void *ids[] = {first, second};
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
  // No handle and a handle holding no channel are never ready, and never fault.
  void *invalid[] = {NULL, empty_handle()};
  assert(channel_select(2, recv_kinds, invalid, words, &received, 0) == 2);
  // A blocked select wakes for a send from another thread.
  pthread_t sender;
  assert(pthread_create(&sender, NULL, select_late_sender, &second) == 0);
  assert(channel_select(2, recv_kinds, ids, words, &received, -1) == 1 && received == 77);
  assert(pthread_join(sender, NULL) == 0);
  printf("  ok  channel select\n");
}
