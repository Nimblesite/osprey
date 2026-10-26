// Mock <unistd.h> for symbolic execution [SYMEX-MOCK-HEADERS]. `read` on fd 0
// is the symbolic stdin stream; `write` reaches the output sink; process and
// descriptor plumbing is unmodelled [SYMEX-MOCK-UNSUPPORTED].
#ifndef OSP_HUNT_UNISTD_H
#define OSP_HUNT_UNISTD_H
#include <sys/types.h>
#define STDIN_FILENO 0
#define STDOUT_FILENO 1
#define STDERR_FILENO 2
ssize_t read(int fd, void *out, size_t count);
ssize_t write(int fd, const void *data, size_t count);
int close(int fd);
int isatty(int fd);
int dup2(int from, int to);
int pipe(int fds[2]);
pid_t fork(void);
int execl(const char *path, const char *argument, ...);
int execvp(const char *file, char *const argv[]);
void _exit(int status) __attribute__((noreturn));
unsigned int sleep(unsigned int seconds);
int usleep(unsigned int microseconds);
#endif
