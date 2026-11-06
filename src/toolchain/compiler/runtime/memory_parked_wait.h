// Waiting without holding up the collector [MEM-GC-SAFEPOINT]. A thread blocked
// here cannot change its stack, so it counts as parked. Its registers are spilled
// at the bottom of what the collector scans, so a pointer living only in a
// callee-saved register stays a root.
#ifndef OSPREY_MEMORY_PARKED_WAIT_H
#define OSPREY_MEMORY_PARKED_WAIT_H

#include "memory_hooks.h"
#include <pthread.h>
#include <setjmp.h>
#include <time.h>

#if defined(__APPLE__)
#define OSP_SPILL_REGISTERS(buffer) ((void)_setjmp(buffer))
#else
#define OSP_SPILL_REGISTERS(buffer) ((void)setjmp(buffer))
#endif

// pthread_cond_wait, or pthread_cond_timedwait when `deadline` is set.
static int osp_parked_wait(pthread_cond_t *cond, pthread_mutex_t *lock, const struct timespec *deadline) {
  jmp_buf registers;
  OSP_SPILL_REGISTERS(registers);
  osp_mem_baton_park(&registers);
  int result = deadline ? pthread_cond_timedwait(cond, lock, deadline) : pthread_cond_wait(cond, lock);
  osp_mem_baton_unpark();
  return result;
}

#endif // OSPREY_MEMORY_PARKED_WAIT_H
