// osprey_thread.h - One stack size for every runtime thread that runs Osprey code.
#ifndef OSPREY_THREAD_H
#define OSPREY_THREAD_H

#include "memory_hooks.h"

#include <pthread.h>
#include <stdbool.h>
#include <stddef.h>
#include <errno.h>
#include <stdlib.h>

// A deep but finite recursion must end in its own result on every thread that
// runs Osprey code (continuations, fibers, websocket handlers, process
// callbacks), not in a silent bus error at the 512 KiB a default secondary
// thread gets. Address space, committed only as it is touched; the main
// thread's own limit is 8 MiB. [FAILURE-EXPLICIT] [CONCURRENCY-SPAWN-AWAIT]
#define OSPREY_THREAD_STACK_BYTES ((size_t)256u << 20)

typedef struct {
  void *(*body)(void *);
  void *arg;
} OspThreadStart;

// Every thread that runs Osprey code guards its stack before the code runs.
static inline void *osp_thread_guarded(void *start) {
  OspThreadStart begin = *(OspThreadStart *)start;
  free(start);
  osp_stack_guard_arm();
  return begin.body(begin.arg);
}

// Starts `body(arg)` on a thread with that stack, detached when asked.
// Answers 0 or the pthread error.
static inline int osp_thread_start(pthread_t *thread, void *(*body)(void *),
                                   void *arg, bool detached) {
  OspThreadStart *start = malloc(sizeof *start);
  if (start == NULL) {
    return ENOMEM;
  }
  *start = (OspThreadStart){body, arg};
  pthread_attr_t attributes;
  int rc = pthread_attr_init(&attributes);
  if (rc != 0) {
    free(start);
    return rc;
  }
  rc = pthread_attr_setstacksize(&attributes, OSPREY_THREAD_STACK_BYTES);
  if (rc == 0 && detached) {
    rc = pthread_attr_setdetachstate(&attributes, PTHREAD_CREATE_DETACHED);
  }
  if (rc == 0) {
    rc = pthread_create(thread, &attributes, osp_thread_guarded, start);
  }
  pthread_attr_destroy(&attributes);
  if (rc != 0) {
    free(start);
  }
  return rc;
}

#endif
