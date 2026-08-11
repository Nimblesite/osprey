// Exercises [CONCURRENCY-SPAWN-AWAIT], [CONCURRENCY-CHANNEL],
// [CONCURRENCY-YIELD], deterministic mode, the env-carrying spawn forms, and
// the process bridge — plus the pthread transition in [MEM-FIBER-ISOLATION].
#include <assert.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

extern int64_t fiber_spawn(int64_t (*fn)(void));
extern int64_t fiber_spawn_env(int64_t (*fn)(void *), void *env);
extern int64_t fiber_spawn_env_owned(int64_t (*fn)(void *), void *env,
                                     int64_t result_managed);
extern int64_t fiber_await(int64_t fiber_id);
extern int64_t fiber_done(int64_t fiber_id);
extern int64_t fiber_sleep(int64_t milliseconds);
extern int64_t fiber_yield(int64_t value);
extern int64_t fiber_set_deterministic_mode(bool enabled);
extern void fiber_cleanup_results(void);
extern int64_t fiber_await_process(int64_t process_id);
extern int64_t fiber_await_process_with_callback(int64_t process_id,
                                                 void (*stdout_callback)(char *));
extern void fiber_cleanup_process(int64_t process_id);
extern int64_t channel_create(int64_t capacity);
extern int64_t channel_send(int64_t channel_id, int64_t value);
extern int64_t channel_recv(int64_t channel_id);
extern int64_t spawn_process_with_handler(const char *command,
                                          void (*handler)(int64_t, int64_t,
                                                          char *));

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

void test_repeated_fiber_await(void) {
  int64_t fiber_id = fiber_spawn(test_function_1);
  assert(fiber_id > 0 && "fiber should spawn for repeated await");

  int64_t first = fiber_await(fiber_id);
  int64_t second = fiber_await(fiber_id);
  assert(first == 42 && second == 42 &&
         "a reusable fiber should return the same result on every await");
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

// [CONCURRENCY-YIELD] yield is a value-preserving passthrough on every path.
void test_fiber_yield_passthrough(void) {
  assert(fiber_yield(42) == 42);
  assert(fiber_yield(0) == 0);
  assert(fiber_yield(-7) == -7);
  assert(fiber_yield(INT64_MAX) == INT64_MAX);
}

static int64_t env_doubler(void *env) { return *(int64_t *)env * 2; }

// The env-carrying spawn forms deliver the environment pointer to the thunk
// and reject a NULL function like the plain form.
void test_fiber_spawn_env_forms(void) {
  int64_t twenty_one = 21;
  int64_t f = fiber_spawn_env(env_doubler, &twenty_one);
  assert(f > 0 && fiber_await(f) == 42);
  int64_t scalar = fiber_spawn_env_owned(env_doubler, &twenty_one, 0);
  int64_t managed = fiber_spawn_env_owned(env_doubler, &twenty_one, 1);
  assert(scalar > 0 && managed > 0);
  assert(fiber_await(scalar) == 42 && fiber_await(managed) == 42);
  assert(fiber_spawn_env(NULL, &twenty_one) == -1);
  assert(fiber_spawn_env_owned(NULL, &twenty_one, 0) == -1);
}

// Deterministic mode executes fibers immediately with identical observable
// results — same values, same repeated-await semantics — and toggles cleanly.
void test_deterministic_mode(void) {
  assert(fiber_set_deterministic_mode(true) == 0);
  int64_t a = fiber_spawn(test_function_1);
  int64_t b = fiber_spawn(test_function_2);
  assert(a > 0 && b > 0 && a != b);
  assert(fiber_done(a) == 0); // queued, not yet executed
  assert(fiber_await(b) == 100); // executes the queue IN ORDER up to b
  assert(fiber_done(a) == 1);    // ...so a completed on the way
  assert(fiber_await(a) == 42);
  assert(fiber_await(a) == 42); // repeated await still answers
  assert(fiber_yield(9) == 9);  // yield stays a passthrough here too
  int64_t env_val = 21;
  int64_t c = fiber_spawn_env(env_doubler, &env_val);
  assert(c > 0 && fiber_await(c) == 42);
  assert(fiber_set_deterministic_mode(false) == 0);
  int64_t d = fiber_spawn(test_function_1); // threaded mode works again
  assert(d > 0 && fiber_await(d) == 42);
}

// fiber_cleanup_results releases retained results without breaking later
// spawns: the runtime stays fully usable afterwards.
void test_cleanup_results(void) {
  int64_t f = fiber_spawn(test_function_1);
  assert(fiber_await(f) == 42);
  fiber_cleanup_results();
  int64_t g = fiber_spawn(test_function_2);
  assert(g > 0 && fiber_await(g) == 100);
  fiber_cleanup_results();
}

static void fiber_proc_event_sink(int64_t process_id, int64_t event_type,
                                  char *data) {
  (void)process_id;
  (void)event_type;
  (void)data;
}

static int stdout_cb_calls = 0;
static void fiber_proc_stdout_cb(char *data) {
  (void)data;
  stdout_cb_calls++;
}

// The fiber-side process bridge forwards to the process runtime: exact exit
// codes come back through both await forms, invalid ids are rejected, and
// cleanup makes an id unknown.
void test_fiber_process_bridge(void) {
  assert(fiber_await_process(-1) == -1);
  assert(fiber_await_process(0) == -1);
  assert(fiber_await_process(999999) == -1);
  int64_t ok = spawn_process_with_handler("true", fiber_proc_event_sink);
  assert(ok > 0 && fiber_await_process(ok) == 0);
  fiber_cleanup_process(ok);
  assert(fiber_await_process(ok) == -1); // cleaned up: id no longer resolves
  int64_t code = spawn_process_with_handler("sh -c 'exit 5'",
                                            fiber_proc_event_sink);
  assert(code > 0 && fiber_await_process(code) == 5);
  fiber_cleanup_process(code);
  int64_t cb = spawn_process_with_handler("true", fiber_proc_event_sink);
  assert(cb > 0);
  assert(fiber_await_process_with_callback(cb, fiber_proc_stdout_cb) == 0);
  fiber_cleanup_process(cb);
  int64_t no_cb = spawn_process_with_handler("true", fiber_proc_event_sink);
  assert(no_cb > 0);
  assert(fiber_await_process_with_callback(no_cb, NULL) == 0); // NULL: plain await
  fiber_cleanup_process(no_cb);
}

void run_all_fiber_tests(void) {
  test_null_function_pointer();
  test_invalid_fiber_await();
  test_valid_fiber_spawn();
  test_repeated_fiber_await();
  test_multiple_fibers();
  test_fiber_bounds_checking();
  test_invalid_channel_capacity();
  test_fiber_sleep();
  test_fiber_done();
  test_concurrent_execution();
  test_fiber_stress();
  test_fiber_yield_passthrough();
  test_fiber_spawn_env_forms();
  test_deterministic_mode();
  test_cleanup_results();
  test_fiber_process_bridge();

  puts("fiber runtime tests passed");
}

int main(void) {
  run_all_fiber_tests();
  return 0;
}
