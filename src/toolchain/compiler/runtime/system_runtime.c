#include <errno.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "effects_runtime.h"
#include "memory_hooks.h"
#include "osprey_thread.h"

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

#include "system_process_records.h"
#include "system_process_posix.h"

// Clean up process resources
void cleanup_process(int64_t process_id) {
  if (process_id < 1 || process_id >= MAX_PROCESSES) {
    return;
  }

  pthread_mutex_lock(&process_mutex);
  ProcessResult *proc = processes[process_id];
  if (proc) {
    processes[process_id] = NULL;
    forget_process_callback(process_id);

    if (proc->command)
      free(proc->command);
    pthread_mutex_destroy(&proc->mutex);
    free(proc);
  }
  pthread_mutex_unlock(&process_mutex);
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

#include "system_process_records.h"

// Close both ends of a handle pair created for a process that will not run.
static void close_handle_pair(HANDLE first, HANDLE second) {
  CloseHandle(first);
  CloseHandle(second);
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
    process_event(proc, event_type, buffer);
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

      char exit_code_str[32];
      snprintf(exit_code_str, sizeof(exit_code_str), "%lld",
               (long long)proc->exit_code);
      process_event(proc, PROCESS_EXIT, exit_code_str);
      break;
    }
  }

  CloseHandle(proc->stdout_rd);
  CloseHandle(proc->stderr_rd);
  // The callbacks ran Osprey code here; release this thread's handler stack.
  __osprey_handler_stack_cleanup();
  return NULL;
}

// cmd.exe's own line limit (8191 characters) plus the terminator.
#define WIN_COMMAND_LINE_MAX 8192

// The Win32 twin. [BUILTIN-PROCESS-FAILURE] is platform-neutral, so this must
// unwind exactly as the POSIX path does: pipes checked separately so a failed
// second one does not strand the first, the record's mutex destroyed on every
// teardown, and the unmonitorable child both ended and WAITED FOR before its
// handle is released.
static int64_t spawn_process(const char *command, ProcessCallback callback) {
  // A command longer than the command line is refused before anything is
  // created: truncating it would run a different, cut-off command.
  char cmdline[WIN_COMMAND_LINE_MAX];
  int written = command && process_callback_present(callback) ? snprintf(cmdline, sizeof(cmdline), "cmd.exe /c %s", command) : -1;
  if (written < 0 || (size_t)written >= sizeof(cmdline)) {
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
  if (pthread_mutex_init(&proc->mutex, NULL) != 0) {
    free(proc);
    pthread_mutex_unlock(&process_mutex);
    return -3;
  }
  adopt_process_callback(proc, callback);
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
  if (osp_thread_start(&proc->monitor_thread, process_monitor_thread, proc, false) != 0) {
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

void cleanup_process(int64_t process_id) {
  if (process_id < 1 || process_id >= MAX_PROCESSES) {
    return;
  }

  pthread_mutex_lock(&process_mutex);
  ProcessResult *proc = processes[process_id];
  if (proc) {
    processes[process_id] = NULL;
    forget_process_callback(process_id);
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

#endif // _WIN32

// A raw C function: a named Osprey function lowered to its code pointer.
int64_t spawn_process_with_handler(const char *command, ProcessEventHandler handler) {
  return spawn_process(command, (ProcessCallback){handler, NULL});
}

// An Osprey function value, such as a capturing lambda [BUILTIN-PROCESS-CLOSURE].
int64_t spawn_process_with_closure(const char *command, void *closure) {
  return spawn_process(command, (ProcessCallback){NULL, closure});
}

// Both platform records share this lifecycle [BUILTIN-PROCESS-FAILURE].
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
#endif // !__wasm__ — fork/exec/pthreads are absent on wasm32-wasip1

// read_file / write_file live in file_runtime.c: they are portable, they share
// the failure channel in io_error.h, and keeping them here left this unit's
// only wasm-relevant content buried under the process runtime.

