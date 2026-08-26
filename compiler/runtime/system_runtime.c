#include <errno.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "memory_hooks.h"

// The process runtime spawns children and streams their output. POSIX uses
// fork/exec/pipe/select; Windows uses the Win32 process APIs from <windows.h>
// (via the compat header). pthreads work on both (winpthreads on Windows).
// The file/JSON/string helpers at the bottom are portable and built everywhere.
//
// wasm32-wasip1 has neither fork/exec nor usable pthreads, so the process half
// is compiled only for native targets — the same split effects_runtime.c makes
// for thread-based continuations. This unit stays in the wasm archive for its
// stdio line-buffering constructor; the portable file half moved to
// file_runtime.c, which is in every archive. [WASM-TARGET]
#ifndef __wasm__
#include <pthread.h>
#ifdef _WIN32
#include "osprey_win_compat.h"
#else
#include <fcntl.h>
#include <signal.h>
#include <sys/select.h>
#include <sys/wait.h>
#include <unistd.h>
#endif
#endif // !__wasm__

// Make stdout line-buffered at startup so a long-running program (e.g. an HTTP
// server that never returns to flush at exit) shows each printed line live,
// whether stdout is a TTY, a pipe, or a file. Line buffering changes only flush
// timing, never the bytes, so captured/differential output is unaffected.
__attribute__((constructor)) static void osprey_stdio_lbf(void) {
  setvbuf(stdout, NULL, _IOLBF, 0);
}

#ifndef __wasm__
// Process event handler function type - Osprey provides this callback
typedef void (*ProcessEventHandler)(int64_t process_id, int64_t event_type,
                                    char *data);

// Event types for process callbacks
#define PROCESS_STDOUT_DATA 1
#define PROCESS_STDERR_DATA 2
#define PROCESS_EXIT 3

// Max concurrently tracked processes (shared by both platform implementations).
#define MAX_PROCESSES 1000

#ifndef _WIN32
// Process result structure
typedef struct {
  int64_t process_id;          // Process ID for tracking
  char *command;               // Command being executed
  int64_t exit_code;           // Process exit code
  bool is_running;             // Process status
  pthread_t monitor_thread;    // Thread monitoring the process
  pthread_mutex_t mutex;       // Mutex for thread safety
  int stdout_pipe[2];          // Pipes for capturing stdout
  int stderr_pipe[2];          // Pipes for capturing stderr
  pid_t pid;                   // Actual process PID
  ProcessEventHandler handler; // Callback for events
} ProcessResult;

// Global process tracking
static ProcessResult *processes[MAX_PROCESSES];
static int64_t next_process_id = 1;
static pthread_mutex_t process_mutex = PTHREAD_MUTEX_INITIALIZER;

