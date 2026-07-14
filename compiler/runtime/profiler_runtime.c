// Osprey sampling CPU profiler — registry, sample store, stack-walk, raw dump.
// Implements [PROF-ACTIVATE-ENV], [PROF-COLLECT-REGISTRY],
// [PROF-COLLECT-UNWIND], [PROF-RAW-FORMAT] (docs/specs/0028-Profiler.md).
#include "profiler_runtime.h"

#if defined(_WIN32) || defined(__wasm__)

// Profiling is POSIX-only for now; the hooks stay linkable everywhere.
void osp_prof_boot(void) {}
void osp_prof_thread_register(int64_t fiber_id, const char *label) {
  (void)fiber_id;
  (void)label;
}
void osp_prof_thread_unregister(void) {}

#else

#include <inttypes.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#if defined(__APPLE__)
#include <mach-o/dyld.h>
#include <mach/mach.h>
#else
#include <link.h>
#endif

// ---- configuration & lifecycle state ---------------------------------------

enum {
  OSP_PROF_DEFAULT_HZ = 997, // non-round: avoids lockstep with periodic ticks
  OSP_PROF_MIN_HZ = 10,
  OSP_PROF_MAX_HZ = 10000,
  OSP_PROF_MAX_SAMPLES = 4000000,
  OSP_PROF_MAX_FRAME_BYTES = 1 << 20, // give up on absurd frame sizes
  OSP_PROF_MIN_CODE_ADDR = 4096,
};

static _Atomic bool g_active = false;
static uint32_t g_rate_hz = OSP_PROF_DEFAULT_HZ;
static char g_out_path[4096];
static uint64_t g_start_unix_ns = 0;
static uint64_t g_start_mono_ns = 0;

bool osp_prof_is_active(void) { return g_active; }
uint32_t osp_prof_rate_hz(void) { return g_rate_hz; }

uint64_t osp_prof_mono_ns(void) {
  struct timespec ts;
  if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
    return 0;
  }
  return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

