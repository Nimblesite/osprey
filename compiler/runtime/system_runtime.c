#include <errno.h>
#include <pthread.h>
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
// [WINDOWS-PORT-PHASE3]
#ifdef _WIN32
#include "osprey_win_compat.h"
#else
#include <fcntl.h>
#include <signal.h>
#include <sys/select.h>
#include <sys/wait.h>
#include <unistd.h>
#endif

// Make stdout line-buffered at startup so a long-running program (e.g. an HTTP
// server that never returns to flush at exit) shows each printed line live,
// whether stdout is a TTY, a pipe, or a file. Line buffering changes only flush
// timing, never the bytes, so captured/differential output is unaffected.
__attribute__((constructor)) static void osprey_stdio_lbf(void) {
  setvbuf(stdout, NULL, _IOLBF, 0);
}

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

// Spawn process with event handler - similar to HTTP server pattern
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

  // Initialize process structure
  proc->process_id = process_id;
  proc->command = strdup(command); // strdup handles const char * correctly
  proc->exit_code = -999; // Not finished yet
  proc->is_running = true;
  proc->handler = handler;
  pthread_mutex_init(&proc->mutex, NULL);

  // Create pipes for stdout and stderr
  if (pipe(proc->stdout_pipe) != 0 || pipe(proc->stderr_pipe) != 0) {
    free(proc->command);
    free(proc);
    pthread_mutex_unlock(&process_mutex);
    return -4; // Pipe creation failed
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
      // Thread creation failed, clean up
      close(proc->stdout_pipe[0]);
      close(proc->stdout_pipe[1]);
      close(proc->stderr_pipe[0]);
      close(proc->stderr_pipe[1]);
      kill(proc->pid, SIGTERM);
      waitpid(proc->pid, NULL, 0);
      free(proc->command);
      free(proc);
      processes[process_id] = NULL;
      pthread_mutex_unlock(&process_mutex);
      return -5; // Thread creation failed
    }

    pthread_mutex_unlock(&process_mutex);
    return process_id;
  } else {
    // Fork failed
    close(proc->stdout_pipe[0]);
    close(proc->stdout_pipe[1]);
    close(proc->stderr_pipe[0]);
    close(proc->stderr_pipe[1]);
    free(proc->command);
    free(proc);
    pthread_mutex_unlock(&process_mutex);
    return -6; // Fork failed
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

#else // _WIN32 — [WINDOWS-PORT-PHASE3] Win32 process runtime

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
  proc->command = strdup(command);
  proc->exit_code = -999;
  proc->is_running = true;
  proc->handler = handler;
  pthread_mutex_init(&proc->mutex, NULL);

  // Inheritable pipes for the child's stdout/stderr.
  SECURITY_ATTRIBUTES sa = {sizeof(sa), NULL, TRUE};
  HANDLE out_rd = NULL, out_wr = NULL, err_rd = NULL, err_wr = NULL;
  if (!CreatePipe(&out_rd, &out_wr, &sa, 0) ||
      !CreatePipe(&err_rd, &err_wr, &sa, 0)) {
    free(proc->command);
    free(proc);
    pthread_mutex_unlock(&process_mutex);
    return -4;
  }
  // The read ends stay in this process — don't let the child inherit them.
  SetHandleInformation(out_rd, HANDLE_FLAG_INHERIT, 0);
  SetHandleInformation(err_rd, HANDLE_FLAG_INHERIT, 0);

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
    CloseHandle(out_rd);
    CloseHandle(err_rd);
    free(proc->command);
    free(proc);
    pthread_mutex_unlock(&process_mutex);
    return -6;
  }

  CloseHandle(pi.hThread);
  proc->process = pi.hProcess;
  proc->stdout_rd = out_rd;
  proc->stderr_rd = err_rd;
  processes[process_id] = proc;

  // Monitor thread fires user callbacks concurrently with the caller.
  osp_mem_notify_multithreaded();
  if (pthread_create(&proc->monitor_thread, NULL, process_monitor_thread, proc) != 0) {
    TerminateProcess(proc->process, 1);
    CloseHandle(proc->process);
    CloseHandle(out_rd);
    CloseHandle(err_rd);
    free(proc->command);
    free(proc);
    processes[process_id] = NULL;
    pthread_mutex_unlock(&process_mutex);
    return -5;
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

// Write file function - returns 0 for success, negative for error
int64_t write_file(char *filename, char *content) {
  if (!filename || !content) {
    return -1;
  }

  FILE *file = fopen(filename, "w");
  if (!file) {
    return -2;
  }

  size_t written = fwrite(content, 1, strlen(content), file);
  fclose(file);

  return (int64_t)written;
}

// Read file function - returns content or NULL on error
char *read_file(char *filename) {
  if (!filename) {
    return NULL;
  }

  FILE *file = fopen(filename, "r");
  if (!file) {
    return NULL;
  }

  // Get file size
  fseek(file, 0, SEEK_END);
  long size = ftell(file);
  fseek(file, 0, SEEK_SET);

  // Allocate buffer and read content
  char *content = malloc((size_t)size + 1);
  if (!content) {
    fclose(file);
    return NULL;
  }

  size_t read_size = fread(content, 1, (size_t)size, file);
  content[read_size] = '\0';
  fclose(file);

  return content;
}

// Simple JSON parsing - extract "code" field
char *parse_json(char *json_string) {
  if (!json_string) {
    return NULL;
  }

  // For now, just return the input
  // TODO: Implement proper JSON parsing
  return strdup(json_string);
}

// Extract arbitrary field from JSON {"field": "value"}
char *extract_json_field(char *json_string, const char *field_name) {
  if (!json_string || !field_name) {
    return NULL;
  }

  // Create the search pattern: "field_name":
  char *pattern = malloc(strlen(field_name) + 4); // "field_name":
  sprintf(pattern, "\"%s\":", field_name);

  char *field_start = strstr(json_string, pattern);
  free(pattern);

  if (!field_start) {
    return NULL;
  }

  // Skip past "field_name":
  field_start += strlen(field_name) + 3;

  // Skip whitespace and quotes
  while (*field_start == ' ' || *field_start == '\t' || *field_start == '"') {
    field_start++;
  }

  // Find the end quote
  char *field_end = strchr(field_start, '"');
  if (!field_end) {
    return NULL;
  }

  // Extract the field value
  size_t field_len = (size_t)(field_end - field_start);
  char *extracted_value = malloc(field_len + 1);
  strncpy(extracted_value, field_start, field_len);
  extracted_value[field_len] = '\0';

  return extracted_value;
}

// Extract code from JSON {"code": "..."} - backward compatibility
char *extract_code(char *json_string) {
  const char *code_field = "code";
  return extract_json_field(json_string, code_field);
}

// String comparison function for map key lookups
// Returns 0 if strings are equal, non-zero otherwise
int osprey_strcmp(const char* s1, const char* s2) {
    if (s1 == NULL || s2 == NULL) {
        return (s1 == s2) ? 0 : -1;
    }
    
    while (*s1 && (*s1 == *s2)) {
        s1++;
        s2++;
    }
    return *(const unsigned char*)s1 - *(const unsigned char*)s2;
}
