// osprey_thread.h - One stack size for every runtime thread that runs Osprey code.
#ifndef OSPREY_THREAD_H
#define OSPREY_THREAD_H

#include <pthread.h>
#include <stdbool.h>
#include <stddef.h>

// A deep but finite recursion must end in its own result on every thread that
// runs Osprey code (continuations, fibers, websocket handlers, process
// callbacks), not in a silent bus error at the 512 KiB a default secondary
// thread gets. Address space, committed only as it is touched; the main
// thread's own limit is 8 MiB. [FAILURE-EXPLICIT] [CONCURRENCY-SPAWN-AWAIT]
#define OSPREY_THREAD_STACK_BYTES ((size_t)256u << 20)

// Starts `body(arg)` on a thread with that stack, detached when asked.
// Answers 0 or the pthread error.
static inline int osp_thread_start(pthread_t *thread, void *(*body)(void *),
                                   void *arg, bool detached) {
  pthread_attr_t attributes;
  int rc = pthread_attr_init(&attributes);
  if (rc != 0) {
    return rc;
  }
  rc = pthread_attr_setstacksize(&attributes, OSPREY_THREAD_STACK_BYTES);
  if (rc == 0 && detached) {
    rc = pthread_attr_setdetachstate(&attributes, PTHREAD_CREATE_DETACHED);
  }
  if (rc == 0) {
    rc = pthread_create(thread, &attributes, body, arg);
  }
  pthread_attr_destroy(&attributes);
  return rc;
}

#endif
