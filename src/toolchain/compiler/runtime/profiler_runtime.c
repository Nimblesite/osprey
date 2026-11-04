// Osprey sampling CPU profiler — thread registry, sessions, stack walk and
// activation. Implements [PROF-ACTIVATE-ENV], [PROF-COLLECT-REGISTRY] and
// [PROF-COLLECT-UNWIND] (docs/specs/tooling/0028-Profiler.md). The sample
// store lives in profiler_store.c, the JSON writers in profiler_dump.c and the
// live-attach endpoint in profiler_stream.c.
#include "profiler_runtime.h"

#if defined(_WIN32) || defined(__wasm__) || defined(OSP_SYMBOLIC)

// Profiling is POSIX-only for now; the hooks stay linkable everywhere. The
// symbolic build has one thread and no timers [SYMEX-RUNTIME-BITCODE].
void osp_prof_boot(void) {}
void osp_prof_thread_register(int64_t fiber_id, const char *label) {
  (void)fiber_id;
  (void)label;
}
void osp_prof_thread_unregister(void) {}

#else

#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

// ---- configuration & lifecycle state ---------------------------------------

enum {
  OSP_PROF_DEFAULT_HZ = 997, // non-round: avoids lockstep with periodic ticks
  OSP_PROF_MIN_HZ = 10,
  OSP_PROF_MAX_HZ = 10000,
  OSP_PROF_MAX_FRAME_BYTES = 1 << 20, // give up on absurd frame sizes
  OSP_PROF_MIN_CODE_ADDR = 4096,
};

static _Atomic bool g_active = false;
// Registration outlives sessions: once a session or the endpoint turned it on,
// every thread keeps registering AND unregistering, so a later session never
// samples a slot whose thread already exited [PROF-COLLECT-REGISTRY].
static _Atomic bool g_registry_on = false;
static uint32_t g_rate_hz = OSP_PROF_DEFAULT_HZ;
// Set only for an OSPREY_PROFILE session: the one session dumped to a file.
static char g_out_path[4096];
static uint64_t g_start_unix_ns = 0;

bool osp_prof_is_active(void) { return g_active; }
uint32_t osp_prof_rate_hz(void) { return g_rate_hz; }
uint64_t osp_prof_session_start_unix_ns(void) { return g_start_unix_ns; }
void osp_prof_registry_enable(void) { g_registry_on = true; }

uint64_t osp_prof_mono_ns(void) {
  struct timespec ts;
  if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
    return 0;
  }
  return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

