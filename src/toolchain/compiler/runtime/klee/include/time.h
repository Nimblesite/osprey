// Mock <time.h> for symbolic execution [SYMEX-MOCK-HEADERS]: the clock is a
// symbolic, non-decreasing value named `clock.<n>` [SYMEX-MOCK-STATELESS].
#ifndef OSP_HUNT_TIME_H
#define OSP_HUNT_TIME_H
#include <sys/types.h>
#define CLOCK_REALTIME 0
#define CLOCK_MONOTONIC 1
struct timespec {
  time_t tv_sec;
  long tv_nsec;
};
int clock_gettime(clockid_t clock, struct timespec *out);
time_t time(time_t *out);
#endif