// Thread function to monitor process and send callbacks
static void *process_monitor_thread(void *arg) {
  ProcessResult *proc = (ProcessResult *)arg;

  // Close write ends in parent
  close(proc->stdout_pipe[1]);
  close(proc->stderr_pipe[1]);

  // Make pipes non-blocking
  fcntl(proc->stdout_pipe[0], F_SETFL, O_NONBLOCK);
  fcntl(proc->stderr_pipe[0], F_SETFL, O_NONBLOCK);

  char buffer[1024];
  fd_set read_fds;
  struct timeval timeout;

  // Monitor process and send callbacks for output
  while (proc->is_running) {
    FD_ZERO(&read_fds);
    FD_SET(proc->stdout_pipe[0], &read_fds);
    FD_SET(proc->stderr_pipe[0], &read_fds);

    timeout.tv_sec = 0;
    timeout.tv_usec = 100000; // 100ms timeout

    int max_fd = (proc->stdout_pipe[0] > proc->stderr_pipe[0])
                     ? proc->stdout_pipe[0]
                     : proc->stderr_pipe[0];

    int ready = select(max_fd + 1, &read_fds, NULL, NULL, &timeout);

    if (ready > 0) {
      // Read stdout and send callback
      if (FD_ISSET(proc->stdout_pipe[0], &read_fds)) {
        ssize_t bytes = read(proc->stdout_pipe[0], buffer, sizeof(buffer) - 1);
        if (bytes > 0) {
          buffer[bytes] = '\0';

          // Send stdout data to Osprey via callback
          if (proc->handler) {
            proc->handler(proc->process_id, PROCESS_STDOUT_DATA, buffer);
          }
        }
      }

      // Read stderr and send callback
      if (FD_ISSET(proc->stderr_pipe[0], &read_fds)) {
        ssize_t bytes = read(proc->stderr_pipe[0], buffer, sizeof(buffer) - 1);
        if (bytes > 0) {
          buffer[bytes] = '\0';

          // Send stderr data to Osprey via callback
          if (proc->handler) {
            proc->handler(proc->process_id, PROCESS_STDERR_DATA, buffer);
          }
        }
      }
    }

    // Check if process is still running
    int status;
    pid_t result = waitpid(proc->pid, &status, WNOHANG);
    if (result > 0) {
      // Process finished
      pthread_mutex_lock(&proc->mutex);
      proc->is_running = false;
      if (WIFEXITED(status)) {
        proc->exit_code = WEXITSTATUS(status);
      } else if (WIFSIGNALED(status)) {
        proc->exit_code = -1; // Terminated by signal
      }
      pthread_mutex_unlock(&proc->mutex);

      // Send exit event to Osprey
      if (proc->handler) {
        char exit_code_str[32];
        snprintf(exit_code_str, sizeof(exit_code_str), "%lld",
                 (long long)proc->exit_code);
        proc->handler(proc->process_id, PROCESS_EXIT, exit_code_str);
      }
      break;
    } else if (result < 0 && errno != ECHILD) {
      // Error in waitpid
      pthread_mutex_lock(&proc->mutex);
      proc->is_running = false;
      proc->exit_code = -1;
      pthread_mutex_unlock(&proc->mutex);

      // Send error exit event
      if (proc->handler) {
        char error_code[] = "-1";
        proc->handler(proc->process_id, PROCESS_EXIT, error_code);
      }
      break;
    }
  }

  // Clean up pipes
  close(proc->stdout_pipe[0]);
  close(proc->stderr_pipe[0]);

  return NULL;
}

// Close both ends of a pipe pair created for a process that will not run.
static void close_pipe_pair(const int fds[2]) {
  close(fds[0]);
  close(fds[1]);
}

// Release a process record that never reached its monitor thread. The caller
// holds process_mutex and no other thread can see `proc`, so this is the only
// owner. Returns `code` so every half-built teardown is one statement and the
// four paths that share it cannot drift apart.
static int64_t abandon_process(ProcessResult *proc, int64_t code) {
  free(proc->command);
  pthread_mutex_destroy(&proc->mutex);
  free(proc);
  pthread_mutex_unlock(&process_mutex);
  return code;
}

