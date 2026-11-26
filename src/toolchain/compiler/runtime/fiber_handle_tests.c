// How many fibers and channels a program can have, that finished fibers and
// dropped channels are reclaimed, and sequential mode's use of the runtime from inside a fiber, split
// from fiber_runtime_tests.c so each test TU stays under the repo size budget.
// [CONCURRENCY-HANDLES] [CONCURRENCY-FIBER-LIFETIME] [CONCURRENCY-CHANNEL-LIFETIME]
// [CONCURRENCY-DETERMINISTIC]
#include <assert.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <unistd.h>
#if defined(__APPLE__)
#include <malloc/malloc.h>
#elif defined(__GLIBC__)
#include <malloc.h>
#endif

#if defined(__SANITIZE_ADDRESS__)
#define OSP_TEST_ASAN
#elif defined(__has_feature)
#if __has_feature(address_sanitizer)
#define OSP_TEST_ASAN
#endif
#endif

#include "fiber_tests_shared.h"
#include "memory_hooks.h"
#include "memory_refused.h"
#include "test_alloc.h"
#include "test_death.h"

enum { MANY = 5000, THREADS = 8, PER_THREAD = 600, THREAD_CAP_PROBE = 12000, HANDLE_BYTES_MAX = 512 };
// Rounds of MANY spawned, awaited and dropped fibers, and what each may leave
// behind on average: a few conservative stack words can keep a handle alive.
enum { RECLAIM_ROUNDS = 10, RECLAIMED_BYTES_MAX = 8, CHANNEL_SLOTS = 64 };


static int64_t doubled(void *word) { return (intptr_t)word * 2; }

static void *as_env(int64_t word) { return (void *)(intptr_t)word; }

// Far past the old limit of 999 for the life of the process, each answering its own
// value: exhaustion once answered -1 with exit 0, or a wild pointer for a string.
static void check_thousands_of_fibers(void) {
  void *previous = NULL;
  for (int64_t i = 0; i < MANY; i++) {
    void *fiber = spawn_fiber(doubled, as_env(i));
    assert(fiber != NULL && fiber != previous);
    previous = fiber;
    assert(fiber_await(fiber) == i * 2);
    assert(fiber_await(fiber) == i * 2);
    assert(fiber_done(fiber) == 1);
  }
}

static void check_thousands_of_channels(void) {
  void *channels[MANY];
  for (int64_t i = 0; i < MANY; i++) {
    channels[i] = channel_create(1);
    assert(channels[i] != NULL && channel_send(channels[i], i, 0) == 1);
  }
  for (int64_t i = MANY - 1; i >= 0; i--) { // every one still its own, however old
    assert(channel_recv(channels[i]) == i);
  }
}

// Bytes the C allocator has handed out and not got back. 0 where there is no exact
// counter: another libc, or AddressSanitizer, which replaces the allocator.
static size_t heap_in_use(void) {
#if defined(OSP_TEST_ASAN)
  return 0;
#elif defined(__APPLE__)
  malloc_statistics_t stats;
  malloc_zone_statistics(NULL, &stats);
  return stats.size_in_use;
#elif defined(__GLIBC__)
  return mallinfo2().uordblks;
#else
  return 0;
#endif
}

static size_t bytes_per_handle(void (*make_many)(void)) {
  size_t before = heap_in_use();
  make_many();
  size_t after = heap_in_use();
  return after > before ? (after - before) / MANY : 0;
}

// What a handle costs while the program holds it, measured: the spec's "a few
// hundred bytes" is this ceiling. The channels are still held, so theirs is whole;
// the fibers were dropped, so theirs is at most what is still to be reclaimed.
static void check_what_a_handle_costs(void) {
  size_t fiber = bytes_per_handle(check_thousands_of_fibers);
  size_t channel = bytes_per_handle(check_thousands_of_channels);
  printf("  %zu bytes per fiber, %zu per channel of one word\n", fiber, channel);
  if (heap_in_use() == 0) {
    return;
  }
  assert(fiber <= HANDLE_BYTES_MAX);
  assert(channel > 0 && channel <= HANDLE_BYTES_MAX);
}

