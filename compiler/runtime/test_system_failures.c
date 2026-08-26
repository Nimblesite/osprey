// Every failure path of the process runtime [BUILTIN-PROCESS-FAILURE]
// (docs/specs/0012-Built-InFunctions.md). Separate from the behavioural half
// only because the interposers below replace malloc, fork, pthread_create and
// pthread_mutex_init for the WHOLE binary, and may be defined in one object.
#include <assert.h>
#include <errno.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>

#include "test_alloc.h"
#include "test_spawn_seam.h"
#include "test_system_shared.h"

extern int64_t spawn_process_with_handler(const char *command,
                                          void (*handler)(int64_t, int64_t,
                                                          char *));
extern int64_t await_process(int64_t process_id);
extern void cleanup_process(int64_t process_id);
extern char *spawn_process(char *command);

// system_runtime.c caps concurrently tracked processes at MAX_PROCESSES and
// never recycles an id. The probe below must be free to burn every one of
// them, so it bounds itself well past that cap rather than guessing it.
#define MAX_PROCESSES_PROBE_LIMIT 4000

// The lowest descriptor the process could allocate right now. `dup` hands back
// exactly that, so returning it also proves the table is unchanged afterwards.
static int lowest_free_descriptor(void) {
  int probe_fd = dup(STDIN_FILENO);
  assert(probe_fd >= 0);
  assert(close(probe_fd) == 0);
  return probe_fd;
}

// The handle number the NEXT spawn will use. Handle numbers only ever
// increase, so one successful spawn reveals it -- and this is the only honest
// way to ask whether a FAILED spawn left a table slot occupied. Awaiting the
// failure CODE proves nothing: -5 is not a handle, and await_process rejects
// it on the range check without ever looking at the table.
static int64_t next_handle_number(void) {
  int64_t probe = spawn_process_with_handler("true", capture_handler);
  assert(probe > 0);
  assert(await_process(probe) == 0);
  cleanup_process(probe);
  return probe + 1;
}

// A spawn that parks instead of returning is not a slow test, it is a
// deadlocked one -- the runtime holds process_mutex across its teardown, so
// nothing else will ever run either. SIGALRM's default disposition ends the
// process, which turns an infinite hang into a reported death.
enum { SPAWN_STALL_BUDGET_SECONDS = 15, COMMAND_MAX = 256 };

// Somewhere writable for the marker files the adversaries below hand back.
static const char *scratch_directory(void) {
  const char *dir = getenv("TMPDIR");
  return (dir == NULL || dir[0] == '\0') ? "/tmp" : dir;
}

// Leave `spare` descriptors available for the duration of `probe`, capping
// RLIMIT_NOFILE so every allocation past them fails with EMFILE and nothing
// already open changes. This is the only portable way to reach the
// out-of-descriptors branches: no argument can provoke them, and they are the
// ones that run when a long-lived program finally exhausts its table.
static void with_descriptor_budget(int spare, void (*probe)(void)) {
  int probe_fd = lowest_free_descriptor();

  struct rlimit saved;
  assert(getrlimit(RLIMIT_NOFILE, &saved) == 0);
  struct rlimit tight = saved;
  tight.rlim_cur = (rlim_t)(probe_fd + spare);
  assert(setrlimit(RLIMIT_NOFILE, &tight) == 0);

  probe();

  assert(setrlimit(RLIMIT_NOFILE, &saved) == 0);
}

// popen cannot get a descriptor, so the legacy blocking spawn reports failure
// rather than reading from a pipe it never opened.
static void probe_legacy_spawn_without_descriptors(void) {
  assert(spawn_process("echo unreachable") == NULL);
}

