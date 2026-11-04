// Osprey live-attach endpoint: the per-process Unix socket, its connections,
// the listener thread and the lifecycle. Implements [PROF-ATTACH-ENDPOINT]
// (docs/specs/tooling/0028-Profiler.md); the protocol is profiler_stream.c.
#include "profiler_stream.h"

#if defined(OSP_DIAG_ENDPOINT)

#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/un.h>
#include <unistd.h>

enum {
  OSP_DIAG_MAX_CLIENTS = 8,
  OSP_DIAG_SEND_TIMEOUT_S = 1,
  OSP_DIAG_SWEEP_MAX = 64,
  OSP_DIAG_FIXED_FDS = 2, // the wake pipe and the listening socket
};

typedef struct DiagEndpoint {
  pid_t owner; // a forked child must not remove its parent's socket
  int listen_fd;
  int wake[2]; // self-pipe: wakes poll() so osp_diag_stop is prompt
  pthread_t thread;
  char path[sizeof(((struct sockaddr_un *)0)->sun_path)];
  DiagClient clients[OSP_DIAG_MAX_CLIENTS];
} DiagEndpoint;

static DiagEndpoint g_diag = {.listen_fd = -1, .wake = {-1, -1}};
static _Atomic bool g_running = false;

// ---- connections ----------------------------------------------------------------

static void set_cloexec(int fd) { (void)fcntl(fd, F_SETFD, FD_CLOEXEC); }

// A send blocked longer than the timeout closes the connection instead of
// stalling the sampler that is waiting on it.
static void configure_client(int fd) {
  set_cloexec(fd);
  struct timeval tv = {.tv_sec = OSP_DIAG_SEND_TIMEOUT_S, .tv_usec = 0};
  (void)setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &tv, sizeof(tv));
#if defined(SO_NOSIGPIPE)
  int one = 1;
  (void)setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &one, sizeof(one));
#endif
}

static void wake_listener(void) {
  const char byte = 1;
  if (g_diag.wake[1] >= 0) {
    (void)!write(g_diag.wake[1], &byte, 1);
  }
}

static bool serve_client(DiagClient *c) {
  ssize_t n = read(c->fd, c->line + c->used, sizeof(c->line) - 1 - c->used);
  if (n < 0 && errno == EINTR) {
    return true;
  }
  if (n <= 0) {
    return false;
  }
  c->used += (size_t)n;
  return osp_diag_take_lines(c);
}

static DiagClient *client_for(int fd) {
  for (int i = 0; i < OSP_DIAG_MAX_CLIENTS; i++) {
    if (g_diag.clients[i].fd == fd) {
      return &g_diag.clients[i];
    }
  }
  return NULL;
}

// A disconnect ends the session it owned; nobody is left to read the rest.
static void close_client(DiagClient *c) {
  osp_diag_forget(c->fd);
  (void)close(c->fd);
  *c = (DiagClient){.fd = -1, .used = 0};
}

static void accept_client(void) {
  int fd = accept(g_diag.listen_fd, NULL, NULL);
  if (fd < 0) {
    return;
  }
  configure_client(fd);
  DiagClient *c = client_for(-1);
  if (c == NULL) {
    osp_diag_reply_error(fd, "busy", "the endpoint serves 8 connections at a time");
    (void)close(fd);
    return;
  }
  *c = (DiagClient){.fd = fd, .used = 0};
}

// ---- the listener thread ------------------------------------------------------

static nfds_t build_pollset(struct pollfd *fds) {
  nfds_t n = 0;
  fds[n++] = (struct pollfd){.fd = g_diag.wake[0], .events = POLLIN};
  fds[n++] = (struct pollfd){.fd = g_diag.listen_fd, .events = POLLIN};
  for (int i = 0; i < OSP_DIAG_MAX_CLIENTS; i++) {
    if (g_diag.clients[i].fd >= 0) {
      fds[n++] = (struct pollfd){.fd = g_diag.clients[i].fd, .events = POLLIN};
    }
  }
  return n;
}

static void serve_ready(const struct pollfd *fds, nfds_t n) {
  char drain[64];
  while ((fds[0].revents & POLLIN) && read(g_diag.wake[0], drain, sizeof(drain)) > 0) {
  }
  for (nfds_t k = OSP_DIAG_FIXED_FDS; k < n; k++) {
    DiagClient *c = client_for(fds[k].fd);
    if (c && (fds[k].revents & (POLLIN | POLLHUP | POLLERR)) && !serve_client(c)) {
      close_client(c);
    }
  }
  if (fds[1].revents & POLLIN) {
    accept_client();
  }
}

