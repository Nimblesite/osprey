// Mock <fcntl.h> for symbolic execution [SYMEX-MOCK-HEADERS]: descriptor
// flags are accepted and ignored, there being no descriptors to flag.
#ifndef OSP_HUNT_FCNTL_H
#define OSP_HUNT_FCNTL_H
#define O_RDONLY 0
#define O_WRONLY 1
#define O_RDWR 2
#define O_NONBLOCK 04000
#define F_GETFL 3
#define F_SETFL 4
int fcntl(int fd, int command, ...);
#endif