// Spawn process with event handler - similar to HTTP server pattern.
// Implements [BUILTIN-PROCESS] and [BUILTIN-PROCESS-FAILURE]: the argument
// check runs before the capacity check, and every one of the five failure
// codes below leaves the process exactly as the call found it -- no descriptor
// held, no child unreaped, no table slot occupied, one handle number spent.
int64_t spawn_process_with_handler(const char *command, ProcessEventHandler handler) {
  if (!command || !handler) {
    return -1;
  }

  pthread_mutex_lock(&process_mutex);

  int64_t process_id = next_process_id++;
  if (process_id >= MAX_PROCESSES) {
    pthread_mutex_unlock(&process_mutex);
    return -2; // Too many processes
  }

  ProcessResult *proc = malloc(sizeof(ProcessResult));
  if (!proc) {
    pthread_mutex_unlock(&process_mutex);
    return -3; // Memory allocation failed
  }

  // Initialize process structure. The mutex comes FIRST and its result is
  // checked, because every teardown below destroys it: abandoning a record
  // whose mutex was never initialised is undefined, and one whose mutex
  // silently failed to initialise is a record the monitor thread will lock.
  proc->process_id = process_id;
  proc->exit_code = -999; // Not finished yet
  proc->is_running = true;
  proc->handler = handler;
  if (pthread_mutex_init(&proc->mutex, NULL) != 0) {
    free(proc); // nothing to destroy and nothing else owned yet
    pthread_mutex_unlock(&process_mutex);
    return -3; // Memory allocation failed
  }
  // Unchecked, this left `command` NULL on a record the monitor thread and
  // every diagnostic read.
  proc->command = strdup(command);
  if (proc->command == NULL) {
    return abandon_process(proc, -3); // Memory allocation failed
  }

  // Create pipes for stdout and stderr. Checked separately: when the table has
  // room for the first pair and not the second, folding them into one condition
  // leaks the pair that succeeded.
  if (pipe(proc->stdout_pipe) != 0) {
    return abandon_process(proc, -4); // Pipe creation failed
  }
  if (pipe(proc->stderr_pipe) != 0) {
    close_pipe_pair(proc->stdout_pipe);
    return abandon_process(proc, -4); // Pipe creation failed
  }

  // Fork the process
  proc->pid = fork();
  if (proc->pid == 0) {
    // Child process
    close(proc->stdout_pipe[0]); // Close read end
    close(proc->stderr_pipe[0]);

    // Redirect stdout and stderr to pipes
    dup2(proc->stdout_pipe[1], STDOUT_FILENO);
    dup2(proc->stderr_pipe[1], STDERR_FILENO);

    close(proc->stdout_pipe[1]);
    close(proc->stderr_pipe[1]);

    // Execute the command
    execl("/bin/sh", "sh", "-c", command, (char *)NULL);
    _exit(127); // If execl fails
  } else if (proc->pid > 0) {
    // Parent process
    processes[process_id] = proc;

    // The monitor thread fires user output callbacks that allocate on the
    // shared heap concurrently with the caller — lock the memory backend first.
    osp_mem_notify_multithreaded();
    // Create monitoring thread
    if (pthread_create(&proc->monitor_thread, NULL, process_monitor_thread,
                       proc) != 0) {
      // Thread creation failed: nothing will ever reap the child, so end it
      // here rather than leave an unmonitored process behind.
      close_pipe_pair(proc->stdout_pipe);
      close_pipe_pair(proc->stderr_pipe);
      // SIGKILL, not SIGTERM. This child has no monitor and has never produced
      // an observable byte, so there is nothing to shut down gracefully -- and
      // the caller is holding process_mutex across the reap below. A child
      // that ignores TERM would park that wait, and the mutex with it,
      // forever: "the monitor thread could not start" would become a program
      // that never returns. KILL cannot be caught, blocked or ignored, so the
      // reap is bounded.
      kill(proc->pid, SIGKILL);
      while (waitpid(proc->pid, NULL, 0) < 0 && errno == EINTR) {
        // Interrupted before the child was collected; it is still owed to us.
      }
      processes[process_id] = NULL;
      return abandon_process(proc, -5); // Thread creation failed
    }

    pthread_mutex_unlock(&process_mutex);
    return process_id;
  } else {
    // Fork failed
    close_pipe_pair(proc->stdout_pipe);
    close_pipe_pair(proc->stderr_pipe);
    return abandon_process(proc, -6); // Fork failed
  }
}

// Wait for process completion - blocks until process finishes
int64_t await_process(int64_t process_id) {
  if (process_id < 1 || process_id >= MAX_PROCESSES) {
    return -1;
  }

  pthread_mutex_lock(&process_mutex);
  ProcessResult *proc = processes[process_id];
  pthread_mutex_unlock(&process_mutex);

  if (!proc) {
    return -1;
  }

  // Wait for monitor thread to complete
  pthread_join(proc->monitor_thread, NULL);

  return proc->exit_code;
}

