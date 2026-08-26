// Exercises [CONCURRENCY-SPAWN-AWAIT], [CONCURRENCY-CHANNEL],
// [CONCURRENCY-YIELD], [CONCURRENCY-SLEEP], [CONCURRENCY-DETERMINISTIC], the
// env-carrying spawn forms, and the process bridge — plus the pthread
// transition in [MEM-FIBER-ISOLATION].
// Spec: docs/specs/0011-LightweightFibersAndConcurrency.md.
#include "test_death.h"

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
extern int64_t channel_create_checked(int64_t capacity);
extern int64_t channel_send(int64_t channel_id, int64_t value, int64_t managed);
extern void channel_cleanup(void);
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
  // A queued deterministic fiber reports READY before it has run: execution
  // happens at the await, and a `while (!fiber_done(f))` poll would otherwise
  // spin forever with nothing to drive it (fiber_runtime.c's `is_deterministic`
  // early return). The contract is "safe to await", not "already finished".
  assert(fiber_done(a) == 1);
  assert(fiber_await(b) == 100); // executes the queue IN ORDER up to b
  assert(fiber_done(a) == 1);    // ...so a completed on the way
  assert(fiber_await(a) == 42);
  assert(fiber_await(a) == 42); // repeated await still answers
  assert(fiber_yield(9) == 9);  // yield stays a passthrough here too
  int64_t env_val = 21;
  int64_t c = fiber_spawn_env(env_doubler, &env_val);
  assert(c > 0 && fiber_await(c) == 42);
  // [CONCURRENCY-SPAWN-AWAIT]: "for a managed T, every await produces an
  // independently owned reference". Queued fibers take that +1 on the
  // deterministic path too, so a second await is never a use-after-release.
  int64_t owned = fiber_spawn_env_owned(env_doubler, &env_val, 1);
  assert(owned > 0 && owned != c);
  assert(fiber_await(owned) == 42);
  assert(fiber_await(owned) == 42);
  assert(fiber_await(owned) == 42);
  assert(fiber_done(owned) == 1);
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

// --- [CONCURRENCY-CHANNEL] -------------------------------------------------
// "Channel(capacity) creates a FIFO channel whose positive integer capacity is
// the number of buffered values"; send blocks while full, recv blocks while
// empty (docs/specs/0011-LightweightFibersAndConcurrency.md).

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
  const int64_t unassigned[] = {997, 998, 999};
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

// Fibers and channels draw from ONE finite handle space. Once it is spent both
// constructors must report -4 rather than index past their arrays, and the
// range guards must keep answering — they do not depend on the id supply.
// Runs LAST: it consumes the process's remaining handles.
void test_handle_space_exhaustion(void) {
  int64_t created = 0;
  int64_t last = 0;
  while ((last = channel_create(1)) > 0) {
    created++;
    assert(created < 1000 && "the id supply must be finite");
  }
  assert(created > 0 && "the loop must have created channels before failing");
  assert(last == -4 && "an exhausted handle space reports -4");
  assert(channel_create(1) == -4 && "exhaustion is sticky");
  assert(channel_create(CHANNEL_CAP) == -4 && "capacity does not matter now");
  int64_t env_val = 21;
  assert(fiber_spawn(test_function_1) == -4 &&
         "fibers share the exhausted id space");
  assert(fiber_spawn_env(env_doubler, &env_val) == -4);
  assert(fiber_spawn_env_owned(env_doubler, &env_val, 1) == -4);
  assert(channel_create(0) == -1 &&
         "an invalid capacity is still rejected before the id supply is read");
  assert(fiber_spawn(NULL) == -1 && "a null thunk is still rejected first");
  // The array is now mostly channels, so the cleanup walk steps over NULL
  // fiber slots for nearly every id and must survive the gaps.
  fiber_cleanup_results();
  assert(fiber_await(99999) == -1);
  assert(fiber_done(99999) == -1);
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

void run_all_fiber_tests(void) {
  test_null_function_pointer();
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
  test_unassigned_fiber_handles();
  test_cleanup_defers_while_a_fiber_runs();
  test_cleanup_drains_unreceived_values();
  test_rejected_managed_send_releases_its_value();
  test_handle_space_exhaustion(); // LAST: spends the process's handle space

  puts("fiber runtime tests passed");
}

int main(void) {
  run_all_fiber_tests();
  return 0;
}
