// Calls the mock does not model [SYMEX-MOCK-UNSUPPORTED]: processes, threads,
// descriptor plumbing and the filesystem metadata API. Each stops its path
// with `unmodelled.err` naming the call, so the report says what the program
// needs rather than exploring a lie.
#include "hunt_mock.h"

#include <klee/klee.h>
#include <pthread.h>
#include <signal.h>
#include <stdio.h>
#include <sys/select.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

void osp_hunt_unsupported(const char *file, int line, const char *name) {
  osp_hunt_note(OSP_HUNT_CALL_UNSUPPORTED);
  char message[OSP_HUNT_NAME_MAX];
  osp_hunt_name(message, "unmodelled: ", name);
  klee_report_error(file, line, message, "unmodelled.err");
}

#define UNSUPPORTED(name) osp_hunt_unsupported(__FILE__, __LINE__, name)

pid_t fork(void) { UNSUPPORTED("fork"); }

int execl(const char *path, const char *argument, ...) {
  (void)path;
  (void)argument;
  UNSUPPORTED("execl");
}

int execvp(const char *file, char *const argv[]) {
  (void)file;
  (void)argv;
  UNSUPPORTED("execvp");
}

int dup2(int from, int to) {
  (void)from;
  (void)to;
  UNSUPPORTED("dup2");
}

int pipe(int fds[2]) {
  (void)fds;
  UNSUPPORTED("pipe");
}

FILE *popen(const char *command, const char *mode) {
  (void)command;
  (void)mode;
  UNSUPPORTED("popen");
}

int pclose(FILE *stream) {
  (void)stream;
  UNSUPPORTED("pclose");
}

pid_t waitpid(pid_t pid, int *status, int options) {
  (void)pid;
  (void)status;
  (void)options;
  UNSUPPORTED("waitpid");
}

int select(int count, fd_set *readable, fd_set *writable, fd_set *failed, struct timeval *timeout) {
  (void)count;
  (void)readable;
  (void)writable;
  (void)failed;
  (void)timeout;
  UNSUPPORTED("select");
}

int kill(pid_t pid, int signal) {
  (void)pid;
  (void)signal;
  UNSUPPORTED("kill");
}

int pthread_create(pthread_t *thread, const pthread_attr_t *attributes, void *(*start)(void *), void *argument) {
  (void)thread;
  (void)attributes;
  (void)start;
  (void)argument;
  UNSUPPORTED("pthread_create");
}

int pthread_join(pthread_t thread, void **result) {
  (void)thread;
  (void)result;
  UNSUPPORTED("pthread_join");
}

int pthread_detach(pthread_t thread) {
  (void)thread;
  UNSUPPORTED("pthread_detach");
}

int stat(const char *path, struct stat *out) {
  (void)path;
  (void)out;
  UNSUPPORTED("stat");
}

int mkdir(const char *path, mode_t mode) {
  (void)path;
  (void)mode;
  UNSUPPORTED("mkdir");
}
