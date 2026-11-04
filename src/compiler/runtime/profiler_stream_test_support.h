// A minimal live-attach client for the endpoint tests [PROF-ATTACH-PROTOCOL]:
// it writes request lines and reads NDJSON the way the language server does,
// and checks every `cpu_batch` against the tables earlier batches built.
#ifndef OSPREY_PROFILER_STREAM_TEST_SUPPORT_H
#define OSPREY_PROFILER_STREAM_TEST_SUPPORT_H

#include "profiler_runtime.h"

#include <assert.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

bool osp_prof_start(const char *out_path, uint32_t rate_hz);
void osp_prof_stop_and_dump(void);

// snprintf into an array that must fit: a truncated path would test the wrong
// file, so a too-long result fails the test instead.
#define TEST_FMT(buf, ...) \
  assert(test_fits(snprintf((buf), sizeof(buf), __VA_ARGS__), sizeof(buf)))
static inline bool test_fits(int n, size_t cap) {
  return n > 0 && (size_t)n < cap;
}

// Name each test before it runs, so a failed assertion in a shared helper
// still says which test it belongs to.
#define TEST_RUN(test) (fprintf(stderr, "# %s\n", #test), test())

typedef struct TestConn {
  int fd;
  char *buf;
  size_t len, cap;
} TestConn;

// What a session's batches added up to: the thread rows and stacks they
// defined, the samples they carried, and whether `label` named a row.
typedef struct TestStream {
  size_t rows, stacks, samples;
  unsigned last_tick;
  bool saw_label;
} TestStream;

// Point TMPDIR at a fresh private directory for this process.
void test_use_private_tmpdir(void);
// Remove every directory test_use_private_tmpdir made, and what is in it.
void test_remove_private_tmpdirs(void);
// This process's socket: `$TMPDIR/osprey-diagnostics-<uid>/<pid>.sock`.
void test_socket_path(char *out, size_t cap);
void test_socket_path_of(long long pid, char *out, size_t cap);
TestConn test_connect(void);
TestConn test_connect_to(const char *path);
void test_close(TestConn *c);
void test_send(TestConn *c, const char *line);
// The next line, or NULL on EOF or after 10 s; the caller frees it.
char *test_next_line(TestConn *c);
char *test_request(TestConn *c, const char *line);
// Send `line` and assert the reply contains `needle`.
void test_expect_error(TestConn *c, const char *line, const char *needle);
// Repeat `line` until it starts a session: a disconnect that ended the last
// one is handled by the listener on its own schedule.
char *test_retry_start(TestConn *c, const char *line);
// Read batches until `min_samples` samples arrived and `label` named a row.
TestStream test_stream_until(TestConn *c, size_t min_samples, const char *label);
// Read until a line of `kind`, checking and adding up any batches before it.
char *test_drain_until(TestConn *c, TestStream *s, const char *kind);

void run_stream_lifecycle_tests(void);

#endif // OSPREY_PROFILER_STREAM_TEST_SUPPORT_H