// Every id burned here is burned for the rest of the process, so this runs
// LAST. `next_process_id` only ever increments — cleanup_process frees the
// slot but never returns the id — so a program that has spawned MAX_PROCESSES
// times can never spawn again even with every slot free. The -2 below is that
// ceiling, reached without a single fork because the descriptor cap fails each
// attempt at the pipe, one id later.
static void probe_spawn_exhausts_descriptors_then_ids(void) {
  int saw_pipe_failure = 0;
  int saw_id_exhaustion = 0;
  for (int attempt = 0; attempt < MAX_PROCESSES_PROBE_LIMIT; attempt++) {
    int64_t result = spawn_process_with_handler("true", capture_handler);
    assert(result == -4 || result == -2); // never a live process id
    if (result == -4) {
      saw_pipe_failure = 1;
    } else {
      saw_id_exhaustion = 1;
      break;
    }
  }
  assert(saw_pipe_failure); // pipe() denied, reported as -4
  assert(saw_id_exhaustion); // ids ran out, reported as -2
  // Failure ordering, now that two preconditions are broken at once: the
  // argument check runs first, so a caller who passed nothing is told that
  // rather than being blamed for the exhausted handle space.
  assert(spawn_process_with_handler(NULL, capture_handler) == -1 &&
         "a missing command is rejected before the capacity check");
  assert(spawn_process_with_handler("true", NULL) == -1 &&
         "and so is a missing callback");
}

// Two descriptors is exactly enough for stdout's pipe and one short for
// stderr's.
#define ONE_PIPE_WORTH_OF_DESCRIPTORS 2

static void probe_spawn_gets_only_one_pipe(void) {
  assert(spawn_process_with_handler("true", capture_handler) == -4 &&
         "a spawn that cannot open both pipes reports -4");
  assert(spawn_process_with_handler("true", capture_handler) == -4 &&
         "and it stays -4: the first attempt returned what it took");
}

// A spawn that opens the first pipe and fails on the second must give BOTH
// descriptors back. Folding the two pipe() calls into one short-circuiting
// condition leaks the pair that succeeded, and a long-lived program that keeps
// spawning under descriptor pressure then runs its table down to nothing while
// every call politely reports -4. Implements [BUILTIN-PROCESS-FAILURE].
static void test_partial_pipe_failure_returns_its_descriptors(void) {
  int before = lowest_free_descriptor();
  with_descriptor_budget(ONE_PIPE_WORTH_OF_DESCRIPTORS,
                         probe_spawn_gets_only_one_pipe);
  int after = lowest_free_descriptor();
  assert(after == before &&
         "a failed spawn must return every descriptor it took");
}

// The -6 arm is the only teardown that has already opened BOTH pipes, so it
// has the most to give back. `test_fork.h` denies fork in THIS process, which
// is the only way the descriptor inventory below means anything: a forked
// child's table is a different table and stays intact however much that child
// leaks. Implements [BUILTIN-PROCESS-FAILURE].
static void test_fork_failure_releases_everything_it_took(void) {
  int before = lowest_free_descriptor();
  int64_t attempted = next_handle_number();

  osp_fork_deny();
  assert(spawn_process_with_handler("true", capture_handler) == -6 &&
         "a spawn whose fork fails reports -6");
  assert(spawn_process_with_handler("true", capture_handler) == -6 &&
         "and keeps reporting it rather than latching a broken state");
  assert(lowest_free_descriptor() == before &&
         "both pipes come back in the same process that opened them");
  osp_fork_allow();

  // Removing the injected failure must leave a runtime that still works: the
  // refusals cost handle numbers and nothing else.
  int64_t recovered = spawn_process_with_handler("true", capture_handler);
  assert(recovered == attempted + 2 &&
         "a spawn after the failure is removed succeeds, two handles on");
  assert(await_process(recovered) == 0);
  cleanup_process(recovered);
  assert(lowest_free_descriptor() == before &&
         "and the recovered spawn returns its descriptors too");
  assert(await_process(recovered) == -1 &&
         "the table slot it used is free again, not permanently occupied");
  assert(await_process(attempted) == -1 &&
         "and neither refusal left a record behind in the slot it skipped");
}

// A record the runtime cannot allocate, and a command string it cannot copy,
// are the same failure: -3, nothing kept, and no half-built record left in the
// table. The copy is the one that used to go unchecked, leaving `command` NULL
// on a record the monitor thread and every diagnostic then read.
// Implements [BUILTIN-PROCESS-FAILURE].
static void test_record_allocation_failures_are_reported(void) {
  long live_before = osp_alloc_live();

  osp_alloc_fail_next(); // the record itself
  assert(spawn_process_with_handler("true", capture_handler) == -3 &&
         "a record that cannot be allocated reports -3");
  assert(osp_alloc_live() == live_before && "and keeps nothing");

  osp_alloc_fail_after(1); // the record, then the command copy
  assert(spawn_process_with_handler("true", capture_handler) == -3 &&
         "a command that cannot be copied reports -3 as well");
  osp_alloc_fail_off();
  assert(osp_alloc_live() == live_before &&
         "and frees the record it had already taken");

  int64_t recovered = spawn_process_with_handler("true", capture_handler);
  assert(recovered > 0 && "the runtime still spawns once memory comes back");
  assert(await_process(recovered) == 0);
  cleanup_process(recovered);
}

