// Lifecycle regressions implement [BUILTIN-PROCESS-FAILURE] on the shared await path.
// Assertion-driven tests for system_runtime.c — the process runtime
// (spawn/await/cleanup with streamed callbacks). The read_file/write_file pair moved to
// test_file_runtime.c with the source it covers. Linked with memory_runtime.c
// by the Makefile's _test_c_runtime; POSIX-only harness.
#include <assert.h>
#include <errno.h>
#include <pthread.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>

#include "test_system_shared.h"

// Include the system runtime header (we'll define the interface)
extern int64_t spawn_process_with_handler(const char *command,
                                          void (*handler)(int64_t, int64_t,
                                                          char *));
extern int64_t await_process(int64_t process_id);
extern void cleanup_process(int64_t process_id);

// Test event handler data
typedef struct {
  int stdout_count;
  int stderr_count;
  int exit_count;
  char last_stdout[1024];
  char last_stderr[1024];
  int64_t last_exit_code;
  pthread_mutex_t mutex;
} TestEventData;

// Test event handler
static void test_event_handler(int64_t process_id, int64_t event_type,
                               char *data) {
  // This would be passed in from test, but for now we'll use a global
  static TestEventData test_data = {0};
  static int initialized = 0;

  if (!initialized) {
    pthread_mutex_init(&test_data.mutex, NULL);
    initialized = 1;
  }

  pthread_mutex_lock(&test_data.mutex);

  switch (event_type) {
  case 1: // PROCESS_STDOUT_DATA
    test_data.stdout_count++;
    strncpy(test_data.last_stdout, data, sizeof(test_data.last_stdout) - 1);
    test_data.last_stdout[sizeof(test_data.last_stdout) - 1] = '\0';
    printf("TEST: Got stdout from process %lld: %s", (long long)process_id,
           data);
    break;
  case 2: // PROCESS_STDERR_DATA
    test_data.stderr_count++;
    strncpy(test_data.last_stderr, data, sizeof(test_data.last_stderr) - 1);
    test_data.last_stderr[sizeof(test_data.last_stderr) - 1] = '\0';
    printf("TEST: Got stderr from process %lld: %s", (long long)process_id,
           data);
    break;
  case 3: // PROCESS_EXIT
    test_data.exit_count++;
    test_data.last_exit_code = atoll(data);
    printf("TEST: Process %lld exited with code: %lld\n", (long long)process_id,
           (long long)test_data.last_exit_code);
    break;
  default:
    printf("TEST: Unknown event type %lld from process %lld\n", (long long)event_type,
           (long long)process_id);
    break;
  }

  pthread_mutex_unlock(&test_data.mutex);
}

// Each command runs to completion and reports its documented exit code: a
// plain echo, `false`, stderr-only output, a streamed sleeper, and a missing
// command (127, the failure surfaces in the child).
static const struct {
  const char *command;
  int64_t exit_code;
} EXIT_CASES[] = {
    {"echo 'Hello from test process'", 0},
    {"false", 1},
    {"sh -c 'echo \"error message\" >&2'", 0},
    {"sh -c 'for i in 1 2 3; do echo \"Line $i\"; sleep 0.1; done'", 0},
    {"nonexistent_command_12345", 127},
};

void test_exit_codes(void) {
  for (size_t i = 0; i < sizeof EXIT_CASES / sizeof EXIT_CASES[0]; i++) {
    int64_t process_id =
        spawn_process_with_handler(EXIT_CASES[i].command, test_event_handler);
    assert(process_id > 0);
    assert(await_process(process_id) == EXIT_CASES[i].exit_code);
    cleanup_process(process_id);
  }
}

// Three processes run at once under distinct ids and all succeed.
void test_multiple_processes(void) {
  int64_t pids[3];
  char command[32];
  for (int i = 0; i < 3; i++) {
    snprintf(command, sizeof command, "echo 'Process %d'", i + 1);
    pids[i] = spawn_process_with_handler(command, test_event_handler);
    assert(pids[i] > 0 && (i == 0 || pids[i] != pids[i - 1]));
  }
  for (int i = 0; i < 3; i++) {
    assert(await_process(pids[i]) == 0);
    cleanup_process(pids[i]);
  }
}

// --- content-exact event capture ---------------------------------------------
// The handler above only counts; this one ACCUMULATES payloads so tests can
// assert the exact bytes a child wrote and the exact exit code it reported.

typedef struct {
  char out[4096];
  char err[4096];
  int exit_events;
  int64_t exit_code;
  pthread_mutex_t mutex;
} CaptureData;

static CaptureData g_capture = {.mutex = PTHREAD_MUTEX_INITIALIZER};

static void capture_reset(void) {
  pthread_mutex_lock(&g_capture.mutex);
  g_capture.out[0] = '\0';
  g_capture.err[0] = '\0';
  g_capture.exit_events = 0;
  g_capture.exit_code = -12345;
  pthread_mutex_unlock(&g_capture.mutex);
}

void capture_handler(int64_t process_id, int64_t event_type, char *data) {
  (void)process_id;
  pthread_mutex_lock(&g_capture.mutex);
  switch (event_type) {
  case 1: // PROCESS_STDOUT_DATA
    strncat(g_capture.out, data,
            sizeof(g_capture.out) - strlen(g_capture.out) - 1);
    break;
  case 2: // PROCESS_STDERR_DATA
    strncat(g_capture.err, data,
            sizeof(g_capture.err) - strlen(g_capture.err) - 1);
    break;
  case 3: // PROCESS_EXIT
    g_capture.exit_events++;
    g_capture.exit_code = atoll(data);
    break;
  default:
    break;
  }
  pthread_mutex_unlock(&g_capture.mutex);
}

