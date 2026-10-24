// Mock <sys/random.h> for symbolic execution [SYMEX-MOCK-HEADERS]: entropy is
// a fresh symbolic object `entropy.<n>` [SYMEX-MOCK-STATELESS].
#ifndef OSP_HUNT_SYS_RANDOM_H
#define OSP_HUNT_SYS_RANDOM_H
#include <sys/types.h>
#define GRND_NONBLOCK 1
ssize_t getrandom(void *out, size_t count, unsigned int flags);
void arc4random_buf(void *out, size_t count);
int getentropy(void *out, size_t count);
#endif
