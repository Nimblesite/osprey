// The I/O failure-reason channel shared by the C runtime and codegen.
//
// Every runtime entry point that reports failure as a magic number (a negative
// int64, a NULL char*) loses WHY it failed the moment it returns: ENOENT,
// EACCES and ENOSPC all arrive at the Osprey program as the same -2. This
// channel carries the reason alongside that status so `Error { message }` holds
// a truthful sentence instead of the placeholder word "Error".
//
// Discipline, and it is not optional:
//
//   * A producer calls osp_io_error_clear() on entry and osp_io_error_set() on
//     EVERY failure path. A failure path that returns without setting is a
//     silent failure — the exact defect this channel exists to delete.
//   * The message is thread-local, like errno: a fiber, a coroutine body and
//     the HTTP server thread each keep their own, so one thread's failure can
//     never be read as another's.
//   * Codegen clears immediately before the call and reads immediately after,
//     so a stale reason from an earlier op can never be attributed to a later
//     one. crates/osprey-codegen/src/extern_call.rs emits that pair.
//
// Implements [BUILTIN-FILE-ERRMSG].
#ifndef OSPREY_IO_ERROR_H
#define OSPREY_IO_ERROR_H

// Discard any reason held for the calling thread. After this, osp_io_error()
// reads NULL until the next osp_io_error_set().
void osp_io_error_clear(void);

// Record why `op` failed on `subject` (a path, handle or URL — may be NULL),
// with `err` an errno value, or 0 when the cause is not an errno. Formats
// "op: subject: reason" into the calling thread's buffer, truncating rather
// than allocating: a reporter that can itself fail out of memory is no reporter.
void osp_io_error_set(const char *op, const char *subject, int err);

// The calling thread's current reason, or NULL if none is held. BORROWED: the
// pointer belongs to the channel and dies at this thread's next set/clear, so
// anything that outlives the call must use osp_io_error_take instead.
const char *osp_io_error(void);

// The calling thread's current reason as a fresh heap copy the caller owns, or
// NULL if none is held. This is what codegen stores into an `Error { message }`
// — a Result can outlive any number of later I/O calls, and a borrowed pointer
// into the channel would read as whatever failed most recently, or as freed
// memory. Under the ARC backend this unit is built with osp_arc_shim.h, so the
// copy carries a Perceus header and the Result block's drop releases it.
char *osp_io_error_take(void);

#endif // OSPREY_IO_ERROR_H