// Clean up process resources
void cleanup_process(int64_t process_id) {
  if (process_id < 1 || process_id >= MAX_PROCESSES) {
    return;
  }

  pthread_mutex_lock(&process_mutex);
  ProcessResult *proc = processes[process_id];
  if (proc) {
    processes[process_id] = NULL;

    if (proc->command)
      free(proc->command);
    pthread_mutex_destroy(&proc->mutex);
    free(proc);
  }
  pthread_mutex_unlock(&process_mutex);
}

// Legacy spawn_process function for backward compatibility - now blocking
char *spawn_process(char *command) {
  if (!command) {
    return NULL;
  }

  // Use popen for simple blocking behavior (legacy support)
  FILE *pipe = popen(command, "r");
  if (!pipe) {
    return NULL;
  }

  // Read all output
  char *output = malloc(4096);
  if (!output) {
    pclose(pipe);
    return NULL;
  }

  size_t total_read = 0;
  size_t buffer_size = 4096;
  char buffer[256];

  while (fgets(buffer, sizeof(buffer), pipe) != NULL) {
    size_t len = strlen(buffer);

    // Resize if needed
    if (total_read + len >= buffer_size) {
      buffer_size *= 2;
      output = realloc(output, buffer_size);
      if (!output) {
        pclose(pipe);
        return NULL;
      }
    }

    strcpy(output + total_read, buffer);
    total_read += len;
  }

  output[total_read] = '\0';
  pclose(pipe);

  return output;
}

#else // _WIN32 — Win32 process runtime

// Windows process result: same shape as the POSIX one but with Win32 handles
// instead of pipe fds + pid. The monitor thread (winpthreads) reads the child's
// stdout/stderr pipes and reports exit, mirroring the POSIX implementation.
typedef struct {
  int64_t process_id;
  char *command;
  int64_t exit_code;
  bool is_running;
  pthread_t monitor_thread;
  pthread_mutex_t mutex;
  HANDLE stdout_rd; // read end of child's stdout
  HANDLE stderr_rd; // read end of child's stderr
  HANDLE process;   // child process handle
  ProcessEventHandler handler;
} ProcessResult;

static ProcessResult *processes[MAX_PROCESSES];
static int64_t next_process_id = 1;
static pthread_mutex_t process_mutex = PTHREAD_MUTEX_INITIALIZER;

// Close both ends of a handle pair created for a process that will not run.
static void close_handle_pair(HANDLE first, HANDLE second) {
  CloseHandle(first);
  CloseHandle(second);
}

// Release a process record that never reached its monitor thread. The caller
// holds process_mutex and no other thread can see `proc`, so this is the only
// owner. Returns `code` so every half-built teardown is one statement and the
// four paths that share it cannot drift apart -- and so this half of the file
// releases exactly what the POSIX half does, the per-record mutex included.
static int64_t abandon_process(ProcessResult *proc, int64_t code) {
  free(proc->command);
  pthread_mutex_destroy(&proc->mutex);
  free(proc);
  pthread_mutex_unlock(&process_mutex);
  return code;
}

// Drain whatever is currently readable on a pipe, dispatching it to the handler.
static void drain_pipe(ProcessResult *proc, HANDLE pipe, int64_t event_type) {
  DWORD avail = 0;
  if (!PeekNamedPipe(pipe, NULL, 0, NULL, &avail, NULL) || avail == 0) {
    return;
  }

  char buffer[1024];
  DWORD to_read = avail < sizeof(buffer) - 1 ? avail : (DWORD)(sizeof(buffer) - 1);
  DWORD got = 0;
  if (ReadFile(pipe, buffer, to_read, &got, NULL) && got > 0) {
    buffer[got] = '\0';
    if (proc->handler) {
      proc->handler(proc->process_id, event_type, buffer);
    }
  }
}

