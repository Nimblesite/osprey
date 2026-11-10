// Waiting from a fiber: sleeping without holding up a collection, the
// non-blocking completion probe, and the process awaits. Included once, last,
// by fiber_runtime.c, whose fiber table it shares.
#ifndef OSPREY_FIBER_WAITS_H
#define OSPREY_FIBER_WAITS_H

// Sleep for `milliseconds`, however many: the count never narrows to 32-bit
// microseconds (a day's sleep once wrapped to 500 s). On Linux the profiler's
// directed SIGPROF would cut a sleep short (sleeping calls are exempt from
// SA_RESTART), so it sleeps to an absolute monotonic deadline and re-arms on
// EINTR; elsewhere nanosleep resumes with the time it had left. The wait stays
// exact whether or not sampling is active [PROF-COLLECT-SAMPLER].
static struct timespec span_of(int64_t milliseconds) {
  struct timespec span;
  span.tv_sec = (time_t)(milliseconds / 1000);
  span.tv_nsec = (long)(milliseconds % 1000) * 1000000L;
  return span;
}

static void sleep_for(int64_t milliseconds) {
  struct timespec span = span_of(milliseconds);
#if defined(__linux__)
  struct timespec until;
  if (clock_gettime(CLOCK_MONOTONIC, &until) == 0) {
    until.tv_sec += span.tv_sec;
    until.tv_nsec += span.tv_nsec;
    if (until.tv_nsec >= 1000000000L) {
      until.tv_sec += 1;
      until.tv_nsec -= 1000000000L;
    }
    while (clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &until, NULL) == EINTR) {
    }
    return;
  }
#endif
  while (nanosleep(&span, &span) == -1 && errno == EINTR) {
  }
}

// A sleeping fiber holds up no collection.
int64_t fiber_sleep(int64_t milliseconds) {
  if (milliseconds <= 0) {
    return 0;
  }
  jmp_buf registers;
  OSP_SPILL_REGISTERS(registers);
  osp_mem_baton_park(&registers);
  sleep_for(milliseconds);
  osp_mem_baton_unpark();
  return 0;
}

// Non-blocking completion probe. Returns 1 if the fiber has finished, 0 if it
// is still running, -1 for an invalid id. Lets a caller animate (sleep + redraw)
// while a fiber does real work, then `fiber_await` it the instant it reports
// done - without ever blocking the animating thread on a join or condvar.
int64_t fiber_done(int64_t fiber_id) {
  Fiber *fiber = fiber_of(fiber_id);
  if (!fiber)
    return -1;

  // Deterministic fibers only run when awaited, so report "ready" immediately;
  // the subsequent fiber_await drives execution. Avoids an infinite spin.
  if (!fiber->uses_thread)
    return 1;

  pthread_mutex_lock(&fiber->mutex);
  bool done = fiber->completed;
  pthread_mutex_unlock(&fiber->mutex);

  return done ? 1 : 0;
}

// iOS supports fibers but excludes the fork/exec process runtime. Keeping these
// wrappers in its archive would make ordinary fiber programs fail to link even
// when they never spawn an external process. [IOS-TARGET-CAPABILITIES]
#if !defined(OSPREY_IOS) && !defined(OSPREY_ANDROID)
extern int64_t await_process(int64_t process_id);
extern void cleanup_process(int64_t process_id);

int64_t fiber_await_process(int64_t process_id) {
  return await_process(process_id);
}

// Await process completion with stdout callback in fiber context
int64_t fiber_await_process_with_callback(int64_t process_id,
                                          void (*stdout_callback)(char *)) {
  if (!stdout_callback) {
    return await_process(process_id);
  }

  return fiber_await_process(process_id);
}

// Clean up process resources
void fiber_cleanup_process(int64_t process_id) { cleanup_process(process_id); }
#endif

#endif
