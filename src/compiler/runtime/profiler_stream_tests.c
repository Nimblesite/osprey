// Live-attach endpoint tests [PROF-ATTACH-ENDPOINT] [PROF-ATTACH-PROTOCOL]
// (docs/specs/tooling/0028-Profiler.md). Every test drives the real socket the
// way a language server does: connect, write request lines, read NDJSON.
// TMPDIR points at a private scratch directory, so nothing here touches the
// user's real diagnostics directory.
#include "profiler_stream_test_support.h"

#include <fcntl.h>
#include <signal.h>
#include <stdatomic.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/wait.h>

static atomic_bool g_spin = true;
static volatile double g_spin_sink = 0;

static void *spinner(void *label) {
  osp_prof_thread_register(42, label);
  double acc = 0;
  while (atomic_load(&g_spin)) {
    for (int i = 1; i < 100000; i++) {
      acc += (double)i / (double)(i + 1);
    }
  }
  g_spin_sink += acc;
  osp_prof_thread_unregister();
  return NULL;
}

// Run `body` with a pipe holding one line as the program's stdin, then assert
// the line is still there to read: the endpoint must close nothing it did not
// open. It once closed fd 0, and every program's `input()` read nothing.
static void with_stdin_intact(void (*body)(void)) {
  int p[2];
  assert(pipe(p) == 0);
  int saved = dup(STDIN_FILENO);
  assert(saved >= 0 && dup2(p[0], STDIN_FILENO) == STDIN_FILENO);
  assert(write(p[1], "hello\n", 6) == 6);
  body();
  char line[8] = {0};
  assert(read(STDIN_FILENO, line, 6) == 6 && memcmp(line, "hello\n", 6) == 0);
  assert(dup2(saved, STDIN_FILENO) == STDIN_FILENO);
  (void)close(saved), (void)close(p[0]), (void)close(p[1]);
}

static void start_disabled(void) {
  setenv("OSPREY_DIAGNOSTICS", "0", 1);
  assert(!osp_diag_start());
  unsetenv("OSPREY_DIAGNOSTICS");
}

static void start_and_stop(void) {
  assert(osp_diag_start());
  osp_diag_stop();
}

// [PROF-ATTACH-ENDPOINT] the endpoint leaves the program's own files alone,
// disabled or not.
static void test_stdin_survives_the_endpoint(void) {
  with_stdin_intact(start_disabled);
  with_stdin_intact(start_and_stop);
}

// [PROF-ATTACH-ENDPOINT] "unless `OSPREY_DIAGNOSTICS=0`".
static void test_opt_out_opens_nothing(void) {
  setenv("OSPREY_DIAGNOSTICS", "0", 1);
  assert(!osp_diag_start());
  unsetenv("OSPREY_DIAGNOSTICS");
  char path[512];
  test_socket_path(path, sizeof(path));
  assert(access(path, F_OK) != 0);
}

// [PROF-ATTACH-ENDPOINT] the socket lives at
// `$TMPDIR/osprey-diagnostics-<uid>/<pid>.sock` in a `0700` directory.
static void test_socket_in_private_directory(void) {
  osp_prof_boot();
  assert(osp_diag_start()); // idempotent
  char path[512];
  test_socket_path(path, sizeof(path));
  struct stat st;
  assert(stat(path, &st) == 0 && S_ISSOCK(st.st_mode));
  *strrchr(path, '/') = '\0';
  assert(stat(path, &st) == 0 && S_ISDIR(st.st_mode));
  assert((st.st_mode & 0777) == 0700 && st.st_uid == getuid());
  assert(osp_prof_self_slot() != NULL); // the endpoint registers main
}

// [PROF-ATTACH-PROTOCOL] `hello` names the process and the protocol.
static void test_hello(void) {
  TestConn c = test_connect();
  char *hello = test_request(&c, "hello");
  char pid[64];
  TEST_FMT(pid, "\"pid\":%lld,", (long long)getpid());
  assert(strstr(hello, "{\"kind\":\"hello\",\"protocol\":1,") == hello);
  assert(strstr(hello, pid) && strstr(hello, "\"platform\":\""));
  assert(strstr(hello, "\"cpu_session\":false"));
  assert(strstr(hello, "\"runtime_version\":\"") && strstr(hello, "\"exe\":\"/"));
  free(hello);
  test_close(&c);
}

// [PROF-ATTACH-PROTOCOL] every malformed request gets a classified error and
// the connection keeps serving.
static void test_errors_are_classified(void) {
  TestConn c = test_connect();
  test_expect_error(&c, "flamegraph please", "unknown_command");
  test_expect_error(&c, "cpu_start", "bad_argument");
  test_expect_error(&c, "cpu_start 99hz", "bad_argument");
  test_expect_error(&c, "cpu_stop", "not_profiling");
  test_expect_error(&c, "hello\r", "kind\":\"hello"); // CRLF is one line ending
  test_close(&c);
}

// [PROF-ATTACH-PROTOCOL] a session streams batches whose rows and stacks index
// the tables the batches built, then stops with `requested`.
static void test_session_streams_a_busy_thread(void) {
  pthread_t t;
  atomic_store(&g_spin, true);
  assert(pthread_create(&t, NULL, spinner, "spinner") == 0);
  TestConn c = test_connect();
  char *started = test_request(&c, "cpu_start 997");
  assert(strstr(started, "{\"kind\":\"cpu_started\",\"rate_hz\":997,") == started);
  assert(strstr(started, "\"images\":[{\"path\":\""));
  free(started);
  TestStream s = test_stream_until(&c, 50, "spinner");
  assert(s.samples >= 50 && s.saw_label);
  test_send(&c, "cpu_stop");
  char *stopped = test_drain_until(&c, &s, "cpu_stopped");
  assert(strstr(stopped, "\"reason\":\"requested\",\"dropped\":"));
  free(stopped);
  atomic_store(&g_spin, false);
  assert(pthread_join(t, NULL) == 0);
  test_close(&c);
}