static void *process_monitor_thread(void *arg) {
  ProcessResult *proc = (ProcessResult *)arg;

  while (proc->is_running) {
    drain_pipe(proc, proc->stdout_rd, PROCESS_STDOUT_DATA);
    drain_pipe(proc, proc->stderr_rd, PROCESS_STDERR_DATA);

    DWORD wait = WaitForSingleObject(proc->process, 100); // 100ms poll
    if (wait == WAIT_OBJECT_0) {
      // Process exited — drain any final output, then report exit.
      drain_pipe(proc, proc->stdout_rd, PROCESS_STDOUT_DATA);
      drain_pipe(proc, proc->stderr_rd, PROCESS_STDERR_DATA);

      DWORD code = 0;
      GetExitCodeProcess(proc->process, &code);
      pthread_mutex_lock(&proc->mutex);
      proc->is_running = false;
      proc->exit_code = (int64_t)code;
      pthread_mutex_unlock(&proc->mutex);

      if (proc->handler) {
        char exit_code_str[32];
        snprintf(exit_code_str, sizeof(exit_code_str), "%lld",
                 (long long)proc->exit_code);
        proc->handler(proc->process_id, PROCESS_EXIT, exit_code_str);
      }
      break;
    }
  }

  CloseHandle(proc->stdout_rd);
  CloseHandle(proc->stderr_rd);
  return NULL;
}

// The Win32 twin. [BUILTIN-PROCESS-FAILURE] is platform-neutral, so this must
// unwind exactly as the POSIX path does: pipes checked separately so a failed
// second one does not strand the first, the record's mutex destroyed on every
// teardown, and the unmonitorable child both ended and WAITED FOR before its
// handle is released.
int64_t spawn_process_with_handler(const char *command,
                                   ProcessEventHandler handler) {
  if (!command || !handler) {
    return -1;
  }

  pthread_mutex_lock(&process_mutex);
  int64_t process_id = next_process_id++;
  if (process_id >= MAX_PROCESSES) {
    pthread_mutex_unlock(&process_mutex);
    return -2;
  }

  ProcessResult *proc = malloc(sizeof(ProcessResult));
  if (!proc) {
    pthread_mutex_unlock(&process_mutex);
    return -3;
  }

  proc->process_id = process_id;
  proc->exit_code = -999;
  proc->is_running = true;
  proc->handler = handler;
  if (pthread_mutex_init(&proc->mutex, NULL) != 0) {
    free(proc);
    pthread_mutex_unlock(&process_mutex);
    return -3;
  }
  proc->command = strdup(command);
  if (proc->command == NULL) {
    return abandon_process(proc, -3);
  }

  // Inheritable pipes for the child's stdout/stderr. Checked separately for
  // the same reason as the POSIX pipes: one combined condition leaks the pair
  // that succeeded when the second one fails.
  SECURITY_ATTRIBUTES sa = {sizeof(sa), NULL, TRUE};
  HANDLE out_rd = NULL, out_wr = NULL, err_rd = NULL, err_wr = NULL;
  if (!CreatePipe(&out_rd, &out_wr, &sa, 0)) {
    return abandon_process(proc, -4);
  }
  if (!CreatePipe(&err_rd, &err_wr, &sa, 0)) {
    close_handle_pair(out_rd, out_wr);
    return abandon_process(proc, -4);
  }
  // The read ends stay in this process. If they cannot be marked
  // non-inheritable the child would hold a copy of each, and the reads here
  // would never see EOF because the write end is not the only one left open.
  // That is a spawn that can never finish, so it is a spawn that must fail.
  if (!SetHandleInformation(out_rd, HANDLE_FLAG_INHERIT, 0) ||
      !SetHandleInformation(err_rd, HANDLE_FLAG_INHERIT, 0)) {
    close_handle_pair(out_rd, out_wr);
    close_handle_pair(err_rd, err_wr);
    return abandon_process(proc, -4);
  }

  // Build "cmd.exe /c <command>" in a mutable buffer (CreateProcess needs one).
  char cmdline[8192];
  snprintf(cmdline, sizeof(cmdline), "cmd.exe /c %s", command);

  STARTUPINFOA si = {0};
  si.cb = sizeof(si);
  si.dwFlags = STARTF_USESTDHANDLES;
  si.hStdOutput = out_wr;
  si.hStdError = err_wr;
  si.hStdInput = GetStdHandle(STD_INPUT_HANDLE);

  PROCESS_INFORMATION pi = {0};
  BOOL ok = CreateProcessA(NULL, cmdline, NULL, NULL, TRUE, 0, NULL, NULL, &si, &pi);

  // The write ends belong to the child now; close our copies so reads see EOF.
  CloseHandle(out_wr);
  CloseHandle(err_wr);

  if (!ok) {
    close_handle_pair(out_rd, err_rd);
    return abandon_process(proc, -6);
  }

  CloseHandle(pi.hThread);
  proc->process = pi.hProcess;
  proc->stdout_rd = out_rd;
  proc->stderr_rd = err_rd;
  processes[process_id] = proc;

  // Monitor thread fires user callbacks concurrently with the caller.
  osp_mem_notify_multithreaded();
  if (pthread_create(&proc->monitor_thread, NULL, process_monitor_thread, proc) != 0) {
    // Same contract as the POSIX -5 arm: end the unmonitorable child and WAIT
    // for it before releasing the handle. The WAIT is what proves nothing is
    // left, not the request: TerminateProcess is asynchronous, so its success
    // only says the kernel accepted it, and its one realistic failure is that
    // the process has already exited -- which is the same outcome. So both
    // branches wait. This cannot park the way a SIGTERM can: the handle came
    // from CreateProcess with full access and termination is not something the
    // target may decline.
    (void)TerminateProcess(proc->process, 1);
    WaitForSingleObject(proc->process, INFINITE);
    CloseHandle(proc->process);
    close_handle_pair(out_rd, err_rd);
    processes[process_id] = NULL;
    return abandon_process(proc, -5);
  }

  pthread_mutex_unlock(&process_mutex);
  return process_id;
}

