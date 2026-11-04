// Mock <sys/types.h> for symbolic execution [SYMEX-MOCK-HEADERS]: LP64 types.
#ifndef OSP_HUNT_SYS_TYPES_H
#define OSP_HUNT_SYS_TYPES_H
#include <stddef.h>
#include <stdint.h>
typedef long ssize_t;
typedef long off_t;
typedef long time_t;
typedef int pid_t;
typedef unsigned int mode_t;
typedef int clockid_t;
typedef long suseconds_t;
#endif
