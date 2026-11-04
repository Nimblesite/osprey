// Endpoint lifecycle tests [PROF-ATTACH-ENDPOINT] (docs/specs/tooling/0028-Profiler.md):
// the directory it refuses, the sockets it sweeps, a fork, a slow reader, and
// a program that exits mid-session.
#include "profiler_stream_test_support.h"

#include <fcntl.h>
#include <stdatomic.h>
#include <sys/stat.h>
#include <sys/wait.h>

static atomic_bool g_busy = true;
static volatile double g_busy_sink = 0;

static void *busy(void *arg) {
  (void)arg;
  osp_prof_thread_register(7, "busy");
  double acc = 0;
  while (atomic_load(&g_busy)) {
    for (int i = 1; i < 100000; i++) {
      acc += (double)i / (double)(i + 1);
    }
  }
  g_busy_sink += acc;
  osp_prof_thread_unregister();
  return NULL;
}

static void diag_dir_of(char *out, size_t cap) {
  int n = snprintf(out, cap, "%s/osprey-diagnostics-%u", getenv("TMPDIR"),
                   (unsigned)getuid());
  assert(n > 0 && (size_t)n < cap);
}

static void touch(const char *dir, const char *name) {
  char path[1024];
  TEST_FMT(path, "%s/%s", dir, name);
  int fd = open(path, O_CREAT | O_WRONLY, 0600);
  assert(fd >= 0);
  (void)close(fd);
}

static bool exists(const char *dir, const char *name) {
  char path[1024];
  TEST_FMT(path, "%s/%s", dir, name);
  return access(path, F_OK) == 0;
}

// [PROF-ATTACH-ENDPOINT] "an existing directory that is not a real directory
// owned by the user with no group or other permission bits disables the
// endpoint rather than being repaired".
static void test_unsafe_directory_disables_endpoint(void) {
  test_use_private_tmpdir();
  char dir[512];
  diag_dir_of(dir, sizeof(dir));
  assert(mkdir(dir, 0700) == 0 && chmod(dir, 0777) == 0);
  assert(!osp_diag_start());
  assert(chmod(dir, 0700) == 0);
  assert(osp_diag_start());
  osp_diag_stop();
  test_use_private_tmpdir();
  diag_dir_of(dir, sizeof(dir));
  assert(symlink("/tmp", dir) == 0); // a link is never the directory itself
  assert(!osp_diag_start());
  test_use_private_tmpdir();
}

// [PROF-ATTACH-ENDPOINT] "each start sweeps sockets in the directory whose
// process no longer exists".
static void test_start_sweeps_dead_sockets(void) {
  pid_t dead = fork();
  assert(dead >= 0);
  if (dead == 0) {
    _exit(0);
  }
  assert(waitpid(dead, NULL, 0) == dead);
  char dir[512], name[64], alive[64];
  diag_dir_of(dir, sizeof(dir));
  assert(mkdir(dir, 0700) == 0);
  TEST_FMT(name, "%lld.sock", (long long)dead);
  TEST_FMT(alive, "%lld.sock", (long long)getppid());
  touch(dir, name);
  touch(dir, alive);
  touch(dir, "notes.sock");
  assert(osp_diag_start());
  assert(!exists(dir, name));
  assert(exists(dir, alive) && exists(dir, "notes.sock"));
}

// [PROF-ATTACH-ENDPOINT] a forked child inherits no endpoint.
static void test_forked_child_gets_no_endpoint(void) {
  pid_t child = fork();
  assert(child >= 0);
  if (child == 0) {
    _exit(osp_diag_start() ? 1 : 0);
  }
  int status = 0;
  assert(waitpid(child, &status, 0) == child);
  assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
}

// [PROF-ATTACH-PROTOCOL] "A connection that cannot take a line within one
// second is closed": a session whose owner stopped reading ends instead of
// stalling the sampler, and the endpoint serves the next client.
static void test_slow_reader_loses_its_session(void) {
  pthread_t t;
  atomic_store(&g_busy, true);
  assert(pthread_create(&t, NULL, busy, NULL) == 0);
  TestConn stuck = test_connect(), probe = test_connect();
  free(test_request(&stuck, "cpu_start 10000")); // and never read again
  char *started = test_retry_start(&probe, "cpu_start 10000");
  free(started);
  test_send(&probe, "cpu_stop");
  TestStream s = {0};
  free(test_drain_until(&probe, &s, "cpu_stopped"));
  char *line;
  while ((line = test_next_line(&stuck)) != NULL) { // ends: the endpoint closed it
    free(line);
  }
  atomic_store(&g_busy, false);
  assert(pthread_join(t, NULL) == 0);
  test_close(&stuck);
  test_close(&probe);
}

static void exiting_child(int ready_fd, int go_fd) {
  pthread_t t;
  char byte = 0;
  assert(osp_diag_start());
  osp_prof_registry_enable();
  atomic_store(&g_busy, true);
  assert(pthread_create(&t, NULL, busy, NULL) == 0);
  assert(write(ready_fd, "r", 1) == 1);
  assert(read(go_fd, &byte, 1) == 1);
  for (volatile long i = 0; i < 50000000; i++) {
  }
  exit(0); // the busy thread is still running: exit ends the session
}

// A connection to a child that serves its own endpoint and exits once `*go`
// is written to; the parent's endpoint is stopped so the child can open one.
static TestConn attach_to_exiting_child(pid_t *child, int *go) {
  int ready[2], go_pipe[2];
  assert(pipe(ready) == 0 && pipe(go_pipe) == 0);
  osp_diag_stop();
  *child = fork();
  assert(*child >= 0);
  if (*child == 0) {
    exiting_child(ready[1], go_pipe[0]);
  }
  char byte = 0, path[512];
  assert(read(ready[0], &byte, 1) == 1);
  (void)close(ready[0]), (void)close(ready[1]), (void)close(go_pipe[0]);
  *go = go_pipe[1];
  test_socket_path_of((long long)*child, path, sizeof(path));
  return test_connect_to(path);
}

// [PROF-ATTACH-PROTOCOL] "Program exit during a session sends the final batch
// and `cpu_stopped` with reason `exit`."
static void test_exit_ends_the_session_with_exit(void) {
  pid_t child = 0;
  int go = -1;
  TestConn c = attach_to_exiting_child(&child, &go);
  free(test_request(&c, "cpu_start 997"));
  assert(write(go, "g", 1) == 1);
  (void)close(go);
  TestStream s = {0};
  char *stopped = test_drain_until(&c, &s, "cpu_stopped");
  if (!strstr(stopped, "\"reason\":\"exit\"") || s.samples == 0) {
    fprintf(stderr, "%zu samples, then %s\n", s.samples, stopped);
    assert(!"an exit mid-session must deliver its samples and say `exit`");
  }
  free(stopped);
  int status = 0;
  char path[512];
  assert(waitpid(child, &status, 0) == child && WIFEXITED(status));
  test_socket_path_of((long long)child, path, sizeof(path));
  assert(access(path, F_OK) != 0); // and it took its socket with it
  test_close(&c);
}

void run_stream_lifecycle_tests(void) {
  TEST_RUN(test_unsafe_directory_disables_endpoint);
  TEST_RUN(test_start_sweeps_dead_sockets);
  TEST_RUN(test_forked_child_gets_no_endpoint);
  TEST_RUN(test_slow_reader_loses_its_session);
  TEST_RUN(test_exit_ends_the_session_with_exit);
  osp_diag_stop();
  printf("profiler_stream_lifecycle_tests: passed (sink=%f)\n", g_busy_sink);
}
