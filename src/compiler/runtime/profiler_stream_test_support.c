// The endpoint tests' client [PROF-ATTACH-PROTOCOL]; see the header.
#include "profiler_stream_test_support.h"

#include <dirent.h>
#include <errno.h>
#include <poll.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <time.h>

enum { TEST_TIMEOUT_MS = 10000, TEST_RETRIES = 500, TEST_MAX_TMPDIRS = 8 };

static char g_tmpdirs[TEST_MAX_TMPDIRS][32];
static int g_tmpdir_count = 0;

void test_use_private_tmpdir(void) {
  assert(g_tmpdir_count < TEST_MAX_TMPDIRS);
  char *dir = g_tmpdirs[g_tmpdir_count++];
  memcpy(dir, "/tmp/osprey-attach-XXXXXX", sizeof("/tmp/osprey-attach-XXXXXX"));
  assert(mkdtemp(dir) != NULL);
  setenv("TMPDIR", dir, 1); // setenv copies
}

// Empty `dir` of plain entries, then remove it (or the link standing in it).
static void remove_flat(const char *dir) {
  DIR *d = opendir(dir);
  struct dirent *e;
  while (d && (e = readdir(d)) != NULL) {
    char path[1024];
    TEST_FMT(path, "%s/%s", dir, e->d_name);
    if (strcmp(e->d_name, ".") != 0 && strcmp(e->d_name, "..") != 0) {
      (void)unlink(path);
    }
  }
  if (d) {
    (void)closedir(d);
  }
  if (rmdir(dir) != 0) {
    (void)unlink(dir);
  }
}

void test_remove_private_tmpdirs(void) {
  for (int i = 0; i < g_tmpdir_count; i++) {
    char diag[512];
    TEST_FMT(diag, "%s/osprey-diagnostics-%u", g_tmpdirs[i],
             (unsigned)getuid());
    remove_flat(diag);
    remove_flat(g_tmpdirs[i]);
  }
  g_tmpdir_count = 0;
}

void test_socket_path_of(long long pid, char *out, size_t cap) {
  int n = snprintf(out, cap, "%s/osprey-diagnostics-%u/%lld.sock",
                   getenv("TMPDIR"), (unsigned)getuid(), pid);
  assert(n > 0 && (size_t)n < cap);
}

void test_socket_path(char *out, size_t cap) {
  test_socket_path_of((long long)getpid(), out, cap);
}

TestConn test_connect_to(const char *path) {
  struct sockaddr_un addr = {.sun_family = AF_UNIX};
  assert(strlen(path) < sizeof(addr.sun_path));
  memcpy(addr.sun_path, path, strlen(path) + 1);
  int fd = socket(AF_UNIX, SOCK_STREAM, 0);
  assert(fd >= 0);
  assert(connect(fd, (const struct sockaddr *)&addr, sizeof(addr)) == 0);
  return (TestConn){.fd = fd, .buf = NULL, .len = 0, .cap = 0};
}

TestConn test_connect(void) {
  char path[512];
  test_socket_path(path, sizeof(path));
  return test_connect_to(path);
}

void test_close(TestConn *c) {
  (void)close(c->fd);
  free(c->buf);
  *c = (TestConn){.fd = -1, .buf = NULL, .len = 0, .cap = 0};
}

void test_send(TestConn *c, const char *line) {
  size_t n = strlen(line);
  assert(write(c->fd, line, n) == (ssize_t)n);
  assert(write(c->fd, "\n", 1) == 1);
}

// The line already buffered, detached from the buffer; NULL without one.
static char *take_buffered(TestConn *c) {
  char *nl = c->len ? memchr(c->buf, '\n', c->len) : NULL;
  if (nl == NULL) {
    return NULL;
  }
  size_t n = (size_t)(nl - c->buf);
  char *line = malloc(n + 1);
  assert(line != NULL);
  memcpy(line, c->buf, n);
  line[n] = '\0';
  memmove(c->buf, nl + 1, c->len - n - 1);
  c->len -= n + 1;
  return line;
}

// The test thread is registered, so on Linux the sampler's SIGPROF reaches it
// mid-poll; poll is never restarted, so EINTR means "ask again", not "timed out".
static bool read_more(TestConn *c) {
  struct pollfd p = {.fd = c->fd, .events = POLLIN};
  int ready;
  while ((ready = poll(&p, 1, TEST_TIMEOUT_MS)) < 0 && errno == EINTR) {
  }
  if (ready != 1) {
    return false;
  }
  if (c->cap - c->len < 65536) {
    c->cap = c->cap * 2 + 65536;
    c->buf = realloc(c->buf, c->cap);
    assert(c->buf != NULL);
  }
  ssize_t n = read(c->fd, c->buf + c->len, c->cap - c->len);
  c->len += n > 0 ? (size_t)n : 0;
  return n > 0;
}