int64_t await_process(int64_t process_id) {
  if (process_id < 1 || process_id >= MAX_PROCESSES) {
    return -1;
  }

  pthread_mutex_lock(&process_mutex);
  ProcessResult *proc = processes[process_id];
  pthread_mutex_unlock(&process_mutex);
  if (!proc) {
    return -1;
  }

  pthread_join(proc->monitor_thread, NULL);
  return proc->exit_code;
}

void cleanup_process(int64_t process_id) {
  if (process_id < 1 || process_id >= MAX_PROCESSES) {
    return;
  }

  pthread_mutex_lock(&process_mutex);
  ProcessResult *proc = processes[process_id];
  if (proc) {
    processes[process_id] = NULL;
    if (proc->process) {
      CloseHandle(proc->process);
    }
    if (proc->command) {
      free(proc->command);
    }
    pthread_mutex_destroy(&proc->mutex);
    free(proc);
  }
  pthread_mutex_unlock(&process_mutex);
}

// Legacy blocking spawn — _popen is the Windows equivalent of popen.
char *spawn_process(char *command) {
  if (!command) {
    return NULL;
  }

  FILE *pipe = _popen(command, "r");
  if (!pipe) {
    return NULL;
  }

  size_t buffer_size = 4096;
  char *output = malloc(buffer_size);
  if (!output) {
    _pclose(pipe);
    return NULL;
  }

  size_t total_read = 0;
  char buffer[256];
  while (fgets(buffer, sizeof(buffer), pipe) != NULL) {
    size_t len = strlen(buffer);
    if (total_read + len >= buffer_size) {
      buffer_size *= 2;
      char *grown = realloc(output, buffer_size);
      if (!grown) {
        free(output);
        _pclose(pipe);
        return NULL;
      }
      output = grown;
    }
    memcpy(output + total_read, buffer, len);
    total_read += len;
  }

  output[total_read] = '\0';
  _pclose(pipe);
  return output;
}

#endif // _WIN32
#endif // !__wasm__ — fork/exec/pthreads are absent on wasm32-wasip1

// read_file / write_file live in file_runtime.c: they are portable, they share
// the failure channel in io_error.h, and keeping them here left this unit's
// only wasm-relevant content buried under the process runtime.

