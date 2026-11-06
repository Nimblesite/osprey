// Osprey live-attach wire protocol and CPU sessions. Implements
// [PROF-ATTACH-PROTOCOL] (docs/specs/tooling/0028-Profiler.md).
//
// Session data is streamed by the SAMPLER thread (osp_diag_pump), because that
// thread is the sample store's single writer [PROF-COLLECT-SAMPLER]; once a
// session stops, the thread that joined the sampler sends the rest. Replies
// come from the listener thread in profiler_endpoint.c. Every write goes
// through msg_send, so a batch and a reply never interleave on the wire.
#include "profiler_stream.h"

#if defined(OSP_DIAG_ENDPOINT)

#include <errno.h>
#include <inttypes.h>
#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

#ifndef OSP_RUNTIME_VERSION
#define OSP_RUNTIME_VERSION "0.0.0-dev"
#endif

// A peer that vanished must never SIGPIPE the program: Linux opts out per
// send, macOS per socket (SO_NOSIGPIPE, set when the endpoint accepts).
#if defined(MSG_NOSIGNAL)
#define OSP_DIAG_SEND_FLAGS MSG_NOSIGNAL
#else
#define OSP_DIAG_SEND_FLAGS 0
#endif

enum { OSP_DIAG_PROTOCOL = 1, OSP_DIAG_MIN_HZ = 10, OSP_DIAG_MAX_HZ = 10000 };
static const uint64_t OSP_DIAG_PUMP_NS = 100000000ULL; // a batch per 100 ms

// Stream cursors: the sampler thread's while a session runs, the stopping
// thread's once it has joined the sampler.
typedef struct DiagStream {
  uint32_t sent_rows;
  size_t sent_stacks;
  uint32_t sent_images;
  uint64_t last_pump_ns;
} DiagStream;

static DiagStream g_stream;
// The connection that owns the CPU session, -1 without one. Written under
// g_session_mutex; the sampler reads it to know where to pump.
static _Atomic int g_session_fd = -1;
static _Atomic bool g_session_broken = false;
static pthread_mutex_t g_session_mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_mutex_t g_write_mutex = PTHREAD_MUTEX_INITIALIZER;

// ---- messages: one JSON object per line ---------------------------------------

static bool send_all(int fd, const char *buf, size_t len) {
  while (len > 0) {
    ssize_t n = send(fd, buf, len, OSP_DIAG_SEND_FLAGS);
    if (n < 0 && errno == EINTR) {
      continue;
    }
    if (n <= 0) {
      return false;
    }
    buf += n;
    len -= (size_t)n;
  }
  return true;
}

typedef struct DiagMsg {
  FILE *f;
  char *buf;
  size_t len;
} DiagMsg;

static bool msg_open(DiagMsg *m) {
  *m = (DiagMsg){.f = NULL, .buf = NULL, .len = 0};
  m->f = open_memstream(&m->buf, &m->len);
  return m->f != NULL;
}

// Finish `m` as one line and send it whole; the buffer is freed either way.
// Only the session's connection has two writers (the sampler's batches, the
// listener's replies), so only it takes the write lock. A peer that cannot take
// a line within the send timeout is shut down: the listener then reads EOF and
// closes it, so a client that stopped reading never holds anything up twice.
static bool msg_send(DiagMsg *m, int fd) {
  fputc('\n', m->f);
  bool ok = fclose(m->f) == 0;
  bool shared = fd == g_session_fd;
  if (shared) {
    pthread_mutex_lock(&g_write_mutex);
  }
  ok = ok && send_all(fd, m->buf, m->len);
  if (shared) {
    pthread_mutex_unlock(&g_write_mutex);
  }
  free(m->buf);
  if (!ok) {
    (void)shutdown(fd, SHUT_RDWR);
  }
  return ok;
}

void osp_diag_reply_error(int fd, const char *code, const char *message) {
  DiagMsg m;
  if (!msg_open(&m)) {
    return;
  }
  fprintf(m.f, "{\"kind\":\"error\",\"code\":\"%s\",\"message\":", code);
  osp_prof_json_string(m.f, message);
  fputc('}', m.f);
  (void)msg_send(&m, fd);
}

