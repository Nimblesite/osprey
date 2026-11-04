// Collection among threads that run at once [MEM-GC-SAFEPOINT]. Included after
// gc_collect_locked; the heap lock is held throughout, and every wait gives it up.
//
// A collection runs only while every other registered thread is parked, and a
// thread parks only where it holds no lock and touches no managed word: blocked
// in the runtime, or here at an allocation. So the collector asks, the others stop
// at their next allocation, and whichever arrives last collects.
//
// Asking must never become a deadlock. A thread can be in a loop that never
// allocates, or in a system call waiting on the very thread that asked, so the
// wait is bounded. Then the collection is put off and retried after more
// allocation, each time further away, which costs memory rather than progress.
#ifndef OSPREY_GC_SAFEPOINT_H
#define OSPREY_GC_SAFEPOINT_H
#include <errno.h>
#include <setjmp.h>
#include <time.h>

#define OSP_GC_SAFEPOINT_WAIT_NS 10000000L
#define OSP_GC_NS_PER_SECOND 1000000000L
#define OSP_GC_RETRY_GAP_MAX ((size_t)256u << 20)

static pthread_cond_t g_safepoint = PTHREAD_COND_INITIALIZER;
static int g_collection_wanted = 0;
static size_t g_retry_at = 0;  // no collection is attempted below this many bytes
static size_t g_retry_gap = 0; // how far the last put-off moved g_retry_at

static void gc_safepoint_deadline(struct timespec *deadline) {
  clock_gettime(CLOCK_REALTIME, deadline);
  deadline->tv_nsec += OSP_GC_SAFEPOINT_WAIT_NS;
  if (deadline->tv_nsec >= OSP_GC_NS_PER_SECOND) {
    deadline->tv_sec += 1;
    deadline->tv_nsec -= OSP_GC_NS_PER_SECOND;
  }
}

// Park here until the others have parked too, or the wait runs out. Waiters stay
// parked for each other, so the last to arrive sees that and goes on to collect.
static void gc_safepoint_wait(OspGcBaton *self) {
  jmp_buf registers;
  (void)setjmp(registers);
  self->parked_at = (uintptr_t)&registers;
  struct timespec deadline;
  gc_safepoint_deadline(&deadline);
  int in_time = 1;
  while (in_time && g_collection_wanted && !gc_may_collect()) {
    in_time = pthread_cond_timedwait(&g_safepoint, &g_lock, &deadline) != ETIMEDOUT;
  }
  self->parked_at = 0;
}

static void gc_collection_settled(size_t retry_gap) {
  g_collection_wanted = 0;
  g_retry_gap = retry_gap;
  g_retry_at = retry_gap ? g_bytes + retry_gap : 0;
  pthread_cond_broadcast(&g_safepoint);
}

static void gc_put_off_collection(void) {
  size_t doubled = g_retry_gap ? g_retry_gap * 2 : g_min_heap;
  gc_collection_settled(doubled < OSP_GC_RETRY_GAP_MAX ? doubled : OSP_GC_RETRY_GAP_MAX);
}

// Called at an allocation when a collection is due or another thread wants one.
static void gc_safepoint(void) {
  OspGcBaton *self = gc_my_baton();
  if (g_off || self == NULL) {
    return;
  }
  g_collection_wanted = 1;
  if (!gc_may_collect()) {
    gc_safepoint_wait(self);
  }
  if (!g_collection_wanted) {
    return; // Another thread collected, or put it off, while this one was parked.
  }
  if (gc_may_collect()) {
    gc_collect_locked();
    gc_collection_settled(0);
  } else {
    gc_put_off_collection();
  }
}

// A thread that stops running may be the last one a waiter needs.
static void gc_wake_safepoint(void) {
  if (g_collection_wanted) {
    pthread_cond_broadcast(&g_safepoint);
  }
}
#endif
