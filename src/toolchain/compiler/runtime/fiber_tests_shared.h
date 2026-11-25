// The fiber and channel runtime entry points the fiber_runtime_tests suite
// calls, and the cases each of its translation units contributes:
// fiber_runtime_tests.c (fibers, sleep, yield, processes), fiber_channel_tests.c
// (channels, select, handle guards and cleanup) and fiber_handle_tests.c
// (handle capacity and sequential mode).
#ifndef OSPREY_FIBER_TESTS_SHARED_H
#define OSPREY_FIBER_TESTS_SHARED_H

#include <stdbool.h>
#include <stdint.h>

#include "fiber_api.h"

extern int64_t fiber_sleep(int64_t milliseconds);
extern int64_t fiber_yield(int64_t value);
extern int64_t fiber_set_deterministic_mode(bool enabled);
extern void fiber_cleanup_results(void);
extern int64_t fiber_await_process(int64_t process_id);
extern int64_t fiber_await_process_with_callback(int64_t process_id,
                                                 void (*stdout_callback)(char *));
extern void fiber_cleanup_process(int64_t process_id);
extern int64_t spawn_process_with_handler(const char *command,
                                          void (*handler)(int64_t, int64_t,
                                                          char *));

// A fiber body that sleeps 100 ms and answers 999 (fiber_runtime_tests.c).

// A compiled program's `spawn` of a scalar body: the managed handle it holds.
static inline void *spawn_fiber(int64_t (*fn)(void *), void *env) {
  return fiber_spawn_env_owned(fn, env, 0);
}

// fiber_handle_tests.c
void run_fiber_handle_tests(void);

// fiber_channel_tests.c
void test_channel_fifo_and_wraparound(void);
void test_channel_capacity_rejections(void);
void test_unusable_capacities_are_refused_not_returned(void);
void test_channel_handle_guards(void);
void test_channel_blocks_both_directions(void);
void test_cleanup_defers_while_a_fiber_runs(void);
void test_cleanup_keeps_what_a_held_channel_buffers(void);
void test_rejected_managed_send_releases_its_value(void);
void test_channel_select(void);
void test_fibers_blocked_for_ever_are_found(void);

#endif