static void *listener_main(void *arg) {
  (void)arg;
  while (g_running) {
    struct pollfd fds[OSP_DIAG_FIXED_FDS + OSP_DIAG_MAX_CLIENTS];
    nfds_t n = build_pollset(fds);
    if (poll(fds, n, -1) > 0) {
      serve_ready(fds, n);
    }
  }
  return NULL;
}

// ---- lifecycle --------------------------------------------------------------------

static bool diagnostics_disabled(void) {
  const char *env = getenv("OSPREY_DIAGNOSTICS");
  return env != NULL && strcmp(env, "0") == 0;
}

static bool diag_dir(char *out, size_t cap) {
  const char *tmp = getenv("TMPDIR");
  tmp = (tmp && *tmp) ? tmp : "/tmp";
  const char *sep = tmp[strlen(tmp) - 1] == '/' ? "" : "/";
  int n = snprintf(out, cap, "%s%sosprey-diagnostics-%u", tmp, sep,
                   (unsigned)getuid());
  return n > 0 && (size_t)n < cap;
}

// A real directory the user owns that nobody else can enter. Anything else
// disables the endpoint rather than being repaired: it may be someone's trap.
static bool private_dir(const char *dir) {
  struct stat st;
  return lstat(dir, &st) == 0 && S_ISDIR(st.st_mode) &&
         st.st_uid == getuid() && (st.st_mode & 077) == 0;
}

static bool ensure_private_dir(const char *dir) {
  if (mkdir(dir, 0700) != 0 && errno != EEXIST) {
    return false;
  }
  return private_dir(dir);
}

// The directory of this process's socket: `g_diag.path` up to its last slash.
static bool socket_dir(char *dir, size_t cap) {
  memcpy(dir, g_diag.path, cap);
  char *slash = strrchr(dir, '/');
  if (slash == NULL) {
    return false;
  }
  *slash = '\0';
  return true;
}

// A socket whose process is gone was left by a crash or a SIGKILL.
static void remove_if_stale(const char *dir, const char *name) {
  char *end = NULL;
  long pid = strtol(name, &end, 10);
  if (end == name || strcmp(end, ".sock") != 0 || pid <= 0) {
    return;
  }
  char path[sizeof(g_diag.path)];
  int n = snprintf(path, sizeof(path), "%s/%s", dir, name);
  if (kill((pid_t)pid, 0) != 0 && errno == ESRCH && n > 0 &&
      (size_t)n < sizeof(path)) {
    (void)unlink(path);
  }
}

static void sweep_stale(const char *dir) {
  DIR *d = opendir(dir);
  if (d == NULL) {
    return;
  }
  struct dirent *e;
  for (int seen = 0; seen < OSP_DIAG_SWEEP_MAX && (e = readdir(d)) != NULL;
       seen++) {
    remove_if_stale(dir, e->d_name);
  }
  (void)closedir(d);
}

// An existing directory must be the user's private one; a missing one is
// created by the bind that first needs it (bind_listener).
static bool prepare_path(void) {
  char dir[sizeof(g_diag.path)];
  if (!diag_dir(dir, sizeof(dir)) ||
      (access(dir, F_OK) == 0 && !private_dir(dir))) {
    return false;
  }
  sweep_stale(dir);
  int n = snprintf(g_diag.path, sizeof(g_diag.path), "%s/%lld.sock", dir,
                   (long long)getpid());
  return n > 0 && (size_t)n < sizeof(g_diag.path);
}

// A bind that finds no directory creates it and binds again: the user has
// none before their first program, and the last program out removes it
// (remove_empty_dir), possibly between prepare_path and this bind.
static bool bind_listener(int fd, const struct sockaddr_un *addr) {
  if (bind(fd, (const struct sockaddr *)addr, sizeof(*addr)) == 0) {
    return true;
  }
  char dir[sizeof(g_diag.path)];
  if (errno != ENOENT || !socket_dir(dir, sizeof(dir)) ||
      !ensure_private_dir(dir)) {
    return false;
  }
  return bind(fd, (const struct sockaddr *)addr, sizeof(*addr)) == 0;
}

