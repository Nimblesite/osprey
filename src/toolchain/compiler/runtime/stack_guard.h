// A run whose recursion exhausts a thread's stack ends naming the cause
// [FAILURE-EXPLICIT] (#363). Included once by each memory backend, beside the
// run-parent watch: osp_mem_boot arms the main thread and osp_thread_start every
// thread that runs Osprey code.
#ifndef OSPREY_STACK_GUARD_H
#define OSPREY_STACK_GUARD_H

#include "memory_hooks.h"
#include "memory_refused.h"

#if (defined(__APPLE__) || defined(__linux__)) && !defined(OSP_SYMBOLIC)
#include <pthread.h>
#include <signal.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

// The handler's own stack: the faulting one has no room left to run it on.
#define OSP_STACK_GUARD_ALT_BYTES ((size_t)64 << 10)
// A fault this far below the stack's end is still the stack: one large frame
// can step over the guard page without touching it.
#define OSP_STACK_GUARD_REACH ((uintptr_t)16 << 20)
// ...and this far above it, inside the last pages the stack itself holds.
#define OSP_STACK_GUARD_SLACK ((uintptr_t)64 << 10)

static _Thread_local uintptr_t g_stack_low; // this thread's lowest stack address; 0 unguarded
static pthread_once_t g_stack_guard_once = PTHREAD_ONCE_INIT;
static pthread_key_t g_stack_guard_key;
static struct sigaction g_stack_guard_previous[2]; // SIGSEGV, SIGBUS

static const char STACK_EXHAUSTED[] =
    "osprey: stack exhausted: a call went deeper than the thread's stack allows\n";

static uintptr_t stack_guard_low(void) {
#if defined(__APPLE__)
  pthread_t self = pthread_self();
  void *top = pthread_get_stackaddr_np(self);
  return (uintptr_t)top - pthread_get_stacksize_np(self);
#else
  pthread_attr_t attributes;
  void *low = NULL;
  size_t size = 0;
  if (pthread_getattr_np(pthread_self(), &attributes) != 0) {
    return 0;
  }
  (void)pthread_attr_getstack(&attributes, &low, &size);
  (void)pthread_attr_destroy(&attributes);
  return (uintptr_t)low;
#endif
}

static int stack_guard_slot(int signal_number) { return signal_number == SIGSEGV ? 0 : 1; }

// A fault that is not a stack's goes to the handler installed before the guard,
// and the guard stays installed: a host that recovers from its own faults keeps
// the guard too. With no handler before it, the default disposition comes back
// and the re-fault ends the run as it would have without the guard.
static void stack_guard_forward(int signal_number, siginfo_t *info, void *context) {
  const struct sigaction *previous = &g_stack_guard_previous[stack_guard_slot(signal_number)];
  bool with_info = (previous->sa_flags & SA_SIGINFO) != 0;
  if (with_info && previous->sa_sigaction != NULL) {
    previous->sa_sigaction(signal_number, info, context);
  } else if (!with_info && previous->sa_handler != SIG_DFL && previous->sa_handler != SIG_IGN) {
    previous->sa_handler(signal_number);
  } else {
    (void)sigaction(signal_number, previous, NULL);
  }
}

// Broken-code quarantine [FAILURE-EXPLICIT]: recursion has no Result to fail
// into, and the fault already happened. A fault on this thread's stack ends the
// run with the memory status, stdout flushed when no other thread holds it. Any
// other fault is forwarded above.
static void stack_guard_fault(int signal_number, siginfo_t *info, void *context) {
  uintptr_t at = (uintptr_t)info->si_addr;
  uintptr_t low = g_stack_low;
  bool overflow = low != 0 && at + OSP_STACK_GUARD_REACH >= low && at < low + OSP_STACK_GUARD_SLACK;
  if (!overflow) {
    stack_guard_forward(signal_number, info, context);
    return;
  }
  if (write(STDERR_FILENO, STACK_EXHAUSTED, sizeof STACK_EXHAUSTED - 1) < 0) {
    // Nowhere else to say it: the status below still names the stop.
  }
  if (ftrylockfile(stdout) == 0) {
    (void)fflush(stdout);
    funlockfile(stdout);
  }
  _exit(OSP_HEAP_EXHAUSTED_STATUS);
}

static void stack_guard_release(void *alternate) {
  stack_t off = {.ss_flags = SS_DISABLE};
  (void)sigaltstack(&off, NULL);
  free(alternate);
}

static void stack_guard_install(void) {
  (void)pthread_key_create(&g_stack_guard_key, stack_guard_release);
  struct sigaction action;
  memset(&action, 0, sizeof action);
  action.sa_sigaction = stack_guard_fault;
  action.sa_flags = SA_SIGINFO | SA_ONSTACK;
  (void)sigemptyset(&action.sa_mask);
  (void)sigaction(SIGSEGV, &action, &g_stack_guard_previous[0]);
  (void)sigaction(SIGBUS, &action, &g_stack_guard_previous[1]);
}

// Gives the calling thread an alternate signal stack and records where its own
// stack ends. A thread that already has one (a sanitizer's, a host's) keeps it;
// one that cannot have one keeps the default fault.
void osp_stack_guard_arm(void) {
  (void)pthread_once(&g_stack_guard_once, stack_guard_install);
  if (g_stack_low != 0) {
    return;
  }
  stack_t current;
  if (sigaltstack(NULL, &current) == 0 && !(current.ss_flags & SS_DISABLE)) {
    g_stack_low = stack_guard_low();
    return;
  }
  void *alternate = malloc(OSP_STACK_GUARD_ALT_BYTES);
  stack_t stack = {.ss_sp = alternate, .ss_size = OSP_STACK_GUARD_ALT_BYTES, .ss_flags = 0};
  if (alternate == NULL || sigaltstack(&stack, NULL) != 0) {
    free(alternate);
    return;
  }
  (void)pthread_setspecific(g_stack_guard_key, alternate);
  g_stack_low = stack_guard_low();
}
#else
void osp_stack_guard_arm(void) {}
#endif

#endif
