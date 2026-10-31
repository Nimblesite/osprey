#ifndef OSPREY_SYSTEM_PROCESS_POSIX_H
#define OSPREY_SYSTEM_PROCESS_POSIX_H

// Private implementation included once by system_runtime.c's POSIX branch.
static bool forward_process_output(ProcessResult *proc, int fd, int64_t event) {
  char buffer[1024];
  ssize_t bytes = read(fd, buffer, sizeof(buffer) - 1);
  if (bytes < 0 && errno == EINTR)
    return true;
  if (bytes <= 0)
    return false;
  buffer[bytes] = '\0';
  if (proc->handler)
    proc->handler(proc->process_id, event, buffer);
  return true;
}

// Implements [BUILTIN-PROCESS]: buffered bytes must precede the exit event.
static void drain_process_output(ProcessResult *proc) {
  while (forward_process_output(proc, proc->stdout_pipe[0], PROCESS_STDOUT_DATA)) {
  }
  while (forward_process_output(proc, proc->stderr_pipe[0], PROCESS_STDERR_DATA)) {
  }
}

static void prepare_process_monitor(ProcessResult *proc) {
  close(proc->stdout_pipe[1]);
  close(proc->stderr_pipe[1]);
  fcntl(proc->stdout_pipe[0], F_SETFL, O_NONBLOCK);
  fcntl(proc->stderr_pipe[0], F_SETFL, O_NONBLOCK);
}

static void forward_ready_output(ProcessResult *proc, const fd_set *ready) {
  if (FD_ISSET(proc->stdout_pipe[0], ready))
    forward_process_output(proc, proc->stdout_pipe[0], PROCESS_STDOUT_DATA);
  if (FD_ISSET(proc->stderr_pipe[0], ready))
    forward_process_output(proc, proc->stderr_pipe[0], PROCESS_STDERR_DATA);
}

static void poll_process_output(ProcessResult *proc) {
  fd_set ready;
  FD_ZERO(&ready);
  FD_SET(proc->stdout_pipe[0], &ready);
  FD_SET(proc->stderr_pipe[0], &ready);
  struct timeval timeout = {.tv_sec = 0, .tv_usec = 100000};
  int max_fd = proc->stdout_pipe[0] > proc->stderr_pipe[0]
                   ? proc->stdout_pipe[0]
                   : proc->stderr_pipe[0];
  if (select(max_fd + 1, &ready, NULL, NULL, &timeout) > 0)
    forward_ready_output(proc, &ready);
}

static void complete_process(ProcessResult *proc, int64_t code) {
  pthread_mutex_lock(&proc->mutex);
  proc->is_running = false;
  proc->exit_code = code;
  pthread_mutex_unlock(&proc->mutex);
  drain_process_output(proc);
  if (proc->handler) {
    char text[32];
    snprintf(text, sizeof(text), "%lld", (long long)code);
    proc->handler(proc->process_id, PROCESS_EXIT, text);
  }
}

static int64_t process_status_code(const ProcessResult *proc, int status) {
  if (WIFEXITED(status))
    return WEXITSTATUS(status);
  return WIFSIGNALED(status) ? -1 : proc->exit_code;
}

static void poll_process_exit(ProcessResult *proc) {
  int status;
  pid_t result = waitpid(proc->pid, &status, WNOHANG);
  if (result > 0)
    complete_process(proc, process_status_code(proc, status));
  else if (result < 0 && errno != EINTR)
    // ECHILD: the host reaped the child, so its status is unavailable.
    complete_process(proc, -1);
}

static void *process_monitor_thread(void *arg) {
  ProcessResult *proc = (ProcessResult *)arg;
  prepare_process_monitor(proc);
  while (proc->is_running) {
    poll_process_output(proc);
    poll_process_exit(proc);
  }
  close(proc->stdout_pipe[0]);
  close(proc->stderr_pipe[0]);
  return NULL;
}