// [PROF-ATTACH-PROTOCOL] one CPU session per process; the owner's disconnect
// ends it, after which another connection can start one.
static void test_one_session_per_process(void) {
  TestConn a = test_connect(), b = test_connect();
  free(test_request(&a, "cpu_start 500"));
  test_expect_error(&b, "cpu_start 500", "already_profiling");
  test_expect_error(&b, "cpu_stop", "not_profiling");
  test_expect_error(&b, "hello", "\"cpu_session\":true");
  test_close(&a);
  char *started = test_retry_start(&b, "cpu_start 500");
  assert(strstr(started, "cpu_started"));
  free(started);
  test_send(&b, "cpu_stop");
  TestStream s = {0};
  free(test_drain_until(&b, &s, "cpu_stopped"));
  test_close(&b);
}

// [PROF-ATTACH-PROTOCOL] an OSPREY_PROFILE session is a session too.
static void test_env_session_blocks_attach(void) {
  char out[] = "/tmp/osprey-attach-env-XXXXXX";
  int fd = mkstemp(out);
  assert(fd >= 0);
  (void)close(fd);
  assert(osp_prof_start(out, 500));
  TestConn c = test_connect();
  test_expect_error(&c, "cpu_start 500", "already_profiling");
  test_close(&c);
  osp_prof_stop_and_dump();
  (void)unlink(out);
}

// [PROF-ATTACH-PROTOCOL] a label is escaped, so a batch is always one line.
static void test_labels_never_break_framing(void) {
  pthread_t t;
  atomic_store(&g_spin, true);
  assert(pthread_create(&t, NULL, spinner, "two\nlines") == 0);
  TestConn c = test_connect();
  free(test_request(&c, "cpu_start 997"));
  TestStream s = test_stream_until(&c, 1, "two\\u000alines");
  assert(s.saw_label);
  test_send(&c, "cpu_stop");
  free(test_drain_until(&c, &s, "cpu_stopped"));
  atomic_store(&g_spin, false);
  assert(pthread_join(t, NULL) == 0);
  test_close(&c);
}

// [PROF-ATTACH-PROTOCOL] request lines are bounded.
static void test_long_line_closes_connection(void) {
  TestConn c = test_connect();
  char line[400];
  memset(line, 'x', sizeof(line) - 1);
  line[sizeof(line) - 1] = '\0';
  test_expect_error(&c, line, "line_too_long");
  assert(test_next_line(&c) == NULL); // then EOF
  test_close(&c);
}

// [PROF-ATTACH-ENDPOINT] the endpoint serves at most eight connections.
static void test_ninth_connection_is_busy(void) {
  TestConn conns[8];
  for (int i = 0; i < 8; i++) {
    conns[i] = test_connect();
    free(test_request(&conns[i], "hello"));
  }
  TestConn extra = test_connect();
  char *busy = test_next_line(&extra);
  assert(busy && strstr(busy, "\"code\":\"busy\""));
  free(busy);
  test_close(&extra);
  for (int i = 0; i < 8; i++) {
    test_close(&conns[i]);
  }
}

// [PROF-ATTACH-ENDPOINT] a stopped endpoint removes its socket; the last
// process out removes the directory too, and a neighbour's socket keeps it.
static void test_stop_removes_socket_and_empty_directory(void) {
  char path[512];
  test_socket_path(path, sizeof(path));
  char dir[512];
  memcpy(dir, path, sizeof(dir));
  *strrchr(dir, '/') = '\0';
  char neighbour[600];
  assert(snprintf(neighbour, sizeof(neighbour), "%s/1.sock", dir) > 0);
  int fd = open(neighbour, O_WRONLY | O_CREAT | O_EXCL, 0600);
  assert(fd >= 0 && close(fd) == 0);
  osp_diag_stop();
  assert(access(path, F_OK) != 0 && access(dir, F_OK) == 0); // kept for 1.sock
  assert(unlink(neighbour) == 0);
  assert(osp_diag_start()); // reuses the directory it was handed
  osp_diag_stop();
  osp_diag_stop(); // idempotent
  assert(access(path, F_OK) != 0 && access(dir, F_OK) != 0);
}

int main(void) {
  signal(SIGPIPE, SIG_IGN); // the TEST's writes to a closed peer
  test_use_private_tmpdir();
  TEST_RUN(test_stdin_survives_the_endpoint);
  TEST_RUN(test_opt_out_opens_nothing);
  TEST_RUN(test_socket_in_private_directory);
  TEST_RUN(test_hello);
  TEST_RUN(test_errors_are_classified);
  TEST_RUN(test_session_streams_a_busy_thread);
  TEST_RUN(test_one_session_per_process);
  TEST_RUN(test_env_session_blocks_attach);
  TEST_RUN(test_labels_never_break_framing);
  TEST_RUN(test_long_line_closes_connection);
  TEST_RUN(test_ninth_connection_is_busy);
  TEST_RUN(test_stop_removes_socket_and_empty_directory);
  run_stream_lifecycle_tests();
  test_remove_private_tmpdirs();
  printf("profiler_stream_tests: all tests passed (sink=%f)\n", g_spin_sink);
  return 0;
}
