// The process table and the teardown of a record that never reached its monitor
// thread, shared by the POSIX and Win32 halves of system_runtime.c. Each half
// defines its own `ProcessResult` and includes this right after it, so both
// release exactly the same fields and the paths that share it cannot drift.
#ifndef OSPREY_SYSTEM_PROCESS_RECORDS_H
#define OSPREY_SYSTEM_PROCESS_RECORDS_H

static ProcessResult *processes[MAX_PROCESSES];
static int64_t next_process_id = 1;
static pthread_mutex_t process_mutex = PTHREAD_MUTEX_INITIALIZER;

// Caller holds process_mutex and no other thread can see `proc`, so this is its
// only owner. Returns `code` so every half-built teardown is one statement.
static int64_t abandon_process(ProcessResult *proc, int64_t code) {
  free(proc->command);
  pthread_mutex_destroy(&proc->mutex);
  free(proc);
  pthread_mutex_unlock(&process_mutex);
  return code;
}

#endif // OSPREY_SYSTEM_PROCESS_RECORDS_H
