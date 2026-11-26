// The process table and the teardown of a record that never reached its monitor
// thread, shared by the POSIX and Win32 halves of system_runtime.c. Each half
// defines its own `ProcessResult` and includes this right after it, so both
// release exactly the same fields and the paths that share it cannot drift.
#ifndef OSPREY_SYSTEM_PROCESS_RECORDS_H
#define OSPREY_SYSTEM_PROCESS_RECORDS_H

static ProcessResult *processes[MAX_PROCESSES];
static int64_t next_process_id = 1;
static pthread_mutex_t process_mutex = PTHREAD_MUTEX_INITIALIZER;

// An Osprey function value passed as the callback [BUILTIN-PROCESS-CLOSURE].
// Its cell starts with the code pointer, which takes the cell itself first, as
// http_listen_closure's handlers do. Calling the cell as a plain C function
// jumped into data. Managed closures must be direct data roots (malloc'd
// records are outside the tracing heap), so the cell lives here, indexed by
// handle, and ARC holds one reference until the record is released.
typedef void (*ProcessClosureHandler)(void *closure, int64_t process_id,
                                      int64_t event_type, char *data);
static void *process_closures[MAX_PROCESSES];

// Exactly one of the two is set: a raw C function or an Osprey closure cell.
typedef struct {
  ProcessEventHandler handler;
  void *closure;
} ProcessCallback;

static bool process_callback_present(ProcessCallback callback) {
  return callback.handler || callback.closure;
}

// Caller holds process_mutex: the handle is not visible to any thread yet.
static void adopt_process_callback(ProcessResult *proc, ProcessCallback callback) {
  proc->handler = callback.handler;
  if (callback.closure) {
    osp_retain(callback.closure);
  }
  process_closures[proc->process_id] = callback.closure;
}

// Caller holds process_mutex and the monitor thread has finished or never ran.
static void forget_process_callback(int64_t process_id) {
  void *closure = process_closures[process_id];
  process_closures[process_id] = NULL;
  if (closure) {
    osp_release(closure);
  }
}

// Every event, on either platform, reaches Osprey through this one call.
static void process_event(const ProcessResult *proc, int64_t event, char *data) {
  void *closure = process_closures[proc->process_id];
  if (closure) {
    ProcessClosureHandler code;
    memcpy(&code, closure, sizeof(code));
    code(closure, proc->process_id, event, data);
  } else if (proc->handler) {
    proc->handler(proc->process_id, event, data);
  }
}

// Caller holds process_mutex and no other thread can see `proc`, so this is its
// only owner. Returns `code` so every half-built teardown is one statement.
static int64_t abandon_process(ProcessResult *proc, int64_t code) {
  forget_process_callback(proc->process_id);
  free(proc->command);
  pthread_mutex_destroy(&proc->mutex);
  free(proc);
  pthread_mutex_unlock(&process_mutex);
  return code;
}

#endif // OSPREY_SYSTEM_PROCESS_RECORDS_H