// The streamed stdout payload arrives byte-exact, the exit event fires exactly
// once, and its code matches the handler-observed value AND await's result.
void test_captured_stdout_and_exit(void) {
  capture_reset();
  int64_t pid = spawn_process_with_handler("printf 'abc'", capture_handler);
  assert(pid > 0);
  int64_t exit_code = await_process(pid);
  assert(exit_code == 0);
  pthread_mutex_lock(&g_capture.mutex);
  assert(strcmp(g_capture.out, "abc") == 0); // exact bytes, no newline added
  assert(g_capture.err[0] == '\0');
  assert(g_capture.exit_events == 1);
  assert(g_capture.exit_code == 0);
  pthread_mutex_unlock(&g_capture.mutex);
  cleanup_process(pid);
}

// Stderr streams on its own event type, and a nonzero exit code is reported
// EXACTLY (both through await and through the exit event).
void test_captured_stderr_and_exact_code(void) {
  capture_reset();
  int64_t pid = spawn_process_with_handler("sh -c 'printf err >&2; exit 7'",
                                           capture_handler);
  assert(pid > 0);
  assert(await_process(pid) == 7);
  pthread_mutex_lock(&g_capture.mutex);
  assert(strcmp(g_capture.err, "err") == 0);
  assert(g_capture.out[0] == '\0');
  assert(g_capture.exit_events == 1);
  assert(g_capture.exit_code == 7);
  pthread_mutex_unlock(&g_capture.mutex);
  cleanup_process(pid);
}

// Invalid arguments and unknown ids are rejected with exact codes; cleanup
// tolerates every invalid id and await-after-cleanup reports -1.
void test_process_argument_rejection(void) {
  assert(spawn_process_with_handler(NULL, capture_handler) == -1);
  assert(spawn_process_with_handler("echo x", NULL) == -1);
  assert(await_process(0) == -1);
  assert(await_process(-3) == -1);
  assert(await_process(999999) == -1);
  assert(await_process(999) == -1); // in-range but never spawned
  cleanup_process(0);
  cleanup_process(-1);
  cleanup_process(999999);
  capture_reset();
  int64_t pid = spawn_process_with_handler("true", capture_handler);
  assert(pid > 0);
  assert(await_process(pid) == 0);
  cleanup_process(pid);
  assert(await_process(pid) == -1); // the slot is gone after cleanup
}

// A child killed by a SIGNAL never carries an exit status, so WIFEXITED is
// false and the status word holds the signal number rather than a code. The
// runtime must report the documented -1 for it — reading WEXITSTATUS of a
// signalled status yields whatever the low byte happens to hold, which for
// SIGKILL is a plain 9 and is indistinguishable from `exit 9`.
//
// The command signals the tracked process ITSELF: the runtime runs it as
// `/bin/sh -c`, so `$$` is exactly the pid being waited on. Wrapping it in a
// second `sh -c` instead would only work where the outer shell exec's the
// inner one — bash does, dash does not, so on Linux the tracked shell would
// survive its child and exit NORMALLY with 137.
static void test_signalled_child_reports_minus_one(void) {
  capture_reset();
  int64_t pid = spawn_process_with_handler("kill -9 $$", capture_handler);
  assert(pid > 0);
  assert(await_process(pid) == -1);
  pthread_mutex_lock(&g_capture.mutex);
  assert(g_capture.exit_events == 1);
  assert(g_capture.exit_code == -1); // not the signal number, not 0
  pthread_mutex_unlock(&g_capture.mutex);
  cleanup_process(pid);
}

static bool buffered_child_finished;

static void assert_buffered_stream(const char *data, char terminal) {
  char expected[3073];
  memset(expected, ' ', sizeof(expected) - 2);
  expected[sizeof(expected) - 2] = terminal;
  expected[sizeof(expected) - 1] = '\0';
  assert(strcmp(data, expected) == 0);
}

static void await_buffered_child(void) {
  // Leave the exited child waitable: the monitor must reap it itself.
  siginfo_t info;
  int result;
  do {
    result = waitid(P_ALL, 0, &info, WEXITED | WNOWAIT);
  } while (result < 0 && errno == EINTR);
  assert(result == 0);
  buffered_child_finished = true;
}

static void buffered_output_handler(int64_t pid, int64_t event, char *data) {
  if (!buffered_child_finished)
    await_buffered_child();
  assert(g_capture.exit_events == 0);
  if (event == 3) {
    assert_buffered_stream(g_capture.out, 'O');
    assert_buffered_stream(g_capture.err, 'E');
  }
  capture_handler(pid, event, data);
}

static void test_exit_drains_buffered_output(void) {
  capture_reset();
  buffered_child_finished = false;
  int64_t pid = spawn_process_with_handler(
      "printf '%3072s' O; printf '%3072s' E >&2; exit 7",
      buffered_output_handler);
  assert(pid > 0);
  assert(await_process(pid) == 7);
  assert(g_capture.exit_events == 1);
  assert(g_capture.exit_code == 7);
  cleanup_process(pid);
}

int main(int argc, char **argv) {
  if (argc == 2 && strcmp(argv[1], "--buffered-output-only") == 0) {
    test_exit_drains_buffered_output();
    return 0;
  }
  printf("Running System Runtime Tests...\n\n");

  // FIRST: the injected-failure seams need a process with exactly one thread
  // and no children of its own.
  run_spawn_failure_tests();

  test_exit_codes();
  test_multiple_processes();
  test_captured_stdout_and_exit();
  test_captured_stderr_and_exact_code();
  test_process_argument_rejection();
  test_signalled_child_reports_minus_one();
  test_exit_drains_buffered_output();
  // LAST: exhausts the process-id space for the rest of this process.
  run_descriptor_exhaustion_tests();

  printf("=== ALL SYSTEM RUNTIME TESTS PASSED ===\n");
  return 0;
}
