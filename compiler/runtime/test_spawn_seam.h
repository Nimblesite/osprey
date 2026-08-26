// Deterministic fork and thread-creation failure, for the two teardown arms
// that run when the kernel refuses a new process or a new thread.
//
// `spawn_process_with_handler` has four half-built teardown paths. The fork one
// is the only path that has already opened both pipes; the thread one has
// opened both pipes AND started a real child that nothing will ever reap. If it forgets
// either pair, a long-lived program under process pressure runs its descriptor
// table down to nothing while every call politely reports -6. Nothing an
// argument can express provokes it: fork fails on kernel state, not on input.
//
// RLIMIT_NPROC is the portable-looking alternative and it is not a seam. It is
// per-real-uid, so it is a property of the machine the test happens to run on:
// a root CI container ignores it entirely and the assertion silently passes on
// a fork that never failed. Worse, the usual dodge -- fork first, cap the
// child, spawn there -- proves nothing at all, because the parent's descriptor
// table is a different table and stays intact however much the child leaks.
//
// Defining `fork` in a test translation unit replaces it for EVERY object in
// that binary at static-link time, on both Mach-O and ELF, so the runtime's
// call lands here while libc's own internal callers (popen, posix_spawn) keep
// their private one. Disarmed -- the default -- it forwards to the real fork,
// so the same suite's successful spawns are untouched and the failure is
// removable: the spawn after `osp_fork_allow()` proves the runtime recovered
// rather than merely reported.
//
// SINGLE-THREADED ARMING ONLY. The switch is a plain global; arm and disarm it
// from the one thread that then calls the runtime, never while a monitor
// thread could fork.
#ifndef OSPREY_TEST_SPAWN_SEAM_H
#define OSPREY_TEST_SPAWN_SEAM_H

#include <dlfcn.h>
#include <errno.h>
#include <pthread.h>
#include <sys/types.h>
#include <unistd.h>

static int osp_fork_denied;
static pid_t (*osp_real_fork)(void);

// Every fork in this binary, the runtime's included.
pid_t fork(void) {
  if (osp_fork_denied) {
    errno = EAGAIN; // what a kernel out of process slots reports
    return -1;
  }
  if (osp_real_fork == NULL) {
    // dlsym cannot recurse here the way it can in an allocator wrapper: it
    // does not fork.
    *(void **)&osp_real_fork = dlsym(RTLD_NEXT, "fork");
  }
  if (osp_real_fork == NULL) {
    errno = ENOSYS;
    return -1;
  }
  return osp_real_fork();
}

static inline void osp_fork_deny(void) { osp_fork_denied = 1; }
static inline void osp_fork_allow(void) { osp_fork_denied = 0; }

// The same trick for the monitor thread. `pthread_create` fails for reasons no
// argument controls either, and the -5 arm is the only one that must undo a
// child process as well as its pipes -- an unmonitored child is a zombie for
// the life of the program.
static int osp_thread_denied;
static int (*osp_real_pthread_create)(pthread_t *, const pthread_attr_t *,
                                      void *(*)(void *), void *);

// A path the denied thread waits for before it answers. The runtime forks and
// then immediately creates the monitor thread, so a child that installs signal
// handlers of its own has usually NOT reached them yet when the teardown's
// signal arrives -- and a SIGTERM that lands on a child still running the
// pre-exec code kills it on the default disposition. That makes a
// signal-ignoring adversary a coin toss instead of a test. Waiting here for a
// marker the child writes after its handlers are in place closes the race from
// the one place that can: inside the failure being injected.
static const char *osp_thread_ready_marker;

enum { OSP_READY_POLL_US = 1000, OSP_READY_POLL_LIMIT = 5000 };

int pthread_create(pthread_t *thread, const pthread_attr_t *attr,
                   void *(*start)(void *), void *arg) {
  if (osp_thread_denied) {
    for (int i = 0; osp_thread_ready_marker != NULL &&
                    i < OSP_READY_POLL_LIMIT &&
                    access(osp_thread_ready_marker, F_OK) != 0;
         i++) {
      usleep(OSP_READY_POLL_US);
    }
    return EAGAIN; // what a process out of thread slots reports
  }
  if (osp_real_pthread_create == NULL) {
    *(void **)&osp_real_pthread_create = dlsym(RTLD_NEXT, "pthread_create");
  }
  if (osp_real_pthread_create == NULL) {
    return ENOSYS;
  }
  return osp_real_pthread_create(thread, attr, start, arg);
}

static inline void osp_thread_deny(void) {
  osp_thread_ready_marker = NULL;
  osp_thread_denied = 1;
}

// Deny, but only once `marker` exists: the spawned child writes it after it is
// in the state the teardown is being tested against.
static inline void osp_thread_deny_when_ready(const char *marker) {
  osp_thread_ready_marker = marker;
  osp_thread_denied = 1;
}

static inline void osp_thread_allow(void) {
  osp_thread_denied = 0;
  osp_thread_ready_marker = NULL;
}

// And for the per-record mutex. `pthread_mutex_init` fails for the same kind
// of reason -- the system is out of something -- and the arm that handles it
// is the one place a record must be freed WITHOUT destroying a mutex, because
// there is no initialised mutex to destroy. Getting that backwards is
// undefined behaviour that no input can provoke and no reader can see.
static int osp_mutex_init_denied;
static int (*osp_real_mutex_init)(pthread_mutex_t *,
                                  const pthread_mutexattr_t *);

int pthread_mutex_init(pthread_mutex_t *mutex,
                       const pthread_mutexattr_t *attr) {
  if (osp_mutex_init_denied) {
    return ENOMEM;
  }
  if (osp_real_mutex_init == NULL) {
    *(void **)&osp_real_mutex_init = dlsym(RTLD_NEXT, "pthread_mutex_init");
  }
  if (osp_real_mutex_init == NULL) {
    return ENOSYS;
  }
  return osp_real_mutex_init(mutex, attr);
}

static inline void osp_mutex_init_deny(void) { osp_mutex_init_denied = 1; }
static inline void osp_mutex_init_allow(void) { osp_mutex_init_denied = 0; }

#endif // OSPREY_TEST_SPAWN_SEAM_H
