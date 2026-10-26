// Mock <errno.h> for symbolic execution [SYMEX-MOCK-HEADERS]. `errno` is the
// mock's own variable, so neither the host libc nor KLEE's libc is involved.
#ifndef OSP_HUNT_ERRNO_H
#define OSP_HUNT_ERRNO_H
extern int osp_hunt_errno;
#define errno osp_hunt_errno
#define EINTR 4
#define EIO 5
#define EBADF 9
#define ECHILD 10
#define ENFILE 23
#define EMFILE 24
#define EAGAIN 11
#define ENOMEM 12
#define EACCES 13
#define EEXIST 17
#define ENOTDIR 20
#define EISDIR 21
#define EINVAL 22
#define EPIPE 32
#define ERANGE 34
#define ENOSYS 38
#define ENOENT 2
#endif