uint64_t osp_prof_unix_ns(void) {
  struct timespec ts;
  if (clock_gettime(CLOCK_REALTIME, &ts) != 0) {
    return 0;
  }
  return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

// ---- thread registry [PROF-COLLECT-REGISTRY] --------------------------------

// A thread row is the immutable identity samples reference. Rows are numbered
// per session and never recycled inside one, so a fiber that exited keeps its
// label in that session's profile. Slots (the sampling targets) are recycled
// freely, and between sessions a slot has no row at all — which is what keeps
// a long-running program with the endpoint open from growing a row per fiber.
typedef struct OspProfRow {
  int64_t fiber_id;
  char label[OSP_PROF_LABEL_MAX + 1];
} OspProfRow;

static pthread_mutex_t g_registry_mutex = PTHREAD_MUTEX_INITIALIZER;
static OspProfSlot g_slots[OSP_PROF_MAX_THREADS];
static OspProfRow *g_rows = NULL;
static uint32_t g_row_len = 0;
static uint32_t g_row_cap = 0;
static __thread int tls_slot = -1;

uint32_t osp_prof_row_count(void) { return g_row_len; }
int64_t osp_prof_row_fiber(uint32_t row) { return g_rows[row].fiber_id; }
const char *osp_prof_row_label(uint32_t row) { return g_rows[row].label; }

static int find_free_slot(void) {
  for (int i = 0; i < OSP_PROF_MAX_THREADS; i++) {
    if (!g_slots[i].active) {
      return i;
    }
  }
  return -1;
}

static bool append_row(int64_t fiber_id, const char *label, uint32_t *out) {
  if (g_row_len == g_row_cap) {
    uint32_t cap = g_row_cap == 0 ? 64 : g_row_cap * 2;
    OspProfRow *rows = realloc(g_rows, (size_t)cap * sizeof(OspProfRow));
    if (!rows) {
      return false;
    }
    g_rows = rows;
    g_row_cap = cap;
  }
  OspProfRow *row = &g_rows[g_row_len];
  row->fiber_id = fiber_id;
  snprintf(row->label, sizeof(row->label), "%s", label ? label : "");
  *out = g_row_len++;
  return true;
}

static void capture_stack_bounds(OspProfSlot *slot) {
#if defined(__APPLE__)
  void *sa = pthread_get_stackaddr_np(pthread_self());
  uintptr_t hi = (uintptr_t)sa;
  slot->stack_hi = hi;
  slot->stack_lo = hi - (uintptr_t)pthread_get_stacksize_np(pthread_self());
#else
  pthread_attr_t attr;
  void *lo = NULL;
  size_t size = 0;
  if (pthread_getattr_np(pthread_self(), &attr) == 0) {
    if (pthread_attr_getstack(&attr, &lo, &size) != 0) {
      lo = NULL;
    }
    pthread_attr_destroy(&attr);
  }
  slot->stack_lo = (uintptr_t)lo;
  slot->stack_hi = (uintptr_t)lo + size;
#endif
}

// Populate a registry slot for the calling thread. The identity pair
// (fiber_id, label) is copied here as well as into the row table: the slot is
// what a signal handler and every osp_prof_self_slot consumer can reach, and
// it is what a later session numbers its rows from.
static void fill_slot(OspProfSlot *slot, uint32_t row, int64_t fiber_id,
                      const char *label) {
  slot->index = row;
  slot->fiber_id = fiber_id;
  snprintf(slot->label, sizeof(slot->label), "%s", label ? label : "");
  slot->pthread = pthread_self();
#if defined(__APPLE__)
  slot->mach_port = pthread_mach_thread_np(pthread_self());
#endif
  capture_stack_bounds(slot);
  slot->cpu_ns_prev = osp_prof_self_cpu_ns();
  slot->wall_ns_prev = osp_prof_mono_ns();
  slot->has_last_stack = false;
  slot->last_stack = 0;
  slot->signal_state = OSP_PROF_STATE_ONCPU;
  slot->active = true;
}

void osp_prof_thread_register(int64_t fiber_id, const char *label) {
  if (!g_registry_on || tls_slot >= 0) {
    return;
  }
  pthread_mutex_lock(&g_registry_mutex);
  // A session numbers the thread now; between sessions it waits for the next
  // session's numbering. A session row that cannot be stored registers nothing,
  // so no sample can ever reference a row the profile lacks.
  int idx = find_free_slot();
  uint32_t row = OSP_PROF_ROW_NONE;
  if (idx >= 0 && (!g_active || append_row(fiber_id, label, &row))) {
    fill_slot(&g_slots[idx], row, fiber_id, label);
    tls_slot = idx;
  }
  pthread_mutex_unlock(&g_registry_mutex);
}

void osp_prof_thread_unregister(void) {
  if (!g_registry_on || tls_slot < 0) {
    return;
  }
  pthread_mutex_lock(&g_registry_mutex);
  g_slots[tls_slot].active = false;
  g_slots[tls_slot].gen++;
  pthread_mutex_unlock(&g_registry_mutex);
  tls_slot = -1;
}

// The calling thread's registry slot, or NULL — async-signal-safe (a TLS read;
// the runtime archive links into the main executable, so TLS is initial-exec).
OspProfSlot *osp_prof_self_slot(void) {
  return tls_slot >= 0 ? &g_slots[tls_slot] : NULL;
}

// Only slots the running session numbered are sampling targets.
int osp_prof_snapshot(OspProfSnap *out, int max) {
  int n = 0;
  pthread_mutex_lock(&g_registry_mutex);
  for (int i = 0; i < OSP_PROF_MAX_THREADS && n < max; i++) {
    if (g_slots[i].active && g_slots[i].index != OSP_PROF_ROW_NONE) {
      out[n] = (OspProfSnap){.slot = &g_slots[i], .gen = g_slots[i].gen};
      n++;
    }
  }
  pthread_mutex_unlock(&g_registry_mutex);
  return n;
}

void osp_prof_registry_lock(void) { pthread_mutex_lock(&g_registry_mutex); }
void osp_prof_registry_unlock(void) { pthread_mutex_unlock(&g_registry_mutex); }

bool osp_prof_snap_live(const OspProfSnap *snap) {
  return snap->slot->active && snap->slot->gen == snap->gen;
}

// Number every registered thread for a new session and make the sampler forget
// what it remembered of the last one. Called under the registry lock.
static void begin_session_rows(void) {
  g_row_len = 0;
  for (int i = 0; i < OSP_PROF_MAX_THREADS; i++) {
    OspProfSlot *slot = &g_slots[i];
    uint32_t row = OSP_PROF_ROW_NONE;
    if (slot->active) {
      slot->index = append_row(slot->fiber_id, slot->label, &row)
                        ? row
                        : OSP_PROF_ROW_NONE;
      osp_prof_sampler_reset_slot(slot);
    }
  }
}

// ---- validated frame-pointer walk [PROF-COLLECT-UNWIND] ---------------------

static uint64_t strip_pac(uint64_t addr) {
#if defined(__aarch64__) && defined(__APPLE__)
  // Darwin userland VAs fit in 47 bits; the mask drops PAC/TBI bits. Do NOT
  // apply this on Linux arm64, where user VAs can legitimately use bit 47.
  return addr & 0x00007FFFFFFFFFFFULL;
#else
  return addr;
#endif
}

bool osp_prof_pc_is_code(uint64_t pc) {
  return strip_pac(pc) >= OSP_PROF_MIN_CODE_ADDR;
}

static int walk_chain(uint64_t fp, uintptr_t lo, uintptr_t hi, uint64_t *out,
                      int max) {
  if (hi < lo + 16) {
    return 0; // degenerate bounds: no room for a single frame record
  }
  int n = 0;
  while (n < max) {
    // Subtraction form: `fp + 16` could wrap for a garbage fp near UINT64_MAX.
    if (fp < lo || fp > hi - 16 || (fp & 7) != 0) {
      break;
    }
    uint64_t next = *(const uint64_t *)(uintptr_t)fp;
    uint64_t ret = strip_pac(*(const uint64_t *)(uintptr_t)(fp + 8));
    if (ret < OSP_PROF_MIN_CODE_ADDR) {
      break;
    }
    out[n++] = ret;
    next = strip_pac(next);
    if (next <= fp || next - fp > OSP_PROF_MAX_FRAME_BYTES) {
      break;
    }
    fp = next;
  }
  return n;
}

int osp_prof_walk(uint64_t pc, uint64_t fp, uint64_t lr, uintptr_t lo,
                  uintptr_t hi, uint64_t *out, int max) {
  if (max < 2) {
    return 0;
  }
  int n = 0;
  // [PROF-COLLECT-UNWIND] Frame 0 decides self-time, so it earns the floor every
  // other frame already gets: `lr` below and every chained return in walk_chain
  // are dropped under OSP_PROF_MIN_CODE_ADDR. The pc was the one frame trusted
  // blindly, so a leaf that is not a code address became the self-time bucket.
  const uint64_t leaf = strip_pac(pc);
  if (osp_prof_pc_is_code(pc)) {
    out[n++] = leaf;
  }
  uint64_t chain[OSP_PROF_MAX_FRAMES];
  int chain_max = max - 2 < OSP_PROF_MAX_FRAMES ? max - 2 : OSP_PROF_MAX_FRAMES;
  int c = walk_chain(strip_pac(fp), lo, hi, chain, chain_max);
  lr = strip_pac(lr);
  // The leaf's caller may exist only in lr (prologue not yet run); dedupe
  // against the first chained return address so it is never double-counted.
  // `n == 0` when the pc was floored out: there is no out[0] to dedupe against.
  if (lr >= OSP_PROF_MIN_CODE_ADDR && (n == 0 || lr != out[0]) &&
      (c == 0 || chain[0] != lr)) {
    out[n++] = lr;
  }
  for (int i = 0; i < c && n < max; i++) {
    out[n++] = chain[i];
  }
  return n;
}

// ---- sessions [PROF-ACTIVATE-ENV] [PROF-ATTACH-PROTOCOL] --------------------

bool osp_prof_session_start(uint32_t rate_hz) {
  if (g_active) {
    return false;
  }
  g_rate_hz = rate_hz;
  g_start_unix_ns = osp_prof_unix_ns();
  osp_prof_store_reset(osp_prof_mono_ns());
  pthread_mutex_lock(&g_registry_mutex);
  g_registry_on = true;
  begin_session_rows();
  g_active = true;
  pthread_mutex_unlock(&g_registry_mutex);
  if (!osp_prof_sampler_start()) {
    g_active = false;
    return false;
  }
  return true;
}

void osp_prof_session_stop(void) {
  if (!g_active) {
    return;
  }
  osp_prof_sampler_stop();
  g_active = false;
}

static uint32_t rate_from_env(void) {
  const char *hz = getenv("OSPREY_PROFILE_HZ");
  if (!hz || !*hz) {
    return OSP_PROF_DEFAULT_HZ;
  }
  long v = strtol(hz, NULL, 10);
  if (v < OSP_PROF_MIN_HZ) {
    return OSP_PROF_MIN_HZ;
  }
  if (v > OSP_PROF_MAX_HZ) {
    return OSP_PROF_MAX_HZ;
  }
  return (uint32_t)v;
}

static void dump_profile(void) {
  FILE *f = fopen(g_out_path, "w");
  if (!f) {
    fprintf(stderr, "osprey profiler: cannot write %s\n", g_out_path);
    return;
  }
  osp_prof_write_raw(f);
  fclose(f);
}

// Exposed for the C unit tests; production entry is boot_env_profiler.
bool osp_prof_start(const char *out_path, uint32_t rate_hz);
bool osp_prof_start(const char *out_path, uint32_t rate_hz) {
  if (g_active) {
    return false;
  }
  snprintf(g_out_path, sizeof(g_out_path), "%s", out_path);
  if (!osp_prof_session_start(rate_hz)) {
    g_out_path[0] = '\0';
    return false;
  }
  return true;
}

// Ends only the OSPREY_PROFILE session: a live-attach session belongs to the
// endpoint, which streams it instead of writing a file.
void osp_prof_stop_and_dump(void);
void osp_prof_stop_and_dump(void) {
  if (!g_active || g_out_path[0] == '\0') {
    return;
  }
  osp_prof_session_stop();
  dump_profile();
  g_out_path[0] = '\0';
}

static void boot_env_profiler(void) {
  const char *out = getenv("OSPREY_PROFILE");
  if (!out || !*out || g_active) {
    return;
  }
  if (osp_prof_start(out, rate_from_env())) {
    osp_prof_thread_register(0, "main");
    atexit(osp_prof_stop_and_dump);
  }
}

void osp_prof_boot(void) {
  boot_env_profiler();
  if (osp_diag_start()) {
    osp_prof_registry_enable();
    osp_prof_thread_register(0, "main");
  }
}

// Also start an OSPREY_PROFILE session from a constructor, so directly-linked
// (non-codegen) binaries — the C unit tests, hand-linked tools — profile too
// when the object is present. Only codegen's osp_prof_boot opens the endpoint:
// a binary that never asked for one must not grow a listener thread.
__attribute__((constructor)) static void osp_prof_ctor(void) {
  boot_env_profiler();
}

#endif // !_WIN32 && !__wasm__