// MANY fibers, each spawned, awaited twice and dropped.
static void spawn_await_and_drop(void) {
  for (int64_t i = 0; i < MANY; i++) {
    void *fiber = spawn_fiber(doubled, as_env(i));
    assert(fiber_await(fiber) == i * 2 && fiber_await(fiber) == i * 2);
  }
  osp_collect();
  fiber_cleanup_results();
}

// MANY channels of CHANNEL_SLOTS words, each used and dropped, half with a
// value still buffered.
static void make_use_and_drop_channels(void) {
  for (int64_t i = 0; i < MANY; i++) {
    void *channel = channel_create(CHANNEL_SLOTS);
    assert(channel != NULL && channel_send(channel, i, 0) == 1);
    assert(channel_send(channel, i + 1, 0) == 1 && channel_recv(channel) == i);
    assert(i % 2 == 0 || channel_recv(channel) == i + 1);
  }
  osp_collect();
  fiber_cleanup_results();
}

// What `rounds` of `make_many` leave behind per object made, once warm.
static size_t kept_per_dropped(void (*make_many)(void)) {
  make_many(); // warm: the collector's pages, the allocator's caches
  size_t before = heap_in_use();
  for (int round = 0; round < RECLAIM_ROUNDS; round++) {
    make_many();
  }
  size_t after = heap_in_use();
  return before != 0 && after > before ? (after - before) / (RECLAIM_ROUNDS * MANY) : 0;
}

// [CONCURRENCY-CHANNEL-LIFETIME]: a channel nothing can name again is freed
// with its buffer and synchronisation, whatever it still buffered. Every
// channel stayed in the runtime's table for the life of the process, so this
// loop grew by a channel and its CHANNEL_SLOTS words per creation.
static void check_dropped_channels_are_reclaimed(void) {
  size_t per_channel = kept_per_dropped(make_use_and_drop_channels);
  printf("  %zu bytes kept per dropped channel\n", per_channel);
  assert(per_channel < RECLAIMED_BYTES_MAX);
}

// [CONCURRENCY-FIBER-LIFETIME] #359: a fiber nothing can await again is freed
// with its record, result and synchronisation. Every finished fiber stayed in
// the runtime's table for the life of the process, so this loop grew by a
// fiber record per spawn.
static void check_finished_fibers_are_reclaimed(void) {
  size_t per_fiber = kept_per_dropped(spawn_await_and_drop);
  printf("  %zu bytes kept per dropped fiber\n", per_fiber);
  assert(per_fiber < RECLAIMED_BYTES_MAX);
}

static void *gate_channel;
static void *report_channel;

static int64_t reports_after_the_gate(void *word) {
  int64_t value = channel_recv(gate_channel) + (intptr_t)word;
  assert(channel_send(report_channel, value, 0) == 1);
  return value;
}

// The holder that lets go first never decides when a fiber dies: a fiber whose
// every other hold ended while it ran keeps running, and its runner frees it.
// AddressSanitizer runs this suite, so a fiber freed under its runner fails here.
static void check_a_dropped_running_fiber_finishes(void) {
  gate_channel = channel_create(1);
  report_channel = channel_create(1);
  Fiber *held = fiber_spawn_env(reports_after_the_gate, as_env(1));
  void *handle = spawn_fiber(reports_after_the_gate, as_env(100));
  assert(held != NULL && handle != NULL && fiber_done(handle) == 0);
  fiber_release(held);
  handle = NULL;
  osp_collect();
  fiber_cleanup_results();
  assert(channel_send(gate_channel, 10, 0) == 1);
  int64_t first = channel_recv(report_channel);
  assert(channel_send(gate_channel, 20, 0) == 1);
  int64_t second = channel_recv(report_channel);
  assert(first + second == 131 && (first == 11 || first == 110));
  fiber_cleanup_results();
}

// Channels and fibers interleaved: each answers its own values.
static void check_fibers_and_channels_interleave(void) {
  for (int64_t i = 0; i < MANY / 5; i++) {
    void *channel = channel_create(1);
    void *fiber = spawn_fiber(doubled, as_env(i));
    assert(channel != NULL && fiber != NULL);
    assert(channel_send(channel, i, 0) == 1 && fiber_await(fiber) == i * 2);
    assert(channel_recv(channel) == i && fiber_done(fiber) == 1);
  }
  fiber_cleanup_results();
  assert(channel_create(0) == NULL && spawn_fiber(NULL, NULL) == NULL);
}

