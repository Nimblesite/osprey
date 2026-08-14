// Assertion-driven tests for file_runtime.c — the portable read_file/write_file
// pair and the failure channel in io_error.h. Split out of
// test_system_runtime.c when file_runtime.c was split out of system_runtime.c:
// a suite covers one translation unit, and both files were over the size
// budget together. Linked with memory_runtime.c by the Makefile's
// _test_c_runtime; POSIX-only harness.
#include <assert.h>
#include <errno.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#include "io_error.h"

extern int64_t write_file(char *filename, char *content);
extern char *read_file(char *filename);

#define FILE_ROUNDTRIP_PATH "/tmp/osprey_file_runtime_test.txt"

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

#define FIFO_PATH "/tmp/osprey_file_runtime_fifo"
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
  printf("Running File Runtime Tests...\n\n");

  test_file_roundtrip();
  test_read_file_non_seekable();
  test_write_file_reports_a_failed_flush();
  test_io_failures_report_a_truthful_reason();

  printf("=== ALL FILE RUNTIME TESTS PASSED ===\n");
  return 0;
}