static bool send_started(int fd) {
  DiagMsg m;
  if (!msg_open(&m)) {
    return false;
  }
  g_stream.sent_images = osp_prof_image_count();
  fprintf(m.f,
          "{\"kind\":\"cpu_started\",\"rate_hz\":%u,\"start_unix_ns\":%" PRIu64
          ",\"images\":[",
          osp_prof_rate_hz(), osp_prof_session_start_unix_ns());
  osp_prof_write_images(m.f);
  fputs("]}", m.f);
  return msg_send(&m, fd);
}

static void write_images_if_changed(FILE *f) {
  uint32_t count = osp_prof_image_count();
  if (count != g_stream.sent_images) {
    fputs(",\"images\":[", f);
    osp_prof_write_images(f);
    fputc(']', f);
    g_stream.sent_images = count;
  }
}

// The rows and stacks new since the last batch and every retained sample. The
// samples then leave the store: the client keeps them, and a long session must
// not grow without bound.
static bool send_batch(int fd) {
  DiagMsg m;
  if (!msg_open(&m)) {
    return false;
  }
  fputs("{\"kind\":\"cpu_batch\",\"threads\":[", m.f);
  uint32_t rows = osp_prof_write_threads(m.f, g_stream.sent_rows);
  fputs("],\"stacks\":[", m.f);
  size_t stacks = osp_prof_write_stacks(m.f, g_stream.sent_stacks);
  fputs("],\"samples\":[", m.f);
  osp_prof_write_samples(m.f, true);
  fputc(']', m.f);
  write_images_if_changed(m.f);
  fputc('}', m.f);
  if (!msg_send(&m, fd)) {
    return false;
  }
  g_stream.sent_rows = rows;
  g_stream.sent_stacks = stacks;
  osp_prof_samples_clear();
  return true;
}

static void send_stopped(int fd, const char *reason) {
  DiagMsg m;
  if (!msg_open(&m)) {
    return;
  }
  fprintf(m.f,
          "{\"kind\":\"cpu_stopped\",\"reason\":\"%s\",\"dropped\":%" PRIu64
          ",\"end_unix_ns\":%" PRIu64 "}",
          reason, osp_prof_dropped(), osp_prof_unix_ns());
  (void)msg_send(&m, fd);
}

// ---- CPU sessions -------------------------------------------------------------

void osp_diag_pump(void) {
  int fd = g_session_fd;
  uint64_t now = osp_prof_mono_ns();
  if (fd < 0 || g_session_broken ||
      now - g_stream.last_pump_ns < OSP_DIAG_PUMP_NS) {
    return;
  }
  g_stream.last_pump_ns = now;
  g_session_broken = !send_batch(fd); // msg_send shut the socket: no retries
}

// Hand the running session to `fd`. The sampler pumps only once g_session_fd
// is set, so `cpu_started` always precedes the first batch.
static void begin_stream(int fd) {
  g_stream = (DiagStream){.sent_rows = 0, .sent_stacks = 0};
  g_session_broken = false;
  if (send_started(fd)) {
    g_session_fd = fd;
  } else {
    osp_prof_session_stop();
  }
}

// Stop the session. The sampler is joined first, so this thread is then the
// store's only user and sends what was never pumped — unless `reason` is NULL
// (the owner disconnected) or the stream already broke. Under g_session_mutex.
static void end_session(const char *reason) {
  int fd = g_session_fd;
  osp_prof_session_stop();
  g_session_fd = -1;
  if (reason != NULL && !g_session_broken && send_batch(fd)) {
    send_stopped(fd, reason);
  }
  g_session_broken = false;
}

void osp_diag_forget(int fd) {
  pthread_mutex_lock(&g_session_mutex);
  if (g_session_fd == fd) {
    end_session(NULL);
  }
  pthread_mutex_unlock(&g_session_mutex);
}

void osp_diag_finish(void) {
  pthread_mutex_lock(&g_session_mutex);
  if (g_session_fd >= 0) {
    end_session("exit");
  }
  pthread_mutex_unlock(&g_session_mutex);
}

// ---- requests -----------------------------------------------------------------