char *test_next_line(TestConn *c) {
  char *line;
  while ((line = take_buffered(c)) == NULL) {
    if (!read_more(c)) {
      return NULL;
    }
  }
  return line;
}

char *test_request(TestConn *c, const char *line) {
  test_send(c, line);
  char *reply = test_next_line(c);
  assert(reply != NULL);
  return reply;
}

void test_expect_error(TestConn *c, const char *line, const char *needle) {
  char *reply = test_request(c, line);
  if (strstr(reply, needle) == NULL) {
    fprintf(stderr, "request `%s`: wanted `%s` in %s\n", line, needle, reply);
    assert(!"reply mismatch");
  }
  free(reply);
}

char *test_retry_start(TestConn *c, const char *line) {
  const struct timespec pause = {.tv_sec = 0, .tv_nsec = 10000000};
  for (int i = 0; i < TEST_RETRIES; i++) {
    char *reply = test_request(c, line);
    if (strstr(reply, "\"kind\":\"cpu_started\"")) {
      return reply;
    }
    free(reply);
    (void)nanosleep(&pause, NULL);
  }
  assert(!"the endpoint never released the session");
  return NULL;
}

// ---- cpu_batch checking -------------------------------------------------------

// The bytes between `"key":[` and its matching `]`, skipping over strings.
static bool array_span(const char *line, const char *key, const char **lo,
                       const char **hi) {
  char pattern[64];
  TEST_FMT(pattern, "\"%s\":[", key);
  const char *p = strstr(line, pattern);
  if (p == NULL) {
    return false;
  }
  *lo = p + strlen(pattern);
  int depth = 0;
  for (p = *lo; *p && !(depth == 0 && *p == ']'); p++) {
    if (*p == '"') {
      for (p++; *p && *p != '"'; p += (*p == '\\') ? 2 : 1) {
      }
    }
    depth += (*p == '[' || *p == '{') - (*p == ']' || *p == '}');
  }
  *hi = p;
  return *p == ']';
}

static size_t count_elements(const char *lo, const char *hi) {
  size_t n = lo < hi ? 1 : 0;
  int depth = 0;
  for (const char *p = lo; p < hi; p++) {
    depth += (*p == '[' || *p == '{') - (*p == ']' || *p == '}');
    n += depth == 0 && *p == ',';
  }
  return n;
}

// Every sample must name a row and a stack some batch has defined by now, and
// ticks never run backwards: the rows of one tick are one sampling instant.
static size_t check_samples(const char *lo, const char *hi, TestStream *s) {
  size_t n = 0;
  for (const char *p = lo; p < hi; p++) {
    unsigned long long t = 0;
    unsigned thread = 0, stack = 0, state = 0, tick = 0;
    if (*p == '[') {
      assert(sscanf(p, "[%llu,%u,%u,%u,%u]", &t, &thread, &stack, &state,
                    &tick) == 5);
      assert(thread < s->rows && stack < s->stacks && state <= 1);
      assert(tick >= s->last_tick);
      s->last_tick = tick;
      p = strchr(p, ']');
      n++;
    }
  }
  return n;
}

static void add_batch(const char *line, TestStream *s, const char *label) {
  const char *lo, *hi;
  assert(strstr(line, "{\"kind\":\"cpu_batch\",") == line);
  assert(array_span(line, "threads", &lo, &hi));
  s->rows += count_elements(lo, hi);
  s->saw_label = s->saw_label || (label && strstr(line, label) &&
                                  strstr(line, label) < hi);
  assert(array_span(line, "stacks", &lo, &hi));
  s->stacks += count_elements(lo, hi);
  assert(array_span(line, "samples", &lo, &hi));
  s->samples += check_samples(lo, hi, s);
}

TestStream test_stream_until(TestConn *c, size_t min_samples, const char *label) {
  TestStream s = {0};
  while (s.samples < min_samples || (label && !s.saw_label)) {
    char *line = test_next_line(c);
    assert(line != NULL);
    add_batch(line, &s, label);
    free(line);
  }
  return s;
}

char *test_drain_until(TestConn *c, TestStream *s, const char *kind) {
  char want[64];
  TEST_FMT(want, "{\"kind\":\"%s\"", kind);
  for (;;) {
    char *line = test_next_line(c);
    assert(line != NULL);
    if (strstr(line, want) == line) {
      return line;
    }
    add_batch(line, s, NULL);
    free(line);
  }
}