static void *spawn_from_a_thread(void *arg) {
  int64_t base = (intptr_t)arg;
  for (int64_t i = 0; i < PER_THREAD; i++) {
    void *fiber = spawn_fiber(doubled, as_env(base + i));
    void *channel = channel_create(1);
    assert(fiber != NULL && channel != NULL);
    assert(channel_send(channel, base + i, 0) == 1);
    assert(fiber_await(fiber) == (base + i) * 2);
    assert(channel_recv(channel) == base + i);
  }
  return NULL;
}

// Channels and fibers are made, used and dropped from many threads at once.
static void check_growth_under_contention(void) {
  pthread_t threads[THREADS];
  for (int64_t t = 0; t < THREADS; t++) {
    assert(pthread_create(&threads[t], NULL, spawn_from_a_thread, as_env(t * 100000)) == 0);
  }
  for (int64_t t = 0; t < THREADS; t++) {
    assert(pthread_join(threads[t], NULL) == 0);
  }
}

static atomic_bool hold_fibers;
static atomic_int ran_on_spawner;
static pthread_t spawner;

static int64_t held_until_released(void *word) {
  if (pthread_equal(pthread_self(), spawner)) {
    atomic_fetch_add(&ran_on_spawner, 1);
  } else {
    while (atomic_load(&hold_fibers)) {
      usleep(100000);
    }
  }
  return (intptr_t)word;
}

// When the system has no thread left to give, spawn still answers a real fiber: the
// body runs on the spawner, and every value comes back. Not every system runs out
// within the probe, so the values are the assertion, and the fallback is only counted.
static void check_spawn_past_the_thread_limit(void) {
  static void *fibers[THREAD_CAP_PROBE];
  spawner = pthread_self();
  atomic_store(&hold_fibers, true);
  for (int64_t i = 0; i < THREAD_CAP_PROBE; i++) {
    fibers[i] = spawn_fiber(held_until_released, as_env(i));
    assert(fibers[i] != NULL);
  }
  atomic_store(&hold_fibers, false);
  for (int64_t i = 0; i < THREAD_CAP_PROBE; i++) {
    assert(fiber_await(fibers[i]) == i);
  }
  printf("  %d of %d fibers ran on the spawner\n", atomic_load(&ran_on_spawner), THREAD_CAP_PROBE);
}

static int64_t sends_once(void *channel) {
  assert(channel_send(channel, 41, 0) == 1);
  return 1;
}

static int64_t receives_once(void *channel) { return channel_recv(channel) + 1; }

static int64_t selects_a_ready_arm(void *channel) {
  int64_t kinds[] = {0};
  void *ids[] = {channel};
  int64_t words[] = {0};
  int64_t received = 0;
  assert(channel_select(1, kinds, ids, words, &received, -1) == 0);
  return received;
}

static int64_t awaits_its_argument(void *fiber) { return fiber_await(fiber) + 1; }

static int64_t spawns_and_awaits(void *word) {
  void *inner = spawn_fiber(doubled, word);
  return fiber_await(inner) + 1;
}

static void *self_handle;
static int64_t awaits_itself(void *ignored) {
  (void)ignored;
  return fiber_await(self_handle);
}

// Fibers in sequential mode use the runtime freely: this hung forever at the await,
// as it ran them holding the lock that send, recv and select take.
static void check_runtime_use_inside_a_queued_fiber(void) {
  void *channel = channel_create(1);
  void *sender = spawn_fiber(sends_once, channel);
  void *receiver = spawn_fiber(receives_once, channel);
  assert(fiber_await(receiver) == 42 && fiber_await(sender) == 1);
  assert(channel_send(channel, 7, 0) == 1);
  void *selector = spawn_fiber(selects_a_ready_arm, channel);
  assert(fiber_await(selector) == 7);
  void *spawner_fiber = spawn_fiber(spawns_and_awaits, as_env(5));
  assert(fiber_await(spawner_fiber) == 11);
}

