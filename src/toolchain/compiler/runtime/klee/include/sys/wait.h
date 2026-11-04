// Mock <sys/wait.h> for symbolic execution [SYMEX-MOCK-HEADERS]; `waitpid` is
// unmodelled [SYMEX-MOCK-UNSUPPORTED].
#ifndef OSP_HUNT_SYS_WAIT_H
#define OSP_HUNT_SYS_WAIT_H
#include <sys/types.h>
#define WNOHANG 1
#define WIFEXITED(status) (((status) & 0x7f) == 0)
#define WEXITSTATUS(status) (((status) >> 8) & 0xff)
#define WIFSIGNALED(status) (((status) & 0x7f) != 0 && ((status) & 0x7f) != 0x7f)
#define WTERMSIG(status) ((status) & 0x7f)
pid_t waitpid(pid_t pid, int *status, int options);
#endif
