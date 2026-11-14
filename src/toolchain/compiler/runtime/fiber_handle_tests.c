// How many fibers and channels a program can have, and sequential mode's use of the
// runtime from inside a fiber, split from fiber_runtime_tests.c so each test TU stays
// under the repo size budget. [CONCURRENCY-HANDLES] [CONCURRENCY-DETERMINISTIC]
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

enum { MANY = 5000, THREADS = 8, PER_THREAD = 600, THREAD_CAP_PROBE = 12000, HANDLE_BYTES_MAX = 512 };

static int64_t doubled(void *word) { return (intptr_t)word * 2; }

static void *as_env(int64_t word) { return (void *)(intptr_t)word; }

// Far past the old limit of 999 for the life of the process, each answering its own
// value: exhaustion once answered -1 with exit 0, or a wild pointer for a string.
static void check_thousands_of_fibers(void) {
  int64_t previous = 0;
  for (int64_t i = 0; i < MANY; i++) {
    int64_t fiber = fiber_spawn_env(doubled, as_env(i));
    assert(fiber > previous);
    previous = fiber;
    assert(fiber_await(fiber) == i * 2);
    assert(fiber_await(fiber) == i * 2);
    assert(fiber_done(fiber) == 1);
  }
}

static void check_thousands_of_channels(void) {
  int64_t channels[MANY];
  for (int64_t i = 0; i < MANY; i++) {
    channels[i] = channel_create(1);
    assert(channels[i] > 0 && channel_send(channels[i], i, 0) == 1);
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

// What a handle costs for the rest of the process, measured: the spec's "a few
// hundred bytes" is this ceiling. Both tables' slots are part of the figure.
static void check_what_a_handle_costs(void) {
  size_t fiber = bytes_per_handle(check_thousands_of_fibers);
  size_t channel = bytes_per_handle(check_thousands_of_channels);
  printf("  %zu bytes per fiber, %zu per channel of one word\n", fiber, channel);
  if (heap_in_use() == 0) {
    return;
  }
  assert(fiber > 0 && fiber <= HANDLE_BYTES_MAX);
  assert(channel > 0 && channel <= HANDLE_BYTES_MAX);
}

// One id space: alternating, neither table mistakes the other's handle for its own.
static void check_handles_are_one_space(void) {
  int64_t channel = 0;
  int64_t fiber = 0;
  for (int64_t i = 0; i < MANY / 5; i++) {
    channel = channel_create(1);
    fiber = fiber_spawn_env(doubled, as_env(i));
    assert(channel > 0 && fiber > 0 && channel != fiber);
    assert(fiber_await(channel) == -1 && fiber_done(channel) == -1);
    assert(channel_send(fiber, 1, 0) == 0 && channel_recv(fiber) == -1);
    assert(fiber_await(fiber) == i * 2);
  }
  fiber_cleanup_results();
  channel_cleanup();
  assert(channel_create(0) == -1 && fiber_spawn_env(NULL, NULL) == -1);
}

static void *spawn_from_a_thread(void *arg) {
  int64_t base = (intptr_t)arg;
  for (int64_t i = 0; i < PER_THREAD; i++) {
    int64_t fiber = fiber_spawn_env(doubled, as_env(base + i));
    int64_t channel = channel_create(1);
    assert(fiber > 0 && channel > 0);
    assert(channel_send(channel, base + i, 0) == 1);
    assert(fiber_await(fiber) == (base + i) * 2);
    assert(channel_recv(channel) == base + i);
  }
  return NULL;
}

// The tables move while other threads look things up in them.
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
  static int64_t fibers[THREAD_CAP_PROBE];
  spawner = pthread_self();
  atomic_store(&hold_fibers, true);
  for (int64_t i = 0; i < THREAD_CAP_PROBE; i++) {
    fibers[i] = fiber_spawn_env(held_until_released, as_env(i));
    assert(fibers[i] > 0);
  }
  atomic_store(&hold_fibers, false);
  for (int64_t i = 0; i < THREAD_CAP_PROBE; i++) {
    assert(fiber_await(fibers[i]) == i);
  }
  printf("  %d of %d fibers ran on the spawner\n", atomic_load(&ran_on_spawner), THREAD_CAP_PROBE);
}

static int64_t sends_once(void *channel) {
  assert(channel_send((intptr_t)channel, 41, 0) == 1);
  return 1;
}

static int64_t receives_once(void *channel) { return channel_recv((intptr_t)channel) + 1; }

static int64_t selects_a_ready_arm(void *channel) {
  int64_t kinds[] = {0};
  int64_t ids[] = {(intptr_t)channel};
  int64_t words[] = {0};
  int64_t received = 0;
  assert(channel_select(1, kinds, ids, words, &received, -1) == 0);
  return received;
}

static int64_t awaits_its_argument(void *fiber) { return fiber_await((intptr_t)fiber) + 1; }

static int64_t spawns_and_awaits(void *word) {
  int64_t inner = fiber_spawn_env(doubled, word);
  return fiber_await(inner) + 1;
}

static int64_t self_handle;
static int64_t awaits_itself(void *ignored) {
  (void)ignored;
  return fiber_await(self_handle);
}

// Fibers in sequential mode use the runtime freely: this hung forever at the await,
// as it ran them holding the lock that send, recv and select take.
static void check_runtime_use_inside_a_queued_fiber(void) {
  int64_t channel = channel_create(1);
  int64_t sender = fiber_spawn_env(sends_once, as_env(channel));
  int64_t receiver = fiber_spawn_env(receives_once, as_env(channel));
  assert(fiber_await(receiver) == 42 && fiber_await(sender) == 1);
  assert(channel_send(channel, 7, 0) == 1);
  int64_t selector = fiber_spawn_env(selects_a_ready_arm, as_env(channel));
  assert(fiber_await(selector) == 7);
  int64_t spawner_fiber = fiber_spawn_env(spawns_and_awaits, as_env(5));
  assert(fiber_await(spawner_fiber) == 11);
}

// A blocked operation runs what is queued, as only that can unblock it.
static void check_blocked_operations_run_the_queue(void) {
  int64_t channel = channel_create(1);
  int64_t sender = fiber_spawn_env(sends_once, as_env(channel));
  assert(channel_recv(channel) == 41 && fiber_await(sender) == 1);
  assert(channel_send(channel, 1, 0) == 1);
  int64_t receiver = fiber_spawn_env(receives_once, as_env(channel));
  assert(channel_send(channel, 2, 0) == 1); // full: only the receiver makes room
  assert(fiber_await(receiver) == 2 && channel_recv(channel) == 2);
}

// Order, nesting, and the waits nothing on this stack could ever answer.
static void check_nested_and_impossible_awaits(void) {
  int64_t first = fiber_spawn_env(doubled, as_env(1));
  int64_t second = fiber_spawn_env(awaits_its_argument, as_env(first));
  int64_t third = fiber_spawn_env(awaits_its_argument, as_env(second));
  assert(fiber_await(third) == 4 && fiber_await(first) == 2 && fiber_await(second) == 3);
  self_handle = fiber_spawn_env(awaits_itself, NULL);
  assert(fiber_await(self_handle) == -1);
}

static void check_thousands_queued(void) {
  int64_t fibers[MANY];
  for (int64_t i = 0; i < MANY; i++) {
    fibers[i] = fiber_spawn_env(doubled, as_env(i));
    assert(fibers[i] > 0 && fiber_done(fibers[i]) == 1);
  }
  assert(fiber_await(fibers[MANY - 1]) == (MANY - 1) * 2);
  for (int64_t i = 0; i < MANY; i++) {
    assert(fiber_await(fibers[i]) == i * 2);
  }
}

void run_fiber_handle_tests(void) {
  check_what_a_handle_costs();
  check_handles_are_one_space();
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
