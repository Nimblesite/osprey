// A program started by `osprey --run` ends when its runner dies [RUN-PARENT-DEATH]
// (docs/specs/tooling/TestRunner.md). The runner waits for the
// program, so killing the runner alone used to leave the program running with
// nobody watching it, and test runners kill exactly that one process. The
// runner names itself in OSPREY_RUN_PARENT; each memory backend calls
// run_parent_watch from osp_mem_boot, the first thing a compiled main does.
#ifndef OSPREY_RUN_PARENT_WATCH_H
#define OSPREY_RUN_PARENT_WATCH_H

#if (defined(__APPLE__) || defined(__linux__)) && !defined(OSP_SYMBOLIC)
#include <errno.h>
#include <pthread.h>
#include <stdio.h>
#include <signal.h>
#include <stdint.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>
#if defined(__linux__)
#include <sys/prctl.h>
#else
#include <dispatch/dispatch.h>
#endif

#define OSP_RUN_ORPHANED_STATUS 1

// Broken-code quarantine [FAILURE-EXPLICIT]: nobody is left to read this
// program's result, so it stops without running anything further.
static void run_parent_died(void *unused) {
  (void)unused;
  _exit(OSP_RUN_ORPHANED_STATUS);
}

#define OSP_RUN_POLL_NSEC 50000000L

// The fallback watch when the system offers none: look every 50 ms, and end
// once this process is no longer the runner's child.
static void *run_parent_poll(void *named) {
  pid_t runner = (pid_t)(intptr_t)named;
  const struct timespec pause = {0, OSP_RUN_POLL_NSEC};
  while (getppid() == runner) {
    (void)nanosleep(&pause, NULL);
  }
  run_parent_died(NULL);
  return NULL;
}

// Start the poll on its own thread. A process that cannot start even that is
// left with no watch, so it says so rather than run on silently.
static inline void run_parent_poll_start(pid_t runner) {
  pthread_t poll;
  if (pthread_create(&poll, NULL, run_parent_poll, (void *)(intptr_t)runner) == 0) {
    (void)pthread_detach(poll);
  } else {
    (void)fputs("osprey: cannot watch the runner named in OSPREY_RUN_PARENT; this program will outlive it\n", stderr);
  }
}

// Ask the system to end this process when `runner` exits, and poll when the
// system refuses ([RUN-PARENT-DEATH]): a watch that silently arms nothing is
// the orphan this exists to prevent.
static inline void run_parent_arm(pid_t runner) {
#if defined(__linux__)
  if (prctl(PR_SET_PDEATHSIG, SIGKILL) != 0) {
    run_parent_poll_start(runner);
  }
#else
  dispatch_source_t watch = dispatch_source_create(DISPATCH_SOURCE_TYPE_PROC, (uintptr_t)runner, DISPATCH_PROC_EXIT,
                                                   dispatch_get_global_queue(QOS_CLASS_UTILITY, 0));
  if (watch) {
    dispatch_source_set_event_handler_f(watch, run_parent_died);
    dispatch_resume(watch);
  } else {
    run_parent_poll_start(runner);
  }
#endif
}

static inline void run_parent_watch(void) {
  const char *named = getenv("OSPREY_RUN_PARENT");
  long runner = named ? strtol(named, NULL, 10) : 0;
  if (runner <= 1) {
    return;
  }
  (void)unsetenv("OSPREY_RUN_PARENT"); // a process this program starts has another parent
  if (getppid() == (pid_t)runner) {
    run_parent_arm((pid_t)runner);
  } else if (kill((pid_t)runner, 0) == 0 || errno != ESRCH) {
    return; // the named runner exists (even one this user may not signal) and is not this program's parent
  }
  if (getppid() != (pid_t)runner) {
    run_parent_died(NULL); // the runner died before the watch was armed
  }
}
#else
static inline void run_parent_watch(void) {}
#endif

#endif
