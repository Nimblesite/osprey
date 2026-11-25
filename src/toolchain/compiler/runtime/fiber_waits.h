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
#if !OSP_ONE_THREAD
static struct timespec span_of(int64_t milliseconds) {
  struct timespec span;
  span.tv_sec = (time_t)(milliseconds / 1000);
  span.tv_nsec = (long)(milliseconds % 1000) * 1000000L;
  return span;
}
#endif

static void sleep_for(int64_t milliseconds) {
#if OSP_ONE_THREAD
  osp_one_thread_sleep(milliseconds > INT64_MAX / 1000000 ? INT64_MAX : milliseconds * 1000000);
#else
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
#endif
}

// A sleeping fiber holds up no collection.
int64_t fiber_sleep(int64_t milliseconds) {
  if (milliseconds <= 0) {
    return 0;
  }
  OspRegisters registers;
  OSP_SPILL_REGISTERS(registers);
  osp_mem_baton_park(&registers);
  sleep_for(milliseconds);
  osp_mem_baton_unpark();
  return 0;
}

// Non-blocking completion probe. Returns 1 if the fiber has finished, 0 if it
// is still running, -1 for no fiber. Lets a caller animate (sleep + redraw)
// while a fiber does real work, then `fiber_await` it the instant it reports
// done - without ever blocking the animating thread on a join or condvar.
static int64_t fiber_finished(Fiber *fiber) {
  if (!fiber) {
    return -1;
  }
  // Deterministic fibers only run when awaited, so report "ready" immediately;
  // the subsequent fiber_await drives execution. Avoids an infinite spin.
  if (!fiber->uses_thread) {
    return 1;
  }
  pthread_mutex_lock(&fiber->mutex);
  bool done = fiber->completed;
  pthread_mutex_unlock(&fiber->mutex);
  return done ? 1 : 0;
}

// `fiberDone` on a program's handle, holding the fiber while it reads it.
int64_t fiber_done(void *handle) {
  Fiber *fiber = (Fiber *)held_of_handle(handle);
  int64_t done = fiber_finished(fiber);
  fiber_release(fiber);
  return done;
}

// A fiber's spawn number, 1 for the first a process spawns: what a program
// interpolating the fiber shows. -1 for no fiber. [CONCURRENCY-SPAWN-AWAIT]
int64_t fiber_id(void *handle) {
  Fiber *fiber = (Fiber *)held_of_handle(handle);
  int64_t id = fiber ? fiber->id : -1;
  fiber_release(fiber);
  return id;
}

// iOS and wasm32 support fibers but exclude the fork/exec process runtime.
// Keeping these wrappers in their archives would make ordinary fiber programs
// fail to link even when they never spawn an external process.
// [IOS-TARGET-CAPABILITIES] [CONCURRENCY-WASM]
#if !defined(OSPREY_IOS) && !defined(OSPREY_ANDROID) && !defined(__wasi__)
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
