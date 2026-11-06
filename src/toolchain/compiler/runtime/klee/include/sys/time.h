// Mock <sys/time.h> for symbolic execution [SYMEX-MOCK-HEADERS].
#ifndef OSP_HUNT_SYS_TIME_H
#define OSP_HUNT_SYS_TIME_H
#include <sys/types.h>
struct timeval {
  time_t tv_sec;
  suseconds_t tv_usec;
};
int gettimeofday(struct timeval *out, void *zone);
#endif
