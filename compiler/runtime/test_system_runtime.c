// Assertion-driven tests for system_runtime.c — the process runtime
// (spawn/await/cleanup with streamed callbacks) and the legacy blocking
// spawn_process. The read_file/write_file pair moved to
// test_file_runtime.c with the source it covers. Linked with memory_runtime.c
// by the Makefile's _test_c_runtime; POSIX-only harness.
#include <assert.h>
#include <errno.h>
#include <pthread.h>
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
extern char *spawn_process(char *command);

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

void test_basic_process_spawn(void) {
  printf("=== Testing Basic Process Spawn ===\n");

  int64_t process_id = spawn_process_with_handler(
      "echo 'Hello from test process'", test_event_handler);

  assert(process_id > 0);
  printf("Process spawned with ID: %lld\n", (long long)process_id);

  // Wait for completion
  int64_t exit_code = await_process(process_id);
  printf("Process completed with exit code: %lld\n", (long long)exit_code);

  assert(exit_code == 0);

  // Clean up
  cleanup_process(process_id);
  printf("Process cleaned up\n");

  printf("=== Basic Process Spawn Test PASSED ===\n\n");
}

void test_multiple_processes(void) {
  printf("=== Testing Multiple Processes ===\n");

  int64_t pid1 =
      spawn_process_with_handler("echo 'Process 1'", test_event_handler);
  int64_t pid2 =
      spawn_process_with_handler("echo 'Process 2'", test_event_handler);
  int64_t pid3 =
      spawn_process_with_handler("echo 'Process 3'", test_event_handler);

  assert(pid1 > 0);
  assert(pid2 > 0);
  assert(pid3 > 0);
  assert(pid1 != pid2);
  assert(pid2 != pid3);

  printf("Spawned processes: %lld, %lld, %lld\n", (long long)pid1,
         (long long)pid2, (long long)pid3);

  // Wait for all to complete
  int64_t exit1 = await_process(pid1);
  int64_t exit2 = await_process(pid2);
  int64_t exit3 = await_process(pid3);

  assert(exit1 == 0);
  assert(exit2 == 0);
  assert(exit3 == 0);

  // Clean up all
  cleanup_process(pid1);
  cleanup_process(pid2);
  cleanup_process(pid3);

  printf("=== Multiple Processes Test PASSED ===\n\n");
}

void test_process_with_error(void) {
  printf("=== Testing Process With Error ===\n");

  int64_t process_id = spawn_process_with_handler(
      "false", test_event_handler); // 'false' command returns exit code 1

  assert(process_id > 0);

  // Wait for completion
  int64_t exit_code = await_process(process_id);
  printf("Error process completed with exit code: %lld\n",
         (long long)exit_code);

  assert(exit_code == 1); // false command should return 1

  cleanup_process(process_id);

  printf("=== Process With Error Test PASSED ===\n\n");
}

void test_process_with_stderr(void) {
  printf("=== Testing Process With Stderr ===\n");

  int64_t process_id = spawn_process_with_handler(
      "sh -c 'echo \"error message\" >&2'", test_event_handler);

  assert(process_id > 0);

  // Wait for completion
  int64_t exit_code = await_process(process_id);

  assert(exit_code == 0);

  cleanup_process(process_id);

  printf("=== Process With Stderr Test PASSED ===\n\n");
}

void test_long_running_process(void) {
  printf("=== Testing Long Running Process ===\n");

  int64_t process_id = spawn_process_with_handler(
      "sh -c 'for i in 1 2 3; do echo \"Line $i\"; sleep 0.1; done'",
      test_event_handler);

  assert(process_id > 0);

  // Wait for completion
  int64_t exit_code = await_process(process_id);

  assert(exit_code == 0);

  cleanup_process(process_id);

  printf("=== Long Running Process Test PASSED ===\n\n");
}

void test_invalid_command(void) {
  printf("=== Testing Invalid Command ===\n");

  int64_t process_id = spawn_process_with_handler("nonexistent_command_12345",
                                                  test_event_handler);

  // Should still get a process ID (the failure happens in the child process)
  assert(process_id > 0);

  // Wait for completion - should get exit code 127 (command not found)
  int64_t exit_code = await_process(process_id);
  printf("Invalid command exit code: %lld\n", (long long)exit_code);

  assert(exit_code == 127); // Standard exit code for command not found

  cleanup_process(process_id);

  printf("=== Invalid Command Test PASSED ===\n\n");
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

// Legacy blocking spawn: exact captured output, NULL rejection, and the
// realloc growth path past the 4096-byte initial buffer.
void test_legacy_spawn_process(void) {
  assert(spawn_process(NULL) == NULL);
  char *out = spawn_process("printf 'hello'");
  assert(out != NULL && strcmp(out, "hello") == 0);
  free(out);
  char *empty = spawn_process("true");
  assert(empty != NULL && empty[0] == '\0');
  free(empty);
  char *big = spawn_process("awk 'BEGIN{for(i=0;i<5000;i++)printf \"x\"}'");
  assert(big != NULL);
  assert(strlen(big) == 5000);
  for (size_t i = 0; i < 5000; i++) {
    assert(big[i] == 'x');
  }
  free(big);
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

int main(void) {
  printf("Running System Runtime Tests...\n\n");

  // FIRST: the injected-failure seams need a process with exactly one thread
  // and no children of its own.
  run_spawn_failure_tests();

  test_basic_process_spawn();
  test_multiple_processes();
  test_process_with_error();
  test_process_with_stderr();
  test_long_running_process();
  test_invalid_command();
  test_captured_stdout_and_exit();
  test_captured_stderr_and_exact_code();
  test_process_argument_rejection();
  test_legacy_spawn_process();
  test_signalled_child_reports_minus_one();
  // LAST: exhausts the process-id space for the rest of this process.
  run_descriptor_exhaustion_tests();

  printf("=== ALL SYSTEM RUNTIME TESTS PASSED ===\n");
  return 0;
}
