// Exercises the pthread transition used by [MEM-FIBER-ISOLATION].
#include <assert.h>
#include <pthread.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

// Include the fiber runtime functions
extern int64_t fiber_spawn(int64_t (*fn)(void));
extern int64_t fiber_await(int64_t fiber_id);
extern int64_t fiber_done(int64_t fiber_id);
extern int64_t fiber_sleep(int64_t milliseconds);
extern int64_t channel_create(int64_t capacity);
extern int64_t channel_send(int64_t channel_id, int64_t value);
extern int64_t channel_recv(int64_t channel_id);

// Test functions
static int64_t test_function_1(void) { return 42; }

static int64_t test_function_2(void) { return 100; }

static int64_t slow_function(void) {
  usleep(100000); // 100ms
  return 999;
}

// CRITICAL TEST: Null function pointer - prevents segfaults
void test_null_function_pointer(void) {
  printf("Testing null function pointer (CRITICAL SEGFAULT PREVENTION)...\n");

  int64_t result = fiber_spawn(NULL);
  assert(result == -1 &&
         "FAIL: fiber_spawn should return -1 for null function pointer");

  printf("✅ PASS: Null function pointer handled correctly - NO SEGFAULT!\n");
}

// CRITICAL TEST: Invalid fiber ID - prevents buffer overflow segfaults
void test_invalid_fiber_await(void) {
  printf("Testing invalid fiber ID in await (CRITICAL BUFFER OVERFLOW "
         "PREVENTION)...\n");

  int64_t result = fiber_await(99999); // Way out of bounds
  assert(result == -1 &&
         "FAIL: fiber_await should return -1 for invalid fiber ID");

  result = fiber_await(-1); // Negative ID
  assert(result == -1 &&
         "FAIL: fiber_await should return -1 for negative fiber ID");

  printf("✅ PASS: Invalid fiber IDs handled correctly - NO BUFFER OVERFLOW!\n");
}

// Test valid fiber spawning and execution
void test_valid_fiber_spawn(void) {
  printf("Testing valid fiber spawning...\n");

  int64_t fiber_id = fiber_spawn(test_function_1);
  assert(fiber_id > 0 &&
         "FAIL: fiber_spawn should return positive ID for valid function");

  int64_t result = fiber_await(fiber_id);
  assert(result == 42 && "FAIL: fiber should return expected value");

  printf("✅ PASS: Valid fiber spawning works correctly\n");
}

// Test multiple concurrent fibers
void test_multiple_fibers(void) {
  printf("Testing multiple concurrent fibers...\n");

  int64_t fiber1 = fiber_spawn(test_function_1);
  int64_t fiber2 = fiber_spawn(test_function_2);

  assert(fiber1 > 0 && fiber2 > 0 && "FAIL: Both fibers should have valid IDs");
  assert(fiber1 != fiber2 && "FAIL: Fibers should have different IDs");

  int64_t result1 = fiber_await(fiber1);
  int64_t result2 = fiber_await(fiber2);

  assert(result1 == 42 && "FAIL: First fiber should return 42");
  assert(result2 == 100 && "FAIL: Second fiber should return 100");

  printf("✅ PASS: Multiple fibers work correctly\n");
}

// Test fiber bounds checking - prevents array overflow
void test_fiber_bounds_checking(void) {
  printf("Testing fiber bounds checking (PREVENTS SEGFAULTS)...\n");

  // Test invalid channel IDs
  int64_t result = channel_send(99999, 42); // Out of bounds
  assert(result == 0 && "FAIL: channel_send should handle out of bounds ID");

  result = channel_recv(99999); // Out of bounds
  assert(result == -1 && "FAIL: channel_recv should handle out of bounds ID");

  printf("✅ PASS: Bounds checking prevents segfaults\n");
}

// Test fiber sleep function
void test_fiber_sleep(void) {
  printf("Testing fiber sleep function...\n");

  int64_t result = fiber_sleep(10); // Sleep for 10ms
  assert(result == 0 && "FAIL: fiber_sleep should return 0 on success");

  printf("✅ PASS: Fiber sleep works correctly\n");
}

// Stress test - create many fibers to test stability
void test_fiber_stress(void) {
  printf("Testing fiber stress (memory management)...\n");

#define NUM_FIBERS 50
  int64_t fiber_ids[NUM_FIBERS];

  // Spawn many fibers
  for (int i = 0; i < NUM_FIBERS; i++) {
    fiber_ids[i] = fiber_spawn(test_function_1);
    assert(fiber_ids[i] > 0 && "FAIL: Each fiber should have valid ID");
  }

  // Wait for all fibers
  for (int i = 0; i < NUM_FIBERS; i++) {
    int64_t result = fiber_await(fiber_ids[i]);
    assert(result == 42 && "FAIL: Each fiber should return 42");
  }

  printf("✅ PASS: Stress test with %d fibers completed successfully\n",
         NUM_FIBERS);
}

// Test concurrent execution
void test_concurrent_execution(void) {
  printf("Testing concurrent fiber execution...\n");

  // Spawn a slow fiber
  int64_t slow_fiber = fiber_spawn(slow_function);

  // Spawn a fast fiber
  int64_t fast_fiber = fiber_spawn(test_function_1);

  // The fast fiber should complete quickly even though slow fiber is running
  int64_t fast_result = fiber_await(fast_fiber);
  assert(fast_result == 42 && "FAIL: Fast fiber should return 42");

  // Now wait for the slow fiber
  int64_t slow_result = fiber_await(slow_fiber);
  assert(slow_result == 999 && "FAIL: Slow fiber should return 999");

  printf("✅ PASS: Concurrent execution works correctly\n");
}

// Test the non-blocking completion probe used by the animated-loader pattern.
void test_fiber_done(void) {
  printf("Testing non-blocking fiber_done probe...\n");

  assert(fiber_done(99999) == -1 &&
         "FAIL: fiber_done should return -1 for invalid fiber ID");
  assert(fiber_done(-1) == -1 &&
         "FAIL: fiber_done should return -1 for negative fiber ID");

  // A slow fiber is still running immediately after spawn.
  int64_t slow_fiber = fiber_spawn(slow_function);
  assert(fiber_done(slow_fiber) == 0 &&
         "FAIL: fiber_done should report 0 while the fiber is still running");

  // After await it has completed and the probe must report ready.
  int64_t slow_result = fiber_await(slow_fiber);
  assert(slow_result == 999 && "FAIL: Slow fiber should return 999");
  assert(fiber_done(slow_fiber) == 1 &&
         "FAIL: fiber_done should report 1 once the fiber has completed");

  printf("✅ PASS: fiber_done reports completion without blocking\n");
}

// Main test runner
void run_all_fiber_tests(void) {
  printf("\n=== FIBER RUNTIME TESTS ===\n");
  printf("🧪 TESTING CRITICAL SEGFAULT PREVENTION FIXES...\n\n");

  test_null_function_pointer();
  test_invalid_fiber_await();
  test_valid_fiber_spawn();
  test_multiple_fibers();
  test_fiber_bounds_checking();
  test_fiber_sleep();
  test_fiber_done();
  test_concurrent_execution();
  test_fiber_stress();

  printf("\n🎉 ALL FIBER TESTS PASSED! 🎉\n");
}

int main(void) {
  run_all_fiber_tests();
  return 0;
}
