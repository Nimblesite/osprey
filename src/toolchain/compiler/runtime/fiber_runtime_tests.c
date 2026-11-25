// Exercises [CONCURRENCY-SPAWN-AWAIT], [CONCURRENCY-CHANNEL],
// [CONCURRENCY-YIELD], [CONCURRENCY-SLEEP], [CONCURRENCY-DETERMINISTIC], the
// env-carrying spawn forms, and the process bridge — plus the pthread
// transition in [MEM-FIBER-ISOLATION].
// Spec: docs/specs/language/0011-LightweightFibersAndConcurrency.md.
#include "test_death.h"

#include <assert.h>
#include <pthread.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#include "fiber_tests_shared.h"

static int64_t test_function_1(void *env) {
  (void)env;
  return 42;
}

static int64_t test_function_2(void *env) {
  (void)env;
  return 100;
}

static int64_t slow_function(void *env) {
  (void)env;
  usleep(100000); // 100ms
  return 999;
}

// No fiber at all answers -1 from every probe, never a read of NULL.
void test_invalid_fiber_await(void) {
  assert(fiber_await(NULL) == -1 && "awaiting no handle is an error");
  assert(fiber_join(NULL) == -1 && "joining no fiber is an error");
  assert(fiber_id(NULL) == -1 && "no fiber has no spawn number");
  fiber_release(NULL); // ending no hold is a no-op
}

void test_valid_fiber_spawn(void) {
  void *fiber = spawn_fiber(test_function_1, NULL);
  assert(fiber != NULL && "spawn should answer a handle for a valid function");
  assert(fiber_await(fiber) == 42 && "fiber should return its function's value");
  Fiber *held = fiber_spawn_env(test_function_1, NULL);
  assert(held != NULL && fiber_join(held) == 42 && fiber_join(held) == 42 &&
         "the runtime's own hold joins the same value every time");
  fiber_release(held);
}

void test_repeated_fiber_await(void) {
  void *fiber = spawn_fiber(test_function_1, NULL);
  assert(fiber != NULL && "fiber should spawn for repeated await");

  int64_t first = fiber_await(fiber);
  int64_t second = fiber_await(fiber);
  assert(first == 42 && second == 42 &&
         "a reusable fiber should return the same result on every await");
}

void test_multiple_fibers(void) {
  void *fiber1 = spawn_fiber(test_function_1, NULL);
  void *fiber2 = spawn_fiber(test_function_2, NULL);

  assert(fiber1 != NULL && fiber2 != NULL && "both fibers should have handles");
  assert(fiber1 != fiber2 && "fibers should have different handles");
  assert(fiber_id(fiber1) > 0 && fiber_id(fiber2) == fiber_id(fiber1) + 1 &&
         "spawn numbers count spawns in order");

  int64_t result1 = fiber_await(fiber1);
  int64_t result2 = fiber_await(fiber2);

  assert(result1 == 42 && "first fiber should return 42");
  assert(result2 == 100 && "second fiber should return 100");
}

static void sleep_past_microsecond_word(void) {
  (void)fiber_sleep(INT64_C(4294968)); // 32-bit microseconds would wrap to 704us.
}

void test_fiber_sleep(void) {
  // [CONCURRENCY-SLEEP] Non-positive durations return immediately.
  assert(fiber_sleep(0) == 0);
  assert(fiber_sleep(-1) == 0);
  int64_t result = fiber_sleep(10);
  assert(result == 0 && "fiber_sleep should return 0 on success");
  assert(osp_death_signal_within(sleep_past_microsecond_word, 1) ==
             OSP_DEATH_STALLED &&
         "[CONCURRENCY-SLEEP] a long sleep must not wrap to a short delay");
}

void test_fiber_stress(void) {
#define NUM_FIBERS 50
  void *fiber_ids[NUM_FIBERS];

  for (int i = 0; i < NUM_FIBERS; i++) {
    fiber_ids[i] = spawn_fiber(test_function_1, NULL);
    assert(fiber_ids[i] != NULL && "each fiber should have a handle");
  }

  for (int i = 0; i < NUM_FIBERS; i++) {
    int64_t result = fiber_await(fiber_ids[i]);
    assert(result == 42 && "each fiber should return 42");
  }
}

void test_concurrent_execution(void) {
  void *slow_fiber = spawn_fiber(slow_function, NULL);
  void *fast_fiber = spawn_fiber(test_function_1, NULL);

  int64_t fast_result = fiber_await(fast_fiber);
  assert(fast_result == 42 && "fast fiber should return 42");

  int64_t slow_result = fiber_await(slow_fiber);
  assert(slow_result == 999 && "slow fiber should return 999");
}