static int listen_on(const char *path) {
  struct sockaddr_un addr = {.sun_family = AF_UNIX};
  memcpy(addr.sun_path, path, strlen(path) + 1); // prepare_path bounded it
  int fd = socket(AF_UNIX, SOCK_STREAM, 0);
  if (fd < 0) {
    return -1;
  }
  set_cloexec(fd);
  (void)unlink(path); // a dead process that had this pid left it behind
  if (!bind_listener(fd, &addr) || listen(fd, OSP_DIAG_MAX_CLIENTS) != 0) {
    (void)close(fd);
    return -1;
  }
  return fd;
}

// The last program out takes the per-user directory with it, so a scratch
// TMPDIR is left as it was found. A neighbour's socket keeps it: rmdir refuses
// a directory that is not empty, and that is the only failure expected here.
static void remove_empty_dir(void) {
  char dir[sizeof(g_diag.path)];
  if (socket_dir(dir, sizeof(dir))) {
    (void)rmdir(dir);
  }
}

static bool open_wake_pipe(void) {
  if (pipe(g_diag.wake) != 0) {
    return false;
  }
  for (int i = 0; i < 2; i++) {
    set_cloexec(g_diag.wake[i]);
    (void)fcntl(g_diag.wake[i], F_SETFL, O_NONBLOCK);
  }
  return true;
}

// Every slot free. Until this runs the slots are zero-initialized, so each one
// would claim fd 0 — and closing "its" socket would close the program's stdin.
static void forget_clients(void) {
  for (int i = 0; i < OSP_DIAG_MAX_CLIENTS; i++) {
    g_diag.clients[i] = (DiagClient){.fd = -1, .used = 0};
  }
}

// Closes only what open_endpoint opened, so it is safe once forget_clients ran.
static void close_fds(void) {
  for (int i = 0; i < OSP_DIAG_MAX_CLIENTS; i++) {
    if (g_diag.clients[i].fd >= 0) {
      (void)close(g_diag.clients[i].fd);
    }
  }
  forget_clients();
  int fds[3] = {g_diag.listen_fd, g_diag.wake[0], g_diag.wake[1]};
  for (int i = 0; i < 3; i++) {
    if (fds[i] >= 0) {
      (void)close(fds[i]);
    }
  }
  g_diag.listen_fd = g_diag.wake[0] = g_diag.wake[1] = -1;
}

// The listener takes no signal meant for the program: it is created with every
// signal blocked and inherits that mask.
static bool spawn_listener(void) {
  sigset_t all, old;
  sigfillset(&all);
  pthread_sigmask(SIG_BLOCK, &all, &old);
  int rc = pthread_create(&g_diag.thread, NULL, listener_main, NULL);
  pthread_sigmask(SIG_SETMASK, &old, NULL);
  return rc == 0;
}

static void diag_at_exit(void) { osp_diag_stop(); }

static bool run_listener(void) {
  g_diag.owner = getpid();
  g_running = true;
  if (spawn_listener()) {
    return true;
  }
  g_running = false;
  close_fds();
  (void)unlink(g_diag.path);
  remove_empty_dir();
  return false;
}

static bool open_endpoint(void) {
  forget_clients();
  if (!open_wake_pipe()) {
    return false;
  }
  g_diag.listen_fd = listen_on(g_diag.path);
  return g_diag.listen_fd >= 0;
}

// A forked child inherits g_running but not the listener thread, and a lock the
// listener held at the fork stays held in the child: it gets no endpoint
// rather than a deadlocked one.
bool osp_diag_start(void) {
  static bool exit_hook = false;
  if (g_running) {
    return getpid() == g_diag.owner;
  }
  if (diagnostics_disabled() || !prepare_path()) {
    return false; // nothing was opened, so nothing may be closed
  }
  if (!open_endpoint()) {
    close_fds();
    return false;
  }
  if (!run_listener()) {
    return false;
  }
  exit_hook = exit_hook || atexit(diag_at_exit) == 0;
  return true;
}

// A session still running is ended as `exit`: its owner gets the last batch.
void osp_diag_stop(void) {
  if (!g_running || getpid() != g_diag.owner) {
    return;
  }
  osp_diag_finish();
  g_running = false;
  wake_listener();
  pthread_join(g_diag.thread, NULL);
  close_fds();
  (void)unlink(g_diag.path);
  remove_empty_dir();
}

#elif !defined(_WIN32) && !defined(__wasm__)

// A phone app cannot be attached to from a desktop language server.
bool osp_diag_start(void) { return false; }
void osp_diag_stop(void) {}

#else

typedef int osp_diag_endpoint_unused; // profiling is POSIX-only

#endif