static void cmd_hello(DiagClient *c, const char *arg) {
  (void)arg;
  DiagMsg m;
  if (!msg_open(&m)) {
    return;
  }
  fprintf(m.f,
          "{\"kind\":\"hello\",\"protocol\":%d,\"pid\":%lld,\"platform\":\"%s\","
          "\"cpu_session\":%s,\"runtime_version\":",
          OSP_DIAG_PROTOCOL, (long long)getpid(), osp_prof_platform(),
          osp_prof_is_active() ? "true" : "false");
  osp_prof_json_string(m.f, OSP_RUNTIME_VERSION);
  fputs(",\"exe\":", m.f);
  osp_prof_write_exe(m.f);
  fputc('}', m.f);
  (void)msg_send(&m, c->fd);
}

// `cpu_start <hz>`: a decimal rate, clamped like OSPREY_PROFILE_HZ.
static bool parse_rate(const char *arg, uint32_t *hz) {
  char *end = NULL;
  long v = strtol(arg, &end, 10);
  if (end == arg || *end != '\0') {
    return false;
  }
  v = v < OSP_DIAG_MIN_HZ ? OSP_DIAG_MIN_HZ : v;
  *hz = (uint32_t)(v > OSP_DIAG_MAX_HZ ? OSP_DIAG_MAX_HZ : v);
  return true;
}

static void cmd_cpu_start(DiagClient *c, const char *arg) {
  uint32_t hz = 0;
  if (!parse_rate(arg, &hz)) {
    osp_diag_reply_error(c->fd, "bad_argument", "cpu_start takes a rate in Hz");
    return;
  }
  pthread_mutex_lock(&g_session_mutex);
  if (osp_prof_is_active()) {
    osp_diag_reply_error(c->fd, "already_profiling",
                         "a CPU session is already running in this process");
  } else if (!osp_prof_session_start(hz)) {
    osp_diag_reply_error(c->fd, "attach_failed",
                         "the sampler thread did not start");
  } else {
    begin_stream(c->fd);
  }
  pthread_mutex_unlock(&g_session_mutex);
}

static void cmd_cpu_stop(DiagClient *c, const char *arg) {
  (void)arg;
  pthread_mutex_lock(&g_session_mutex);
  if (g_session_fd != c->fd) {
    osp_diag_reply_error(c->fd, "not_profiling",
                         "this connection has no CPU session");
  } else {
    end_session("requested");
  }
  pthread_mutex_unlock(&g_session_mutex);
}

typedef void (*DiagCommand)(DiagClient *c, const char *arg);

static const struct {
  const char *name;
  DiagCommand run;
} k_commands[] = {
    {"hello", cmd_hello},
    {"cpu_start", cmd_cpu_start},
    {"cpu_stop", cmd_cpu_stop},
};

static void handle_line(DiagClient *c, char *line) {
  line[strcspn(line, "\r")] = '\0';
  char *arg = strchr(line, ' ');
  if (arg) {
    *arg++ = '\0';
  } else {
    arg = line + strlen(line);
  }
  for (size_t i = 0; i < sizeof(k_commands) / sizeof(k_commands[0]); i++) {
    if (strcmp(line, k_commands[i].name) == 0) {
      k_commands[i].run(c, arg);
      return;
    }
  }
  osp_diag_reply_error(c->fd, "unknown_command", line);
}

bool osp_diag_take_lines(DiagClient *c) {
  char *nl;
  while ((nl = memchr(c->line, '\n', c->used)) != NULL) {
    *nl = '\0';
    size_t taken = (size_t)(nl - c->line) + 1;
    handle_line(c, c->line);
    memmove(c->line, nl + 1, c->used - taken);
    c->used -= taken;
  }
  if (c->used == sizeof(c->line) - 1) {
    osp_diag_reply_error(c->fd, "line_too_long",
                         "a request line is at most 255 bytes");
    return false;
  }
  return true;
}

#elif !defined(_WIN32) && !defined(__wasm__)

void osp_diag_pump(void) {} // no endpoint on this platform to stream to

#else

typedef int osp_diag_stream_unused; // profiling is POSIX-only

#endif