void test_fiber_done(void) {
  assert(fiber_done(NULL) == -1 && "fiber_done should return -1 for no handle");

  void *slow_fiber = spawn_fiber(slow_function, NULL);
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

// env_doubler turns the 21 an env spawn hands it into 42 on every form.
static void await_env_doubled(void *fiber) {
  assert(fiber != NULL && fiber_await(fiber) == 42 &&
         "an env fiber should double the environment value");
}

// The env-carrying spawn forms deliver the environment pointer to the thunk
// and reject a NULL function before scheduling.
void test_fiber_spawn_env_forms(void) {
  int64_t twenty_one = 21;
  Fiber *held = fiber_spawn_env(env_doubler, &twenty_one);
  assert(held != NULL && fiber_join(held) == 42);
  fiber_release(held);
  void *scalar = fiber_spawn_env_owned(env_doubler, &twenty_one, 0);
  void *managed = fiber_spawn_env_owned(env_doubler, &twenty_one, 1);
  assert(scalar != NULL && managed != NULL);
  await_env_doubled(scalar);
  await_env_doubled(managed);
  assert(fiber_spawn_env(NULL, &twenty_one) == NULL);
  assert(fiber_spawn_env_owned(NULL, &twenty_one, 0) == NULL);
}

// [CONCURRENCY-SPAWN-AWAIT]: a reusable fiber answers every await.
static void await_repeatedly(void *fiber, int64_t expected, int times) {
  for (int i = 0; i < times; i++) {
    assert(fiber_await(fiber) == expected &&
           "[CONCURRENCY-SPAWN-AWAIT] every await should return the value");
  }
}

// Deterministic mode executes fibers immediately with identical observable
// results — same values, same repeated-await semantics — and toggles cleanly.
void test_deterministic_mode(void) {
  assert(fiber_set_deterministic_mode(true) == 0);
  void *a = spawn_fiber(test_function_1, NULL);
  void *b = spawn_fiber(test_function_2, NULL);
  assert(a != NULL && b != NULL && a != b);
  // A queued deterministic fiber reports READY before it has run: execution
  // happens at the await, and a `while (!fiber_done(f))` poll would otherwise
  // spin forever with nothing to drive it (fiber_runtime.c's `is_deterministic`
  // early return). The contract is "safe to await", not "already finished".
  assert(fiber_done(a) == 1);
  assert(fiber_await(b) == 100); // executes the queue IN ORDER up to b
  assert(fiber_done(a) == 1);    // ...so a completed on the way
  await_repeatedly(a, 42, 2);    // repeated await still answers
  assert(fiber_yield(9) == 9);   // yield stays a passthrough here too
  int64_t env_val = 21;
  void *c = spawn_fiber(env_doubler, &env_val);
  await_env_doubled(c);
  // [CONCURRENCY-SPAWN-AWAIT]: "for a managed T, every await produces an
  // independently owned reference". Queued fibers take that +1 on the
  // deterministic path too, so a second await is never a use-after-release.
  void *owned = fiber_spawn_env_owned(env_doubler, &env_val, 1);
  assert(owned != NULL && owned != c);
  await_repeatedly(owned, 42, 3);
  assert(fiber_done(owned) == 1);
  assert(fiber_set_deterministic_mode(false) == 0);
  void *d = spawn_fiber(test_function_1, NULL); // threaded mode works again
  assert(d != NULL && fiber_await(d) == 42);
}

// fiber_cleanup_results reclaims the fibers whose handles died without
// touching a fiber a handle still holds: the runtime stays fully usable.
void test_cleanup_results(void) {
  void *f = spawn_fiber(test_function_1, NULL);
  assert(fiber_await(f) == 42);
  fiber_cleanup_results();
  assert(fiber_await(f) == 42 && "a held fiber survives cleanup");
  void *g = spawn_fiber(test_function_2, NULL);
  assert(g != NULL && fiber_await(g) == 100);
  fiber_cleanup_results();
  assert(fiber_await(g) == 100 && fiber_done(g) == 1);
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

// Spawn under the event sink and check the exit code comes back.
static int64_t await_exit_code(const char *command, int64_t expected) {
  int64_t pid = spawn_process_with_handler(command, fiber_proc_event_sink);
  assert(pid > 0 && fiber_await_process(pid) == expected &&
         "the process bridge should report the process's exit code");
  fiber_cleanup_process(pid);
  return pid;
}

// The callback form answers the exit code too, with or without a callback.
static void await_with_callback(void (*callback)(char *)) {
  int64_t pid = spawn_process_with_handler("true", fiber_proc_event_sink);
  assert(pid > 0 && fiber_await_process_with_callback(pid, callback) == 0 &&
         "the callback await form should report the exit code");
  fiber_cleanup_process(pid);
}

// The fiber-side process bridge forwards to the process runtime: exact exit
// codes come back through both await forms, invalid ids are rejected, and
// cleanup makes an id unknown.
void test_fiber_process_bridge(void) {
  assert(fiber_await_process(-1) == -1);
  assert(fiber_await_process(0) == -1);
  assert(fiber_await_process(999999) == -1);
  int64_t ok = await_exit_code("true", 0);
  assert(fiber_await_process(ok) == -1); // cleaned up: id no longer resolves
  (void)await_exit_code("sh -c 'exit 5'", 5);
  await_with_callback(fiber_proc_stdout_cb);
  await_with_callback(NULL); // NULL: plain await
}

void run_all_fiber_tests(void) {
  test_invalid_fiber_await();
  test_valid_fiber_spawn();
  test_repeated_fiber_await();
  test_multiple_fibers();
  test_fiber_sleep();
  test_fiber_done();
  test_concurrent_execution();
  test_fiber_stress();
  test_fiber_yield_passthrough();
  test_fiber_spawn_env_forms();
  test_deterministic_mode();
  test_cleanup_results();
  test_fiber_process_bridge();
  test_channel_fifo_and_wraparound();
  test_channel_capacity_rejections();
  test_unusable_capacities_are_refused_not_returned();
  test_channel_handle_guards();
  test_channel_blocks_both_directions();
  test_cleanup_defers_while_a_fiber_runs();
  test_cleanup_drains_unreceived_values();
  test_rejected_managed_send_releases_its_value();
  run_fiber_handle_tests();

  puts("fiber runtime tests passed");
}

int main(void) {
  test_channel_select();
  run_all_fiber_tests();
  return 0;
}
