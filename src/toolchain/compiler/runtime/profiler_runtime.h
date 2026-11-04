// Osprey sampling CPU profiler — shared declarations. Implements
// [PROF-COLLECT-SAMPLER] / [PROF-COLLECT-REGISTRY] (docs/specs/tooling/0028-Profiler.md).
//
// The profiler is compiled into every runtime archive. It samples only while a
// session runs: one started by OSPREY_PROFILE=<path> at process start
// [PROF-ACTIVATE-ENV], or one a client starts through the live-attach endpoint
// [PROF-ATTACH-ENDPOINT]. Public hooks are safe to call unconditionally from
// the rest of the runtime.
#ifndef OSPREY_PROFILER_RUNTIME_H
#define OSPREY_PROFILER_RUNTIME_H

#include <stdbool.h>
#include <stdint.h>

// ---- public hooks (no-ops while inactive) ----------------------------------

// Start the OSPREY_PROFILE session [PROF-ACTIVATE-ENV] and open the live-attach
// endpoint [PROF-ATTACH-ENDPOINT]. Codegen emits a call at the top of every
// program's main: static archives only extract referenced objects, so without
// this anchor a program that never touches fibers would silently link no
// profiler at all. Idempotent.
void osp_prof_boot(void);
// Register the calling thread for sampling, tagged with the Osprey fiber id it
// carries (0 = main thread, -1 = effect continuation) and a short label.
void osp_prof_thread_register(int64_t fiber_id, const char *label);
// Remove the calling thread from the sampling registry (call before exit).
void osp_prof_thread_unregister(void);

#if !defined(_WIN32) && !defined(__wasm__) && !defined(OSP_SYMBOLIC)

#include <pthread.h>
#include <stddef.h>
#include <stdio.h>

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
// A registered slot's `index` while no session runs: rows exist only inside a
// session, so a thread that registered between sessions has none yet.
#define OSP_PROF_ROW_NONE UINT32_MAX

// One retained sample [PROF-RAW-FORMAT]: `t_ns` is relative to session start,
// and `tick` numbers the sampler tick that recorded it [PROF-ATTACH-PROTOCOL].
typedef struct OspProfSample {
  uint64_t t_ns;
  uint32_t thread;
  uint32_t stack;
  uint32_t tick;
  uint8_t state;
} OspProfSample;

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

// True when `pc` can be a code address at all [PROF-COLLECT-UNWIND]. The first
// page is never mapped executable on any supported platform, so a pc below it
// is not an instruction. osp_prof_walk floors such a pc out of frame 0, which
// means a caller that recorded the result anyway would publish a stack whose
// frame 0 is a RETURN address -- and the offline symbolizer decides the -1
// return-address adjustment POSITIONALLY ([PROF-SYMBOLIZE-OFFLINE]), so the
// sample would attribute to the line after the call. Sample sources must
// therefore drop such a capture rather than reshape it; see profiler_sampler.c.
bool osp_prof_pc_is_code(uint64_t pc);

// Validated frame-pointer chain walk [PROF-COLLECT-UNWIND]. Returns the frame
// count written to `out`: the pc first when osp_prof_pc_is_code(pc), then the
// deduplicated lr, then the chain. Every frame is floored at the same code
// address bound, the leaf included, so the count can be 0 when nothing survives.
int osp_prof_walk(uint64_t pc, uint64_t fp, uint64_t lr, uintptr_t lo,
                  uintptr_t hi, uint64_t *out, int max);

// Platform sampler backend (profiler_sampler.c).
bool osp_prof_sampler_start(void);
void osp_prof_sampler_stop(void);
// Forget what the sampler kept from a slot's previous session: its last stack
// indexes a store that was reset, and a Linux ring may still hold entries
// stamped with that session's row. Under the registry lock, sampler stopped.
void osp_prof_sampler_reset_slot(OspProfSlot *slot);
// CPU time consumed by the calling thread, in nanoseconds (0 on failure).
uint64_t osp_prof_self_cpu_ns(void);

// ---- sessions & thread rows (profiler_runtime.c) ---------------------------
// Session start and stop are called from one thread at a time: the boot path,
// or the endpoint under its session mutex.

// Start sampling every registered thread with empty tables, or return false
// when a session already runs (an OSPREY_PROFILE session included).
bool osp_prof_session_start(uint32_t rate_hz);
// Stop sampling and join the sampler; the caller then owns the store.
void osp_prof_session_stop(void);
// Let threads register from now on, with or without a session.
void osp_prof_registry_enable(void);
uint64_t osp_prof_session_start_unix_ns(void);
uint64_t osp_prof_unix_ns(void);
// Thread rows [PROF-RAW-FORMAT]: the identities a session's samples reference,
// never recycled within it. Read them under the registry lock.
uint32_t osp_prof_row_count(void);
int64_t osp_prof_row_fiber(uint32_t row);
const char *osp_prof_row_label(uint32_t row);

// ---- sample store (profiler_store.c) ---------------------------------------
// Single writer: the sampler thread, or a thread that has joined it.

// Empty every table; sample times are then measured from `epoch_ns`.
void osp_prof_store_reset(uint64_t epoch_ns);
// The sampler starts a tick: samples recorded from now on carry its number.
void osp_prof_store_begin_tick(void);
size_t osp_prof_stack_count(void);
// The leaf-first pcs of interned stack `index`, `*len` of them.
const uint64_t *osp_prof_stack_pcs(size_t index, uint32_t *len);
size_t osp_prof_sample_count(void);
OspProfSample osp_prof_sample_at(size_t index);
// Forget the retained samples but keep the interned stacks, so a stream that
// already sent the stacks keeps indexing them.
void osp_prof_samples_clear(void);
uint64_t osp_prof_dropped(void);

// ---- JSON writers (profiler_dump.c) [PROF-RAW-FORMAT] ----------------------

// A quoted JSON string; control characters are escaped, so a value can never
// break the one-object-per-line framing of the endpoint [PROF-ATTACH-PROTOCOL].
void osp_prof_json_string(FILE *f, const char *s);
void osp_prof_write_exe(FILE *f);
void osp_prof_write_images(FILE *f);
// How many images osp_prof_write_images would write now.
uint32_t osp_prof_image_count(void);
// Rows [from, count) under the registry lock; returns count.
uint32_t osp_prof_write_threads(FILE *f, uint32_t from);
// Stacks [from, count); returns count.
size_t osp_prof_write_stacks(FILE *f, size_t from);
// Every retained sample as a [PROF-RAW-FORMAT] row, with its tick as a fifth
// column when `with_tick` (the stream's rows) [PROF-ATTACH-PROTOCOL].
void osp_prof_write_samples(FILE *f, bool with_tick);
const char *osp_prof_platform(void);
// The whole raw profile of the session that just stopped.
void osp_prof_write_raw(FILE *f);

// ---- live-attach endpoint (profiler_stream.c) [PROF-ATTACH-ENDPOINT] -------

// Open the endpoint unless OSPREY_DIAGNOSTICS=0 or the platform has none.
bool osp_diag_start(void);
// Close the endpoint, ending any session and removing the socket.
void osp_diag_stop(void);
// Sampler hook, called after every tick: streams the session's new data.
void osp_diag_pump(void);

#endif // !_WIN32 && !__wasm__

#endif // OSPREY_PROFILER_RUNTIME_H
