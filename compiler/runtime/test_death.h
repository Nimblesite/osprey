// Forked death tests whose coverage actually counts.
//
// A fatal guard is shipped code like any other line, and the gcov gate cannot
// tell "never tested" from "untestable" — it reads 0% for both. The obstacle is
// purely mechanical: gcov flushes its counters from an `atexit` handler, and
// `abort()` does not run those. So every `fprintf(stderr, "FATAL: ...");
// abort();` arm in this runtime measured as dead code no matter how many tests
// drove it, and the honest response to a threshold is to make the measurement
// true rather than to exempt the lines.
//
// The child installs a SIGABRT handler that dumps the counters and then
// RE-RAISES with the default disposition, so the parent still observes a real
// death by SIGABRT. The assertion is not softened to make the measurement
// possible: `osp_death_signal` reports 0 for a child that returned normally,
// which is a failing result for a guard whose whole contract is that it stops.
//
// The counter dump itself is `OSP_GCOV_DUMP` from test_gcov.h, shared with the
// fork harnesses that are not death tests.
#ifndef OSPREY_TEST_DEATH_H
#define OSPREY_TEST_DEATH_H

#include <signal.h>
#include <stddef.h>
#include <sys/wait.h>
#include <unistd.h>

#include "test_gcov.h"

// Reported when the fork itself failed, so a test can say "could not observe"
// rather than silently passing on a child that never ran.
#define OSP_DEATH_UNOBSERVED (-1)

// Reported when the child outlived its budget and had to be killed. Distinct
// from every signal number, so a stalled body can never be counted as a guard
// that fired.
#define OSP_DEATH_STALLED (-2)

// The code under test.
typedef void (*OspDeathBody)(void);

// How long a child gets to either die or return before it is killed. A blocking
// `waitpid` on a child that does NEITHER parks the suite for good, and a hung
// suite is read as broken infrastructure rather than as the failing test it
// actually is -- that already cost this tree one CI hang. Generous enough that
// a loaded machine cannot trip it.
enum { OSP_DEATH_BUDGET_SECONDS = 30, OSP_DEATH_POLL_US = 2000 };

// NOT async-signal-safe, and it cannot be made so: `__gcov_dump` walks
// libgcov's own state and writes files, none of which POSIX permits from a
// handler. The residual risk is aborting inside libgcov itself, and it is
// bounded rather than argued away: a handler that wedges is a child that never
// exits, and the parent's deadline turns that into OSP_DEATH_STALLED — a
// reported failure, not a hang. It is also confined to the `--coverage` build;
// the functional build expands OSP_GCOV_DUMP to nothing.
static void osp_death_dump_and_reraise(int sig) {
  OSP_GCOV_DUMP();
  (void)signal(sig, SIG_DFL);
  (void)raise(sig);
}

// Reap `pid`, waiting at most `budget` seconds. The deadline is enforced from
// the PARENT and with SIGKILL, which no child can block, ignore or handle: an
// in-child `alarm` would be defeated by exactly the states worth defending
// against -- a body that blocks or resets SIGALRM, and a SIGABRT handler wedged
// inside libgcov, which is where this harness's own risk lives.
static inline int osp_death_reap(pid_t pid, unsigned budget) {
  unsigned long limit = (unsigned long)budget * (1000000UL / OSP_DEATH_POLL_US);
  int status = 0;
  for (unsigned long waited = 0;; waited += 1) {
    pid_t seen = waitpid(pid, &status, WNOHANG);
    if (seen == pid) {
      return WIFSIGNALED(status) ? WTERMSIG(status) : 0;
    }
    if (seen < 0) {
      return OSP_DEATH_UNOBSERVED;
    }
    if (waited >= limit) {
      (void)kill(pid, SIGKILL);
      (void)waitpid(pid, &status, 0); // the corpse is reaped, never left behind
      return OSP_DEATH_STALLED;
    }
    (void)usleep(OSP_DEATH_POLL_US);
  }
}

// Run `body` in a forked child with `budget` seconds to die or return, and
// report the signal that killed it — 0 if it ran to completion, which means the
// guard under test did NOT fire, and OSP_DEATH_STALLED if it did neither.
static inline int osp_death_signal_within(OspDeathBody body, unsigned budget) {
  pid_t pid = fork();
  if (pid < 0) {
    return OSP_DEATH_UNOBSERVED;
  }
  if (pid == 0) {
    (void)signal(SIGABRT, osp_death_dump_and_reraise);
    body();
    _exit(0); // reached only when the guard let the call through
  }
  return osp_death_reap(pid, budget);
}

// The same, under the default budget.
static inline int osp_death_signal(OspDeathBody body) {
  return osp_death_signal_within(body, OSP_DEATH_BUDGET_SECONDS);
}

#endif // OSPREY_TEST_DEATH_H
