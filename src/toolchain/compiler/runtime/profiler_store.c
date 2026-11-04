// Osprey sampling CPU profiler — stack interning and the sample store.
// Implements [PROF-RAW-FORMAT] (docs/specs/tooling/0028-Profiler.md).
//
// Single writer: only the sampler thread mutates the store while a session
// runs, and only a thread that joined the sampler (the exit dump, a live
// session's final flush) reads it afterwards, so no locking is needed.
#include "profiler_runtime.h"

#if !defined(_WIN32) && !defined(__wasm__)

#include <stdlib.h>
#include <string.h>

enum { OSP_PROF_MAX_SAMPLES = 4000000 };

typedef struct OspProfStack {
  uint32_t offset;
  uint32_t len;
  uint64_t hash;
} OspProfStack;

typedef struct OspProfStore {
  uint64_t *pc_pool;
  size_t pc_len, pc_cap;
  OspProfStack *stacks;
  size_t stack_len, stack_cap;
  uint32_t *stack_table; // open addressing; value = index + 1
  size_t table_cap;
  OspProfSample *samples;
  size_t sample_len, sample_cap;
  uint64_t dropped;
  uint64_t epoch_ns;
  uint32_t tick;
} OspProfStore;

static OspProfStore g_store;

void osp_prof_store_reset(uint64_t epoch_ns) {
  free(g_store.pc_pool);
  free(g_store.stacks);
  free(g_store.stack_table);
  free(g_store.samples);
  g_store = (OspProfStore){.epoch_ns = epoch_ns};
}

void osp_prof_store_begin_tick(void) { g_store.tick++; }
void osp_prof_note_drop(void) { g_store.dropped++; }
uint64_t osp_prof_dropped(void) { return g_store.dropped; }
size_t osp_prof_stack_count(void) { return g_store.stack_len; }
size_t osp_prof_sample_count(void) { return g_store.sample_len; }
void osp_prof_samples_clear(void) { g_store.sample_len = 0; }

const uint64_t *osp_prof_stack_pcs(size_t index, uint32_t *len) {
  const OspProfStack *s = &g_store.stacks[index];
  *len = s->len;
  return &g_store.pc_pool[s->offset];
}

OspProfSample osp_prof_sample_at(size_t index) {
  return g_store.samples[index];
}

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
  return s->len == n && memcmp(&g_store.pc_pool[s->offset], pcs,
                               (size_t)n * sizeof(uint64_t)) == 0;
}

static bool table_grow(void) {
  size_t cap = g_store.table_cap == 0 ? 4096 : g_store.table_cap * 2;
  uint32_t *table = calloc(cap, sizeof(uint32_t));
  if (!table) {
    return false;
  }
  for (size_t i = 0; i < g_store.stack_len; i++) {
    size_t j = (size_t)(g_store.stacks[i].hash & (cap - 1));
    while (table[j] != 0) {
      j = (j + 1) & (cap - 1);
    }
    table[j] = (uint32_t)i + 1;
  }
  free(g_store.stack_table);
  g_store.stack_table = table;
  g_store.table_cap = cap;
  return true;
}

static bool pool_reserve(uint32_t n) {
  if (g_store.pc_len + n <= g_store.pc_cap) {
    return true;
  }
  size_t cap = g_store.pc_cap == 0 ? 8192 : g_store.pc_cap * 2;
  while (cap < g_store.pc_len + n) {
    cap *= 2;
  }
  uint64_t *pool = realloc(g_store.pc_pool, cap * sizeof(uint64_t));
  if (!pool) {
    return false;
  }
  g_store.pc_pool = pool;
  g_store.pc_cap = cap;
  return true;
}

static bool stacks_reserve(void) {
  if (g_store.stack_len < g_store.stack_cap) {
    return true;
  }
  size_t cap = g_store.stack_cap == 0 ? 1024 : g_store.stack_cap * 2;
  OspProfStack *stacks = realloc(g_store.stacks, cap * sizeof(OspProfStack));
  if (!stacks) {
    return false;
  }
  g_store.stacks = stacks;
  g_store.stack_cap = cap;
  return true;
}

// The table slot holding `pcs`, or the empty slot where it belongs.
static size_t probe(const uint64_t *pcs, uint32_t n, uint64_t h) {
  size_t j = (size_t)(h & (g_store.table_cap - 1));
  while (g_store.stack_table[j] != 0) {
    const OspProfStack *s = &g_store.stacks[g_store.stack_table[j] - 1];
    if (s->hash == h && stack_equals(s, pcs, n)) {
      return j;
    }
    j = (j + 1) & (g_store.table_cap - 1);
  }
  return j;
}

static bool append_stack(const uint64_t *pcs, uint32_t n, uint64_t h,
                         size_t slot) {
  if (!pool_reserve(n) || !stacks_reserve()) {
    return false;
  }
  memcpy(&g_store.pc_pool[g_store.pc_len], pcs, (size_t)n * sizeof(uint64_t));
  g_store.stacks[g_store.stack_len] =
      (OspProfStack){.offset = (uint32_t)g_store.pc_len, .len = n, .hash = h};
  g_store.pc_len += n;
  g_store.stack_table[slot] = (uint32_t)++g_store.stack_len;
  return true;
}

static bool intern_stack(const uint64_t *pcs, uint32_t n, uint32_t *out) {
  if (g_store.table_cap == 0 ||
      g_store.stack_len * 4 >= g_store.table_cap * 3) {
    if (!table_grow()) {
      return false;
    }
  }
  uint64_t h = hash_pcs(pcs, n);
  size_t slot = probe(pcs, n, h);
  if (g_store.stack_table[slot] == 0 && !append_stack(pcs, n, h, slot)) {
    return false;
  }
  *out = g_store.stack_table[slot] - 1;
  return true;
}

static bool samples_reserve(void) {
  if (g_store.sample_len < g_store.sample_cap) {
    return true;
  }
  if (g_store.sample_cap >= OSP_PROF_MAX_SAMPLES) {
    return false;
  }
  size_t cap = g_store.sample_cap == 0 ? 4096 : g_store.sample_cap * 2;
  OspProfSample *samples =
      realloc(g_store.samples, cap * sizeof(OspProfSample));
  if (!samples) {
    return false;
  }
  g_store.samples = samples;
  g_store.sample_cap = cap;
  return true;
}

void osp_prof_record_repeat(uint64_t t_ns, uint32_t thread_index,
                            uint32_t stack_index, uint8_t state) {
  if (!samples_reserve()) {
    g_store.dropped++;
    return;
  }
  uint64_t rel = t_ns >= g_store.epoch_ns ? t_ns - g_store.epoch_ns : 0;
  g_store.samples[g_store.sample_len++] =
      (OspProfSample){.t_ns = rel, .thread = thread_index, .stack = stack_index,
                      .tick = g_store.tick, .state = state};
}

uint32_t osp_prof_record_sample(uint64_t t_ns, uint32_t thread_index,
                                const uint64_t *pcs, uint32_t n, uint8_t state) {
  uint32_t stack = 0;
  if (n == 0 || !intern_stack(pcs, n, &stack)) {
    g_store.dropped++;
    return OSP_PROF_STACK_NONE;
  }
  osp_prof_record_repeat(t_ns, thread_index, stack, state);
  return stack;
}

#else

typedef int osp_prof_store_unused; // profiling is POSIX-only

#endif // !_WIN32 && !__wasm__
