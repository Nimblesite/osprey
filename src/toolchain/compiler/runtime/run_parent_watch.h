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
#include <signal.h>
#include <stdint.h>
#include <stdlib.h>
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

// Ask the system to end this process when `runner` exits.
static void run_parent_arm(pid_t runner) {
#if defined(__linux__)
  (void)runner;
  (void)prctl(PR_SET_PDEATHSIG, SIGKILL);
#else
  dispatch_source_t watch = dispatch_source_create(DISPATCH_SOURCE_TYPE_PROC, (uintptr_t)runner, DISPATCH_PROC_EXIT,
                                                   dispatch_get_global_queue(QOS_CLASS_UTILITY, 0));
  if (watch) {
    dispatch_source_set_event_handler_f(watch, run_parent_died);
    dispatch_resume(watch);
  }
#endif
}

static void run_parent_watch(void) {
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
static void run_parent_watch(void) {}
#endif

#endif
