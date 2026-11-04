// Mock <sys/select.h> for symbolic execution [SYMEX-MOCK-HEADERS]; `select`
// is unmodelled [SYMEX-MOCK-UNSUPPORTED].
#ifndef OSP_HUNT_SYS_SELECT_H
#define OSP_HUNT_SYS_SELECT_H
#include <sys/time.h>
#define FD_SETSIZE 1024
typedef struct {
  unsigned long bits[FD_SETSIZE / (8 * sizeof(unsigned long))];
} fd_set;
#define FD_ZERO(set) \
  do { for (unsigned i_ = 0; i_ < sizeof((set)->bits) / sizeof((set)->bits[0]); i_++) (set)->bits[i_] = 0; } while (0)
#define FD_SET(fd, set) ((set)->bits[(fd) / 64] |= 1UL << ((fd) % 64))
#define FD_CLR(fd, set) ((set)->bits[(fd) / 64] &= ~(1UL << ((fd) % 64)))
#define FD_ISSET(fd, set) (((set)->bits[(fd) / 64] >> ((fd) % 64)) & 1UL)
int select(int count, fd_set *readable, fd_set *writable, fd_set *failed, struct timeval *timeout);
#endif
