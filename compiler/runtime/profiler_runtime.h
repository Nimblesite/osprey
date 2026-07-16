// Osprey sampling CPU profiler — shared declarations. Implements
// [PROF-COLLECT-SAMPLER] / [PROF-COLLECT-REGISTRY] (docs/specs/0028-Profiler.md).
//
// The profiler is compiled into every runtime archive and stays inert unless
// OSPREY_PROFILE=<path> is set at process start [PROF-ACTIVATE-ENV]. Public
// hooks are safe to call unconditionally from the rest of the runtime.
#ifndef OSPREY_PROFILER_RUNTIME_H
#define OSPREY_PROFILER_RUNTIME_H

#include <stdbool.h>
#include <stdint.h>

// ---- public hooks (no-ops while inactive) ----------------------------------

// Activate the profiler when OSPREY_PROFILE is set [PROF-ACTIVATE-ENV].
// Codegen emits a call at the top of every program's main: static archives
// only extract referenced objects, so without this anchor a program that never
// touches fibers would silently link no profiler at all. Idempotent.
void osp_prof_boot(void);
// Register the calling thread for sampling, tagged with the Osprey fiber id it
// carries (0 = main thread, -1 = effect continuation) and a short label.
void osp_prof_thread_register(int64_t fiber_id, const char *label);
// Remove the calling thread from the sampling registry (call before exit).
void osp_prof_thread_unregister(void);

#if !defined(_WIN32) && !defined(__wasm__)

#include <pthread.h>

// ---- internal API shared between profiler translation units ----------------

enum {
  OSP_PROF_MAX_FRAMES = 128,
  OSP_PROF_MAX_THREADS = 1024,
  OSP_PROF_LABEL_MAX = 15,
};

// Sample states [PROF-RAW-FORMAT].
enum { OSP_PROF_STATE_ONCPU = 0, OSP_PROF_STATE_WAITING = 1 };

// One registered thread. Slots are fixed storage; `active` flips under the
// registry mutex and `gen` advances on unregister so a stale snapshot entry
// can be detected instead of sampling a recycled slot.
typedef struct OspProfSlot {
  // _Atomic: the Linux SIGPROF handler reads it lock-free to reject samples
  // racing an unregister/recycle; all writes happen under the registry mutex.
  _Atomic bool active;
  uint32_t gen;
  uint32_t index;
  pthread_t pthread;
#if defined(__APPLE__)
  uint32_t mach_port;
#endif
  int64_t fiber_id;
  char label[OSP_PROF_LABEL_MAX + 1];
  uintptr_t stack_lo;
  uintptr_t stack_hi;
  // Sampler-owned bookkeeping (only the sampler thread touches these, under
  // the registry lock).
  uint64_t cpu_ns_prev;
  uint64_t wall_ns_prev;
  uint32_t last_stack;
  bool has_last_stack;
  uint8_t signal_state; // Linux: state decided when the last SIGPROF was sent
  void *ring; // Linux: per-thread SPSC ring written by the SIGPROF handler.
} OspProfSlot;

// Sentinel returned by osp_prof_record_sample when the sample was dropped
// (never a valid interned stack index).
#define OSP_PROF_STACK_NONE UINT32_MAX

// A snapshot entry: the slot plus the generation it had when snapshotted, so
// the sampler can detect a slot that was unregistered (and possibly recycled)
// between the snapshot and the sample.
typedef struct OspProfSnap {
  OspProfSlot *slot;
  uint32_t gen;
} OspProfSnap;

bool osp_prof_is_active(void);
uint32_t osp_prof_rate_hz(void);
uint64_t osp_prof_mono_ns(void);

// Copy the currently active slots with their generations. The sampler must
// revalidate each entry under the registry lock before touching its thread
// (osp_prof_registry_lock + osp_prof_snap_live) — a fiber can exit and be
// pthread_join'ed between the snapshot and the sample, and using a joined
// pthread_t / deallocated mach port is undefined behavior.
int osp_prof_snapshot(OspProfSnap *out, int max);

// Registry lock, held by the sampler around each per-slot sampling action so
// unregister (which precedes the join) cannot complete mid-sample. Lock order
// is registry -> malloc everywhere; never allocate while a thread is
// suspended.
void osp_prof_registry_lock(void);
void osp_prof_registry_unlock(void);
// Whether a snapshotted slot is still the same live registration.
bool osp_prof_snap_live(const OspProfSnap *snap);

// The calling thread's registry slot, or NULL. Async-signal-safe.
OspProfSlot *osp_prof_self_slot(void);

// Intern `pcs` (leaf-first) and append one sample. Single consumer: only the
// sampler thread calls this. Returns the interned stack index, or
// OSP_PROF_STACK_NONE when the sample was dropped.
uint32_t osp_prof_record_sample(uint64_t t_ns, uint32_t thread_index,
                                const uint64_t *pcs, uint32_t n, uint8_t state);
// Append a sample reusing an already-interned stack (Linux waiting samples).
void osp_prof_record_repeat(uint64_t t_ns, uint32_t thread_index,
                            uint32_t stack_index, uint8_t state);
void osp_prof_note_drop(void);

// Validated frame-pointer chain walk [PROF-COLLECT-UNWIND]. Returns the frame
// count written to `out` (leaf-first: pc, deduplicated lr, then the chain).
int osp_prof_walk(uint64_t pc, uint64_t fp, uint64_t lr, uintptr_t lo,
                  uintptr_t hi, uint64_t *out, int max);

// Platform sampler backend (profiler_sampler.c).
bool osp_prof_sampler_start(void);
void osp_prof_sampler_stop(void);
// CPU time consumed by the calling thread, in nanoseconds (0 on failure).
uint64_t osp_prof_self_cpu_ns(void);

#endif // !_WIN32 && !__wasm__

#endif // OSPREY_PROFILER_RUNTIME_H