// The record's own mutex failing to initialise is the one teardown that must
// NOT destroy it: destroying a mutex that was never initialised is undefined,
// and the record is freed on its own. Implements [BUILTIN-PROCESS-FAILURE].
static void test_mutex_initialisation_failure_is_reported(void) {
  long live_before = osp_alloc_live();
  int64_t attempted = next_handle_number();

  osp_mutex_init_deny();
  assert(spawn_process_with_handler("true", capture_handler) == -3 &&
         "a record whose mutex cannot be initialised reports -3");
  osp_mutex_init_allow();

  assert(osp_alloc_live() == live_before && "and the record is freed");
  assert(await_process(attempted) == -1 &&
         "the slot it would have used is empty");
  assert(next_handle_number() == attempted + 2 &&
         "exactly one handle number was consumed by the refusal");
}

// The -5 arm is the only teardown holding a real child process: without a
// monitor nothing will ever reap it, so ending it there is part of the
// contract, not tidiness. Implements [BUILTIN-PROCESS-FAILURE].
static void test_thread_failure_reaps_the_child_it_started(void) {
  int before = lowest_free_descriptor();
  int64_t attempted = next_handle_number();

  // The adversary: a child that IGNORES SIGTERM and then says so. A teardown
  // that asks politely and waits without a bound parks here forever, still
  // holding process_mutex, and takes every later spawn in this program with
  // it. The seam waits for the marker before refusing the thread, so the
  // teardown's signal is guaranteed to arrive AFTER the trap is installed --
  // otherwise it lands on a child still in the pre-exec code, where the
  // default disposition kills it and the adversary proves nothing. The alarm
  // turns the hang this is hunting into a death the harness reports.
  char marker[COMMAND_MAX];
  char adversary[COMMAND_MAX];
  int n = snprintf(marker, sizeof(marker), "%s/osprey-term-ignorer-%ld",
                   scratch_directory(), (long)getpid());
  assert(n > 0 && (size_t)n < sizeof(marker));
  (void)unlink(marker);
  n = snprintf(adversary, sizeof(adversary),
               "trap '' TERM; : > %s; sleep 30", marker);
  assert(n > 0 && (size_t)n < sizeof(adversary));

  alarm(SPAWN_STALL_BUDGET_SECONDS);
  osp_thread_deny_when_ready(marker);
  int64_t refused = spawn_process_with_handler(adversary, capture_handler);
  osp_thread_allow();
  assert(alarm(0) > 0 && "the refusal must return well inside its budget");
  assert(refused == -5 && "a spawn whose monitor cannot start reports -5");
  assert(access(marker, F_OK) == 0 &&
         "the adversary really did reach its trap before it was ended");
  assert(unlink(marker) == 0);

  assert(lowest_free_descriptor() == before &&
         "both pipes come back from the -5 path too");
  assert(await_process(attempted) == -1 &&
         "the slot it would have used is empty, not a freed record");
  // Nothing left to reap. A child kept alive here would outlive the program it
  // was spawned for, still holding the pipes it was given.
  int status = 0;
  assert(waitpid(-1, &status, WNOHANG) == -1 && errno == ECHILD &&
         "the unmonitorable child was killed and collected before returning");

  int64_t recovered = spawn_process_with_handler("true", capture_handler);
  assert(recovered == attempted + 1 &&
         "the runtime spawns again, one handle number past the refusal");
  assert(await_process(recovered) == 0);
  cleanup_process(recovered);
  assert(lowest_free_descriptor() == before);
}

void run_spawn_failure_tests(void) {
  test_record_allocation_failures_are_reported();
  test_mutex_initialisation_failure_is_reported();
  test_thread_failure_reaps_the_child_it_started();
  test_partial_pipe_failure_returns_its_descriptors();
  test_fork_failure_releases_everything_it_took();
}

void run_descriptor_exhaustion_tests(void) {
  with_descriptor_budget(0, probe_legacy_spawn_without_descriptors);
  with_descriptor_budget(0, probe_spawn_exhausts_descriptors_then_ids);
}