static uint64_t unix_ns_now(void) {
  struct timespec ts;
  if (clock_gettime(CLOCK_REALTIME, &ts) != 0) {
    return 0;
  }
  return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

// ---- thread registry [PROF-COLLECT-REGISTRY] --------------------------------

// A thread row is the immutable identity samples reference; rows are never
// recycled, so a fiber that exited keeps its label in the dump. Slots (the
// sampling targets) are recycled freely.
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
  uintptr_t hi = (uintptr_t)pthread_get_stackaddr_np(pthread_self());
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

static void fill_slot(OspProfSlot *slot, uint32_t row) {
  slot->index = row;
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
  if (!g_active || tls_slot >= 0) {
    return;
  }
  pthread_mutex_lock(&g_registry_mutex);
  // Re-check under the lock: a registration racing the exit dump must not
  // append rows while dump_threads iterates them.
  int idx = g_active ? find_free_slot() : -1;
  uint32_t row = 0;
  if (idx >= 0 && append_row(fiber_id, label, &row)) {
    fill_slot(&g_slots[idx], row);
    tls_slot = idx;
  }
  pthread_mutex_unlock(&g_registry_mutex);
}

void osp_prof_thread_unregister(void) {
  if (!g_active || tls_slot < 0) {
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

int osp_prof_snapshot(OspProfSnap *out, int max) {
  int n = 0;
  pthread_mutex_lock(&g_registry_mutex);
  for (int i = 0; i < OSP_PROF_MAX_THREADS && n < max; i++) {
    if (g_slots[i].active) {
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

// ---- stack interning & sample store -----------------------------------------
// Single consumer: only the sampler thread mutates these (the exit dump runs
// after the sampler is joined), so no locking is needed.

typedef struct OspProfStack {
  uint32_t offset;
  uint32_t len;
  uint64_t hash;
} OspProfStack;

typedef struct OspProfSample {
  uint64_t t_ns;
  uint32_t thread;
  uint32_t stack;
  uint8_t state;
} OspProfSample;

static uint64_t *g_pc_pool = NULL;
static size_t g_pc_len = 0, g_pc_cap = 0;
static OspProfStack *g_stacks = NULL;
static size_t g_stack_len = 0, g_stack_cap = 0;
static uint32_t *g_stack_table = NULL; // open addressing; value = index + 1
static size_t g_table_cap = 0;
static OspProfSample *g_samples = NULL;
static size_t g_sample_len = 0, g_sample_cap = 0;
static uint64_t g_dropped = 0;

void osp_prof_note_drop(void) { g_dropped++; }

static uint64_t hash_pcs(const uint64_t *pcs, uint32_t n) {
  uint64_t h = 1469598103934665603ULL; // FNV-1a
  for (uint32_t i = 0; i < n; i++) {
    for (int b = 0; b < 8; b++) {
      h = (h ^ ((pcs[i] >> (8 * b)) & 0xFF)) * 1099511628211ULL;
    }
  }
  return h;
}

static bool stack_equals(const OspProfStack *s, const uint64_t *pcs, uint32_t n) {
  return s->len == n &&
         memcmp(&g_pc_pool[s->offset], pcs, (size_t)n * sizeof(uint64_t)) == 0;
}

static bool table_grow(void) {
  size_t cap = g_table_cap == 0 ? 4096 : g_table_cap * 2;
  uint32_t *table = calloc(cap, sizeof(uint32_t));
  if (!table) {
    return false;
  }
  for (size_t i = 0; i < g_stack_len; i++) {
    size_t j = (size_t)(g_stacks[i].hash & (cap - 1));
    while (table[j] != 0) {
      j = (j + 1) & (cap - 1);
    }
    table[j] = (uint32_t)i + 1;
  }
  free(g_stack_table);
  g_stack_table = table;
  g_table_cap = cap;
  return true;
}

static bool pool_reserve(uint32_t n) {
  if (g_pc_len + n <= g_pc_cap) {
    return true;
  }
  size_t cap = g_pc_cap == 0 ? 8192 : g_pc_cap * 2;
  while (cap < g_pc_len + n) {
    cap *= 2;
  }
  uint64_t *pool = realloc(g_pc_pool, cap * sizeof(uint64_t));
  if (!pool) {
    return false;
  }
  g_pc_pool = pool;
  g_pc_cap = cap;
  return true;
}

static bool stacks_reserve(void) {
  if (g_stack_len < g_stack_cap) {
    return true;
  }
  size_t cap = g_stack_cap == 0 ? 1024 : g_stack_cap * 2;
  OspProfStack *stacks = realloc(g_stacks, cap * sizeof(OspProfStack));
  if (!stacks) {
    return false;
  }
  g_stacks = stacks;
  g_stack_cap = cap;
  return true;
}

static bool intern_stack(const uint64_t *pcs, uint32_t n, uint32_t *out) {
  if (g_table_cap == 0 || g_stack_len * 4 >= g_table_cap * 3) {
    if (!table_grow()) {
      return false;
    }
  }
  uint64_t h = hash_pcs(pcs, n);
  size_t j = (size_t)(h & (g_table_cap - 1));
  while (g_stack_table[j] != 0) {
    OspProfStack *s = &g_stacks[g_stack_table[j] - 1];
    if (s->hash == h && stack_equals(s, pcs, n)) {
      *out = g_stack_table[j] - 1;
      return true;
    }
    j = (j + 1) & (g_table_cap - 1);
  }
  if (!pool_reserve(n) || !stacks_reserve()) {
    return false;
  }
  memcpy(&g_pc_pool[g_pc_len], pcs, (size_t)n * sizeof(uint64_t));
  g_stacks[g_stack_len] =
      (OspProfStack){.offset = (uint32_t)g_pc_len, .len = n, .hash = h};
  g_pc_len += n;
  g_stack_table[j] = (uint32_t)++g_stack_len;
  *out = (uint32_t)(g_stack_len - 1);
  return true;
}

static bool samples_reserve(void) {
  if (g_sample_len < g_sample_cap) {
    return true;
  }
  if (g_sample_cap >= OSP_PROF_MAX_SAMPLES) {
    return false;
  }
  size_t cap = g_sample_cap == 0 ? 4096 : g_sample_cap * 2;
  OspProfSample *samples = realloc(g_samples, cap * sizeof(OspProfSample));
  if (!samples) {
    return false;
  }
  g_samples = samples;
  g_sample_cap = cap;
  return true;
}

void osp_prof_record_repeat(uint64_t t_ns, uint32_t thread_index,
                            uint32_t stack_index, uint8_t state) {
  if (!samples_reserve()) {
    g_dropped++;
    return;
  }
  uint64_t rel = t_ns >= g_start_mono_ns ? t_ns - g_start_mono_ns : 0;
  g_samples[g_sample_len++] = (OspProfSample){
      .t_ns = rel, .thread = thread_index, .stack = stack_index, .state = state};
}

uint32_t osp_prof_record_sample(uint64_t t_ns, uint32_t thread_index,
                                const uint64_t *pcs, uint32_t n, uint8_t state) {
  uint32_t stack = 0;
  if (n == 0 || !intern_stack(pcs, n, &stack)) {
    g_dropped++;
    return OSP_PROF_STACK_NONE;
  }
  osp_prof_record_repeat(t_ns, thread_index, stack, state);
  return stack;
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
  out[n++] = strip_pac(pc);
  uint64_t chain[OSP_PROF_MAX_FRAMES];
  int chain_max = max - 2 < OSP_PROF_MAX_FRAMES ? max - 2 : OSP_PROF_MAX_FRAMES;
  int c = walk_chain(strip_pac(fp), lo, hi, chain, chain_max);
  lr = strip_pac(lr);
  // The leaf's caller may exist only in lr (prologue not yet run); dedupe
  // against the first chained return address so it is never double-counted.
  if (lr >= OSP_PROF_MIN_CODE_ADDR && lr != out[0] && (c == 0 || chain[0] != lr)) {
    out[n++] = lr;
  }
  for (int i = 0; i < c && n < max; i++) {
    out[n++] = chain[i];
  }
  return n;
}

// ---- raw profile dump [PROF-RAW-FORMAT] --------------------------------------

static void json_escape_into(FILE *f, const char *s) {
  for (; *s; s++) {
    if (*s == '"' || *s == '\\') {
      fputc('\\', f);
    }
    fputc(*s, f);
  }
}

#if defined(__APPLE__)
static void dump_images(FILE *f) {
  uint32_t count = _dyld_image_count();
  for (uint32_t i = 0; i < count; i++) {
    fprintf(f, "%s{\"path\":\"", i == 0 ? "" : ",");
    json_escape_into(f, _dyld_get_image_name(i));
    fprintf(f, "\",\"base\":%" PRIu64 ",\"slide\":%" PRIu64 "}",
            (uint64_t)(uintptr_t)_dyld_get_image_header(i),
            (uint64_t)_dyld_get_image_vmaddr_slide(i));
  }
}
static void write_exe_path(FILE *f) {
  char buf[4096];
  uint32_t size = sizeof(buf);
  if (_NSGetExecutablePath(buf, &size) == 0) {
    json_escape_into(f, buf);
  }
}
#else
static int dump_image_cb(struct dl_phdr_info *info, size_t size, void *data) {
  (void)size;
  FILE *f = data;
  static int first = 1;
  fprintf(f, "%s{\"path\":\"", first ? "" : ",");
  first = 0;
  json_escape_into(f, info->dlpi_name ? info->dlpi_name : "");
  fprintf(f, "\",\"base\":%" PRIu64 ",\"slide\":%" PRIu64 "}",
          (uint64_t)info->dlpi_addr, (uint64_t)info->dlpi_addr);
  return 0;
}
static void dump_images(FILE *f) { dl_iterate_phdr(dump_image_cb, f); }
static void write_exe_path(FILE *f) {
  char buf[4096];
  ssize_t n = readlink("/proc/self/exe", buf, sizeof(buf) - 1);
  if (n > 0) {
    buf[n] = '\0';
    json_escape_into(f, buf);
  }
}
#endif

static void dump_threads(FILE *f) {
  // Serialize against a late registration's append_row realloc.
  pthread_mutex_lock(&g_registry_mutex);
  for (uint32_t i = 0; i < g_row_len; i++) {
    fprintf(f, "%s{\"fiber\":%lld,\"label\":\"", i == 0 ? "" : ",",
            (long long)g_rows[i].fiber_id);
    json_escape_into(f, g_rows[i].label);
    fprintf(f, "\"}");
  }
  pthread_mutex_unlock(&g_registry_mutex);
}

static void dump_stacks(FILE *f) {
  for (size_t i = 0; i < g_stack_len; i++) {
    fprintf(f, "%s[", i == 0 ? "" : ",");
    for (uint32_t j = 0; j < g_stacks[i].len; j++) {
      fprintf(f, "%s%" PRIu64, j == 0 ? "" : ",",
              g_pc_pool[g_stacks[i].offset + j]);
    }
    fprintf(f, "]");
  }
}

static void dump_samples(FILE *f) {
  for (size_t i = 0; i < g_sample_len; i++) {
    const OspProfSample *s = &g_samples[i];
    fprintf(f, "%s[%" PRIu64 ",%u,%u,%u]", i == 0 ? "" : ",", s->t_ns,
            s->thread, s->stack, s->state);
  }
}

static const char *platform_name(void) {
#if defined(__APPLE__) && defined(__aarch64__)
  return "macos-arm64";
#elif defined(__APPLE__)
  return "macos-x86_64";
#elif defined(__aarch64__)
  return "linux-arm64";
#else
  return "linux-x86_64";
#endif
}

static void dump_profile(void) {
  FILE *f = fopen(g_out_path, "w");
  if (!f) {
    fprintf(stderr, "osprey profiler: cannot write %s\n", g_out_path);
    return;
  }
  fprintf(f,
          "{\"version\":1,\"pid\":%lld,\"rate_hz\":%u,\"platform\":\"%s\","
          "\"start_unix_ns\":%" PRIu64 ",\"end_unix_ns\":%" PRIu64
          ",\"dropped\":%" PRIu64 ",\"exe\":\"",
          (long long)getpid(), g_rate_hz, platform_name(), g_start_unix_ns,
          unix_ns_now(), g_dropped);
  write_exe_path(f);
  fprintf(f, "\",\"images\":[");
  dump_images(f);
  fprintf(f, "],\"threads\":[");
  dump_threads(f);
  fprintf(f, "],\"stacks\":[");
  dump_stacks(f);
  fprintf(f, "],\"samples\":[");
  dump_samples(f);
  fprintf(f, "]}\n");
  fclose(f);
}

// ---- lifecycle [PROF-ACTIVATE-ENV] -------------------------------------------

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

// Exposed for the C unit tests; production entry is the constructor below.
bool osp_prof_start(const char *out_path, uint32_t rate_hz);
bool osp_prof_start(const char *out_path, uint32_t rate_hz) {
  if (g_active) {
    return false;
  }
  snprintf(g_out_path, sizeof(g_out_path), "%s", out_path);
  g_rate_hz = rate_hz;
  g_start_unix_ns = unix_ns_now();
  g_start_mono_ns = osp_prof_mono_ns();
  g_active = true;
  if (!osp_prof_sampler_start()) {
    g_active = false;
    return false;
  }
  return true;
}

void osp_prof_stop_and_dump(void);
void osp_prof_stop_and_dump(void) {
  if (!g_active) {
    return;
  }
  osp_prof_sampler_stop();
  g_active = false;
  dump_profile();
}

void osp_prof_boot(void) {
  const char *out = getenv("OSPREY_PROFILE");
  if (!out || !*out || g_active) {
    return;
  }
  if (osp_prof_start(out, rate_from_env())) {
    osp_prof_thread_register(0, "main");
    atexit(osp_prof_stop_and_dump);
  }
}

// Also boot from a constructor so directly-linked (non-codegen) binaries — the
// C unit tests, hand-linked tools — profile too when the object is present.
__attribute__((constructor)) static void osp_prof_ctor(void) { osp_prof_boot(); }

#endif // !_WIN32 && !__wasm__
