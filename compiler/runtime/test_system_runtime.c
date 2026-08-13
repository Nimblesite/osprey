// Assertion-driven tests for system_runtime.c — the process runtime
// (spawn/await/cleanup with streamed callbacks), the legacy blocking
// spawn_process, and the portable read_file/write_file pair. Linked with
// memory_runtime.c by the Makefile's _test_c_runtime; POSIX-only harness.
#include <assert.h>
#include <errno.h>
#include <pthread.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#include "io_error.h"

// Include the system runtime header (we'll define the interface)
extern int64_t spawn_process_with_handler(const char *command,
                                          void (*handler)(int64_t, int64_t,
                                                          char *));
extern int64_t await_process(int64_t process_id);
extern void cleanup_process(int64_t process_id);
extern char *spawn_process(char *command);
extern int64_t write_file(char *filename, char *content);
extern char *read_file(char *filename);

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

static void capture_handler(int64_t process_id, int64_t event_type,
                            char *data) {
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

#define FILE_ROUNDTRIP_PATH "/tmp/osprey_system_runtime_test.txt"

// write_file returns the byte count written; read_file returns the exact
// content; NULLs and missing files are rejected; rewrites truncate.
void test_file_roundtrip(void) {
  assert(write_file(NULL, "x") == -1);
  assert(write_file(FILE_ROUNDTRIP_PATH, NULL) == -1);
  assert(read_file(NULL) == NULL);
  assert(read_file("/nonexistent_osprey_dir/nope.txt") == NULL);
  assert(write_file("/nonexistent_osprey_dir/nope.txt", "x") == -2);
  const char *content = "line one\nline two\n";
  assert(write_file(FILE_ROUNDTRIP_PATH, (char *)(uintptr_t)content) ==
         (int64_t)strlen(content));
  char *back = read_file(FILE_ROUNDTRIP_PATH);
  assert(back != NULL && strcmp(back, content) == 0);
  free(back);
  assert(write_file(FILE_ROUNDTRIP_PATH, (char *)(uintptr_t) "short") == 5);
  char *truncated = read_file(FILE_ROUNDTRIP_PATH);
  assert(truncated != NULL && strcmp(truncated, "short") == 0); // truncated
  free(truncated);
  assert(write_file(FILE_ROUNDTRIP_PATH, (char *)(uintptr_t) "") == 0);
  char *emptied = read_file(FILE_ROUNDTRIP_PATH);
  assert(emptied != NULL && emptied[0] == '\0');
  free(emptied);
  assert(remove(FILE_ROUNDTRIP_PATH) == 0);
}

#define FIFO_PATH "/tmp/osprey_system_runtime_fifo"
// Comfortably past any pipe buffer, so the writer below cannot finish before
// the reader disappears, and past any malloc slack, so an undersized
// destination buffer corrupts the heap instead of getting away with it.
#define FIFO_PAYLOAD_BYTES 1048576

// Fork a child running `body` against FIFO_PATH and return its pid; the caller
// reaps it. The FIFO is created here so neither side can race its existence.
static pid_t fork_fifo_peer(void (*body)(void)) {
  (void)remove(FIFO_PATH);
  assert(mkfifo(FIFO_PATH, 0600) == 0);
  pid_t peer = fork();
  assert(peer >= 0);
  if (peer == 0) {
    body();
    _exit(0);
  }
  return peer;
}

static void fifo_write_payload(void) {
  FILE *out = fopen(FIFO_PATH, "w");
  if (out != NULL) {
    for (size_t i = 0; i < FIFO_PAYLOAD_BYTES; i++) {
      (void)fputc('A', out);
    }
    (void)fclose(out);
  }
}

// read_file must survive a NON-SEEKABLE stream. fseek/ftell both fail on a
// FIFO and ftell reports -1; sizing the destination from it allocated
// malloc((size_t)-1 + 1) — ZERO bytes — and then fread((size_t)-1) into it,
// overflowing the heap by however many bytes the writer chose to send. The
// length must come from what was actually read, never from a seek that a
// stream is entitled to refuse. Implements [BUILTIN-FILE].
static void test_read_file_non_seekable(void) {
  pid_t writer = fork_fifo_peer(fifo_write_payload);
  char *content = read_file(FIFO_PATH);
  int status = 0;
  (void)waitpid(writer, &status, 0);
  assert(content != NULL);
  assert(strlen(content) == FIFO_PAYLOAD_BYTES);
  for (size_t i = 0; i < FIFO_PAYLOAD_BYTES; i++) {
    assert(content[i] == 'A');
  }
  free(content);
  assert(remove(FIFO_PATH) == 0);
}

static void fifo_reader_hangs_up(void) {
  FILE *in = fopen(FIFO_PATH, "r");
  if (in != NULL) {
    usleep(50000); // let the writer's fopen return and fill the pipe buffer
    (void)fclose(in);
  }
}

// A write that does not reach its destination must NOT report success. stdio
// buffers, so the bytes leave for the file at flush time — which is fclose —
// and a write_file that returns fwrite's count without checking it against the
// requested length, and drops fclose's status entirely, reports a successful
// write of data that was never stored. Silent data loss. Implements
// [BUILTIN-FILE].
static void test_write_file_reports_a_failed_flush(void) {
  void (*previous)(int) = signal(SIGPIPE, SIG_IGN);
  pid_t reader = fork_fifo_peer(fifo_reader_hangs_up);
  char *payload = malloc(FIFO_PAYLOAD_BYTES + 1);
  assert(payload != NULL);
  memset(payload, 'B', FIFO_PAYLOAD_BYTES);
  payload[FIFO_PAYLOAD_BYTES] = '\0';
  int64_t written = write_file(FIFO_PATH, payload);
  int status = 0;
  (void)waitpid(reader, &status, 0);
  free(payload);
  assert(written < 0);
  const char *reason = osp_io_error();
  assert(reason != NULL);
  assert(strstr(reason, strerror(EPIPE)) != NULL);
  (void)signal(SIGPIPE, previous);
  assert(remove(FIFO_PATH) == 0);
}

// Every failure carries WHY it failed, and every success retires the previous
// reason so a later failure cannot inherit a stale one. Without this the
// Osprey-level `Error { message }` reads the placeholder word "Error" and a
// missing directory is indistinguishable from a permissions denial or a full
// disk. Implements [BUILTIN-FILE-ERRMSG].
static void test_io_failures_report_a_truthful_reason(void) {
  const char *missing = "/nonexistent_osprey_dir/nope.txt";
  assert(read_file((char *)(uintptr_t)missing) == NULL);
  const char *read_reason = osp_io_error();
  assert(read_reason != NULL);
  assert(strstr(read_reason, missing) != NULL);
  assert(strstr(read_reason, strerror(ENOENT)) != NULL);

  assert(write_file((char *)(uintptr_t)missing, (char *)(uintptr_t) "x") == -2);
  const char *write_reason = osp_io_error();
  assert(write_reason != NULL);
  assert(strstr(write_reason, missing) != NULL);
  assert(strstr(write_reason, strerror(ENOENT)) != NULL);

  // A success must leave nothing behind for the next failure to borrow.
  assert(write_file(FILE_ROUNDTRIP_PATH, (char *)(uintptr_t) "ok") == 2);
  assert(osp_io_error() == NULL);
  char *back = read_file(FILE_ROUNDTRIP_PATH);
  assert(back != NULL && strcmp(back, "ok") == 0);
  assert(osp_io_error() == NULL);
  free(back);
  assert(remove(FILE_ROUNDTRIP_PATH) == 0);

  // A rejected argument is a failure like any other and owes a reason too.
  assert(write_file(NULL, (char *)(uintptr_t) "x") == -1);
  assert(osp_io_error() != NULL);
  assert(read_file(NULL) == NULL);
  assert(osp_io_error() != NULL);
}

int main(void) {
  printf("Running System Runtime Tests...\n\n");

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
  test_file_roundtrip();
  test_read_file_non_seekable();
  test_write_file_reports_a_failed_flush();
  test_io_failures_report_a_truthful_reason();

  printf("=== ALL SYSTEM RUNTIME TESTS PASSED ===\n");
  return 0;
}
