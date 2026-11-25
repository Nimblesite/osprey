// Mock <sched.h> for symbolic execution [SYMEX-MOCK-HEADERS]: one thread
// explores the program, so yielding hands the processor to nobody.
#ifndef OSP_HUNT_SCHED_H
#define OSP_HUNT_SCHED_H
int sched_yield(void);
#endif