// A blocked operation runs what is queued, as only that can unblock it.
static void check_blocked_operations_run_the_queue(void) {
  void *channel = channel_create(1);
  void *sender = spawn_fiber(sends_once, channel);
  assert(channel_recv(channel) == 41 && fiber_await(sender) == 1);
  assert(channel_send(channel, 1, 0) == 1);
  void *receiver = spawn_fiber(receives_once, channel);
  assert(channel_send(channel, 2, 0) == 1); // full: only the receiver makes room
  assert(fiber_await(receiver) == 2 && channel_recv(channel) == 2);
}

// Order, nesting, and the waits nothing on this stack could ever answer.
static void check_nested_and_impossible_awaits(void) {
  void *first = spawn_fiber(doubled, as_env(1));
  void *second = spawn_fiber(awaits_its_argument, first);
  void *third = spawn_fiber(awaits_its_argument, second);
  assert(fiber_await(third) == 4 && fiber_await(first) == 2 && fiber_await(second) == 3);
  self_handle = spawn_fiber(awaits_itself, NULL);
  assert(fiber_await(self_handle) == -1);
}

static void check_thousands_queued(void) {
  void *fibers[MANY];
  for (int64_t i = 0; i < MANY; i++) {
    fibers[i] = spawn_fiber(doubled, as_env(i));
    assert(fibers[i] != NULL && fiber_done(fibers[i]) == 1);
  }
  assert(fiber_await(fibers[MANY - 1]) == (MANY - 1) * 2);
  for (int64_t i = 0; i < MANY; i++) {
    assert(fiber_await(fibers[i]) == i * 2);
  }
}

static int64_t seven(void *unused) {
  (void)unused;
  return 7;
}

// The compiled `spawn` the runtime cannot allocate a fiber for, in a child.
static void refused_spawn(void) {
  osp_alloc_fail_next(); // the fiber record itself
  (void)fiber_spawn_env_owned(seven, NULL, 0);
}

// [MEM-REFUSED-STOP] #359: compiled `spawn` has no failure branch, so a fiber the
// runtime cannot allocate ends the run with the memory status, never a negative
// handle the program would carry on with. The runtime's own callers see NULL.
static void test_a_refused_compiled_spawn_stops_by_name(void) {
  assert(osp_death_exit(refused_spawn) == OSP_DEATH_EXITED + OSP_HEAP_EXHAUSTED_STATUS);
  osp_alloc_fail_next();
  assert(fiber_spawn_env(seven, NULL) == NULL);
  void *ran = fiber_spawn_env_owned(seven, NULL, 0);
  assert(ran != NULL && fiber_await(ran) == 7);
}

static void refuse_the_fiber_record(void) {
  assert(fiber_set_deterministic_mode(true) == 0); // queued: no thread allocates first
  osp_alloc_fail_next();
}

static void spawn_once(void) { (void)fiber_spawn_env_owned(seven, NULL, 0); }

// [MEM-REFUSED-STOP] A refused spawn names the bytes the refused allocation
// asked for: here the fiber record, the first thing a spawn allocates.
static void test_a_refused_spawn_names_the_bytes_refused(void) {
  char needle[96];
  int written = snprintf(needle, sizeof(needle),
                         "osprey: allocation refused: spawn request_bytes=%zu\n", fiber_record_bytes);
  assert(written > 0 && (size_t)written < sizeof(needle));
  assert(osp_child_refusal_names(refuse_the_fiber_record, spawn_once, needle));
}

void run_fiber_handle_tests(void) {
  test_a_refused_compiled_spawn_stops_by_name();
  test_a_refused_spawn_names_the_bytes_refused();
  check_what_a_handle_costs();
  check_finished_fibers_are_reclaimed();
  check_dropped_channels_are_reclaimed();
  check_a_dropped_running_fiber_finishes();
  check_fibers_and_channels_interleave();
  check_growth_under_contention();
  check_spawn_past_the_thread_limit();
  assert(fiber_set_deterministic_mode(true) == 0);
  check_runtime_use_inside_a_queued_fiber();
  check_blocked_operations_run_the_queue();
  check_nested_and_impossible_awaits();
  check_thousands_queued();
  assert(fiber_set_deterministic_mode(false) == 0);
  fiber_cleanup_results();
}
