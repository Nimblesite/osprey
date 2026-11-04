// Mock <signal.h> for symbolic execution [SYMEX-MOCK-HEADERS]; `kill` is
// unmodelled [SYMEX-MOCK-UNSUPPORTED].
#ifndef OSP_HUNT_SIGNAL_H
#define OSP_HUNT_SIGNAL_H
#include <sys/types.h>
#define SIGINT 2
#define SIGKILL 9
#define SIGPIPE 13
#define SIGTERM 15
typedef int sig_atomic_t;
int kill(pid_t pid, int signal);
#endif
