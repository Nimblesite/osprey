// Waiting without holding up the collector [MEM-GC-SAFEPOINT]. A thread blocked
// here cannot change its stack, so it counts as parked. Its registers are spilled
// at the bottom of what the collector scans, so a pointer living only in a
// callee-saved register stays a root.
#ifndef OSPREY_MEMORY_PARKED_WAIT_H
#define OSPREY_MEMORY_PARKED_WAIT_H

#include "memory_hooks.h"
#include <errno.h>
#include <pthread.h>
#include <time.h>

// WASI preview1 runs one thread [WASM-TARGET], and KLEE explores one
// [SYMEX-MOCK-THREADS]: fibers run on it in spawn order
// [CONCURRENCY-DETERMINISTIC].
#if defined(__wasi__) || defined(OSP_SYMBOLIC)
#define OSP_ONE_THREAD 1
#else
#define OSP_ONE_THREAD 0
#endif

#if OSP_ONE_THREAD
// No other stack can be scanned, so there are no registers to spill, and no
// other thread can ever signal.
typedef int OspRegisters;
#define OSP_SPILL_REGISTERS(buffer) ((void)((buffer) = 0))
#include <stdio.h>
#include <stdlib.h>

#if defined(OSP_SYMBOLIC)
// Symbolic time is the mock's clock, which every reading advances: nothing sleeps.
static inline void osp_one_thread_sleep(int64_t nanoseconds) { (void)nanoseconds; }
#else
#include <wasi/api.h>

// Sleeps the one thread for `nanoseconds` through WASI's own clock
// subscription. wasi-libc's nanosleep needs a 128-bit multiply from
// compiler-rt that the wasm32 link does not carry.
static inline void osp_one_thread_sleep(int64_t nanoseconds) {
  if (nanoseconds <= 0) {
    return;
  }
  __wasi_subscription_t wake = {
      .userdata = 0,
      .u = {.tag = __WASI_EVENTTYPE_CLOCK,
            .u = {.clock = {.id = __WASI_CLOCKID_MONOTONIC,
                            .timeout = (__wasi_timestamp_t)nanoseconds,
                            .precision = 0,
                            .flags = 0}}}};
  __wasi_event_t woke;
  __wasi_size_t events = 0;
  (void)__wasi_poll_oneoff(&wake, &woke, 1, &events);
}
#endif

// Exit status of a run whose one thread waits for something only another
// thread could do: every fiber is blocked, so nothing can ever wake it.
#define OSP_ALL_BLOCKED_STATUS 70

// A wait on the one thread. A bounded wait sleeps to its deadline, as nothing
// can change before it; an unbounded one could never end, so it stops the run
// by name instead of hanging. [CONCURRENCY-WASM] [SYMEX-MOCK-THREADS]
// [FAILURE-EXPLICIT]
static inline int osp_parked_wait(pthread_cond_t *cond, pthread_mutex_t *lock, const struct timespec *deadline) {
  (void)cond;
  (void)lock;
  if (deadline == NULL) {
    fputs("osprey: every fiber is blocked: this wait can never be woken\n", stderr);
#if defined(OSP_SYMBOLIC)
    abort(); // the symbolic runtime's modelled stop: the hunt reports the deadlock
#else
    _Exit(OSP_ALL_BLOCKED_STATUS);
#endif
  }
  struct timespec now;
  if (clock_gettime(CLOCK_REALTIME, &now) == 0) {
    osp_one_thread_sleep((int64_t)(deadline->tv_sec - now.tv_sec) * 1000000000LL +
                   (deadline->tv_nsec - now.tv_nsec));
  }
  return ETIMEDOUT;
}
#else
#include <setjmp.h>
typedef jmp_buf OspRegisters;
#if defined(__APPLE__)
#define OSP_SPILL_REGISTERS(buffer) ((void)_setjmp(buffer))
#else
#define OSP_SPILL_REGISTERS(buffer) ((void)setjmp(buffer))
#endif

// pthread_cond_wait, or pthread_cond_timedwait when `deadline` is set.
static inline int osp_parked_wait(pthread_cond_t *cond, pthread_mutex_t *lock, const struct timespec *deadline) {
  OspRegisters registers;
  OSP_SPILL_REGISTERS(registers);
  osp_mem_baton_park(&registers);
  int result = deadline ? pthread_cond_timedwait(cond, lock, deadline) : pthread_cond_wait(cond, lock);
  osp_mem_baton_unpark();
  return result;
}
#endif

#endif // OSPREY_MEMORY_PARKED_WAIT_H