static void close_pipe_pair(const int fds[2]) {
  close(fds[0]);
  close(fds[1]);
}

// Caller holds process_mutex; no monitor can see this half-built record.
static int64_t abandon_process(ProcessResult *proc, int64_t code) {
  free(proc->command);
  pthread_mutex_destroy(&proc->mutex);
  free(proc);
  pthread_mutex_unlock(&process_mutex);
  return code;
}

static ProcessResult *create_process(int64_t id, ProcessEventHandler handler) {
  ProcessResult *proc = malloc(sizeof(ProcessResult));
  if (!proc)
    return NULL;
  proc->process_id = id;
  proc->exit_code = -999;
  proc->is_running = true;
  proc->handler = handler;
  if (pthread_mutex_init(&proc->mutex, NULL) != 0) {
    free(proc);
    return NULL;
  }
  return proc;
}

static int64_t prepare_process(ProcessResult *proc, const char *command) {
  proc->command = strdup(command);
  if (!proc->command)
    return -3;
  if (pipe(proc->stdout_pipe) != 0)
    return -4;
  if (pipe(proc->stderr_pipe) != 0) {
    close_pipe_pair(proc->stdout_pipe);
    return -4;
  }
  return 0;
}

static void execute_process_child(const ProcessResult *proc) {
  close(proc->stdout_pipe[0]);
  close(proc->stderr_pipe[0]);
  dup2(proc->stdout_pipe[1], STDOUT_FILENO);
  dup2(proc->stderr_pipe[1], STDERR_FILENO);
  close(proc->stdout_pipe[1]);
  close(proc->stderr_pipe[1]);
  execl("/bin/sh", "sh", "-c", proc->command, (char *)NULL);
  _exit(127);
}

static int64_t abandon_unmonitored_process(ProcessResult *proc) {
  close_pipe_pair(proc->stdout_pipe);
  close_pipe_pair(proc->stderr_pipe);
  // An unmonitored child that ignores TERM must not park process_mutex forever.
  kill(proc->pid, SIGKILL);
  while (waitpid(proc->pid, NULL, 0) < 0 && errno == EINTR) {
  }
  processes[proc->process_id] = NULL;
  return abandon_process(proc, -5);
}

static int64_t launch_process(ProcessResult *proc) {
  proc->pid = fork();
  if (proc->pid == 0)
    execute_process_child(proc);
  if (proc->pid < 0) {
    close_pipe_pair(proc->stdout_pipe);
    close_pipe_pair(proc->stderr_pipe);
    return abandon_process(proc, -6);
  }
  processes[proc->process_id] = proc;
  // Callbacks allocate concurrently with the caller: lock the shared heap first.
  osp_mem_notify_multithreaded();
  if (pthread_create(&proc->monitor_thread, NULL, process_monitor_thread, proc) != 0)
    return abandon_unmonitored_process(proc);
  pthread_mutex_unlock(&process_mutex);
  return proc->process_id;
}

static int64_t spawn_allocated_process(int64_t id, const char *command,
                                      ProcessEventHandler handler) {
  ProcessResult *proc = create_process(id, handler);
  if (!proc) {
    pthread_mutex_unlock(&process_mutex);
    return -3;
  }
  int64_t error = prepare_process(proc, command);
  return error != 0 ? abandon_process(proc, error) : launch_process(proc);
}

// Implements [BUILTIN-PROCESS-FAILURE]: argument checks precede capacity checks;
// failed spawns release every resource and spend exactly one handle number.
int64_t spawn_process_with_handler(const char *command, ProcessEventHandler handler) {
  if (!command || !handler)
    return -1;
  pthread_mutex_lock(&process_mutex);
  int64_t id = next_process_id++;
  if (id >= MAX_PROCESSES) {
    pthread_mutex_unlock(&process_mutex);
    return -2;
  }
  return spawn_allocated_process(id, command, handler);
}

#endif
