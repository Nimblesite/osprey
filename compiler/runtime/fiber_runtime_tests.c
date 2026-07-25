// Exercises [CONCURRENCY-SPAWN-AWAIT], [CONCURRENCY-CHANNEL],
// [CONCURRENCY-YIELD], and the pthread transition in [MEM-FIBER-ISOLATION].
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <unistd.h>

extern int64_t fiber_spawn(int64_t (*fn)(void));
extern int64_t fiber_await(int64_t fiber_id);
extern int64_t fiber_done(int64_t fiber_id);
extern int64_t fiber_sleep(int64_t milliseconds);
extern int64_t channel_create(int64_t capacity);
extern int64_t channel_send(int64_t channel_id, int64_t value);
extern int64_t channel_recv(int64_t channel_id);

static int64_t test_function_1(void) { return 42; }

static int64_t test_function_2(void) { return 100; }

static int64_t slow_function(void) {
  usleep(100000); // 100ms
  return 999;
}

void test_null_function_pointer(void) {
  int64_t result = fiber_spawn(NULL);
  assert(result == -1 &&
         "fiber_spawn should return -1 for a null function pointer");
}

void test_invalid_fiber_await(void) {
  int64_t result = fiber_await(99999);
  assert(result == -1 &&
         "fiber_await should return -1 for an out-of-range fiber ID");

  result = fiber_await(-1);
  assert(result == -1 &&
         "fiber_await should return -1 for a negative fiber ID");
}

void test_valid_fiber_spawn(void) {
  int64_t fiber_id = fiber_spawn(test_function_1);
  assert(fiber_id > 0 &&
         "fiber_spawn should return a positive ID for a valid function");

  int64_t result = fiber_await(fiber_id);
  assert(result == 42 && "fiber should return its function's value");
}

void test_multiple_fibers(void) {
  int64_t fiber1 = fiber_spawn(test_function_1);
  int64_t fiber2 = fiber_spawn(test_function_2);

  assert(fiber1 > 0 && fiber2 > 0 && "both fibers should have valid IDs");
  assert(fiber1 != fiber2 && "fibers should have different IDs");

  int64_t result1 = fiber_await(fiber1);
  int64_t result2 = fiber_await(fiber2);

  assert(result1 == 42 && "first fiber should return 42");
  assert(result2 == 100 && "second fiber should return 100");
}

void test_fiber_bounds_checking(void) {
  int64_t result = channel_send(99999, 42);
  assert(result == 0 && "channel_send should reject an out-of-range ID");

  result = channel_recv(99999);
  assert(result == -1 && "channel_recv should reject an out-of-range ID");
}

// A zero-capacity buffer cannot rendezvous in this runtime: both send and recv
// would wait on an empty buffer. Reject it at construction instead of creating
// a channel that can only deadlock. [CONCURRENCY-CHANNEL]
void test_invalid_channel_capacity(void) {
  assert(channel_create(0) == -1 &&
         "zero-capacity channels must be rejected");
  assert(channel_create(-1) == -1 &&
         "negative-capacity channels must be rejected");
}

void test_fiber_sleep(void) {
  // [CONCURRENCY-SLEEP] Non-positive durations return immediately.
  assert(fiber_sleep(0) == 0);
  assert(fiber_sleep(-1) == 0);
  int64_t result = fiber_sleep(10);
  assert(result == 0 && "fiber_sleep should return 0 on success");
}

void test_fiber_stress(void) {
#define NUM_FIBERS 50
  int64_t fiber_ids[NUM_FIBERS];

  for (int i = 0; i < NUM_FIBERS; i++) {
    fiber_ids[i] = fiber_spawn(test_function_1);
    assert(fiber_ids[i] > 0 && "each fiber should have a valid ID");
  }

  for (int i = 0; i < NUM_FIBERS; i++) {
    int64_t result = fiber_await(fiber_ids[i]);
    assert(result == 42 && "each fiber should return 42");
  }
}

void test_concurrent_execution(void) {
  int64_t slow_fiber = fiber_spawn(slow_function);
  int64_t fast_fiber = fiber_spawn(test_function_1);

  int64_t fast_result = fiber_await(fast_fiber);
  assert(fast_result == 42 && "fast fiber should return 42");

  int64_t slow_result = fiber_await(slow_fiber);
  assert(slow_result == 999 && "slow fiber should return 999");
}

void test_fiber_done(void) {
  assert(fiber_done(99999) == -1 &&
         "fiber_done should return -1 for an out-of-range fiber ID");
  assert(fiber_done(-1) == -1 &&
         "fiber_done should return -1 for a negative fiber ID");

  int64_t slow_fiber = fiber_spawn(slow_function);
  assert(fiber_done(slow_fiber) == 0 &&
         "fiber_done should report 0 while the fiber is running");

  int64_t slow_result = fiber_await(slow_fiber);
  assert(slow_result == 999 && "slow fiber should return 999");
  assert(fiber_done(slow_fiber) == 1 &&
         "fiber_done should report 1 after completion");
}

void run_all_fiber_tests(void) {
  test_null_function_pointer();
  test_invalid_fiber_await();
  test_valid_fiber_spawn();
  test_multiple_fibers();
  test_fiber_bounds_checking();
  test_invalid_channel_capacity();
  test_fiber_sleep();
  test_fiber_done();
  test_concurrent_execution();
  test_fiber_stress();

  puts("fiber runtime tests passed");
}

int main(void) {
  run_all_fiber_tests();
  return 0;
}
