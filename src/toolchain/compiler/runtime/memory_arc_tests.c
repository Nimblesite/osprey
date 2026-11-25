// Vanilla-C src/osprey/tests for runtime/memory_arc.c — the Perceus ARC backend
// ([GC-ARC-PERCEUS], [MEM-ACYCLIC], [MEM-OWNERSHIP], [MEM-BACKENDS],
// [MEM-BACKENDS-CUSTOM], docs/specs/compiler/0018-MemoryManagement.md). Built by the Makefile's
// _test_c_runtime; never with osp_arc_shim.h (the foreign-pointer fixtures need
// real libc malloc/free). memory_arc.c exports no live-count symbol, so
// liveness is asserted structurally: the {int64 meta; int32 rc; uint32 size}
// header at body-16 is read back with memcpy, and reclamation is proved by
// *witness* leaves whose refcount can only move if their owner was really
// dropped and its layout word really walked — never by reading freed memory.
#include "arc_tests_shared.h"
#include "memory_hooks.h"
#include "test_gcov.h"

#include <assert.h>
#include <pthread.h>
#include <signal.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

// Each allocator can be told to refuse, because the paths that handle a refusal
// are shipped code. Those paths live in memory_arc.c's own translation unit, so
// the suite includes it after redirecting the allocators there.
static int g_refuse_malloc, g_refuse_calloc, g_refuse_realloc;
static void *refusable_malloc(size_t size) { return g_refuse_malloc ? NULL : malloc(size); }
static void *refusable_calloc(size_t n, size_t size) { return g_refuse_calloc ? NULL : calloc(n, size); }
static void *refusable_realloc(void *old, size_t size) { return g_refuse_realloc ? NULL : realloc(old, size); }
#define malloc(size) refusable_malloc(size)
#define calloc(n, size) refusable_calloc((n), (size))
#define realloc(old, size) refusable_realloc((old), (size))
// The ceiling ends a child without atexit, so its coverage is written first.
#define _Exit(status) (OSP_GCOV_DUMP(), _exit(status))
#include "memory_arc.c"
#undef _Exit
#undef malloc
#undef calloc
#undef realloc

_Atomic long g_checks = 0;

// [MEM-OWNERSHIP-STATIC-VERIFICATION] OSPREY_SCRIBBLE_FREES hands every
// released block back to libc, where AddressSanitizer can poison it, instead of
// parking it on the pool: the registry drops it at once. Off, a release parks
// the slot, so the registry keeps it.
static void t_scribble_bypasses_pool(void) {
  g_scribble_frees = 1;
  void *scribbled = osp_alloc_tagged(48, OSP_MEM_RAW);
  size_t held = g_present;
  CHECK(scribbled != NULL);
  osp_release(scribbled);
  CHECK(g_present == held - 1);
  g_scribble_frees = 0;
  void *parked = osp_alloc_tagged(48, OSP_MEM_RAW);
  held = g_present;
  CHECK(parked != NULL);
  osp_release(parked);
  CHECK(g_present == held);
}

static void t_alloc_sizes_oom(void) {
  void *zero = osp_alloc(0); // malloc(HDR+1): still a distinct live address
  void *neg = osp_alloc(-8); // size <= 0 clamps to 0
  void *min = osp_alloc(INT64_MIN), *one = osp_alloc(1);
  CHECK(zero && neg && min && one);
  CHECK(size_of(zero) == 0u && size_of(neg) == 0u && size_of(min) == 0u);
  CHECK(size_of(one) == 1u && rc_of(zero) == 1 && rc_of(min) == 1);
  CHECK(zero != neg && neg != min && min != one);
  osp_release(zero); osp_release(neg); osp_release(min); osp_release(one);
  CHECK(osp_alloc((int64_t)1 << 62) == NULL); // no such address space
  CHECK(osp_alloc_tagged(INT64_MAX, OSP_MEM_RAW) == NULL);
  CHECK(osp_arc_calloc((size_t)1 << 40, 4096) == NULL);
  // uint32_t header `size` is the hard ceiling: oversize is REJECTED, never
  // truncated (OSP_ARC_HDR + SIZE_MAX wraps to a malloc(15) that SUCCEEDS).
  CHECK(osp_alloc_tagged((int64_t)UINT32_MAX + 1, OSP_MEM_RAW) == NULL);
  // [FAILURE-EXPLICIT] Check the i64 request before narrowing to size_t.
  int64_t wide[] = {(int64_t)UINT32_MAX + 9, (int64_t)1 << 40, INT64_MAX - 7};
  for (unsigned i = 0; i < sizeof wide / sizeof wide[0]; i++) {
    CHECK(osp_alloc_tagged(wide[i], OSP_MEM_RAW) == NULL);
    CHECK(osp_alloc_tagged_noinit(wide[i], OSP_MEM_RAW) == NULL);
  }
#if SIZE_MAX == UINT32_MAX
  // Pool rounding plus the header must not wrap into a tiny allocation.
  for (int64_t tail = 0; tail < 31; tail++) {
    CHECK(osp_alloc_tagged((int64_t)UINT32_MAX - tail, OSP_MEM_RAW) == NULL);
    CHECK(osp_alloc_tagged_noinit((int64_t)UINT32_MAX - tail, OSP_MEM_RAW) == NULL);
  }
#endif
  CHECK(osp_arc_malloc((size_t)-1) == NULL);
  CHECK(osp_arc_calloc((size_t)-1, 2) == NULL); // n * size overflows first
  osp_release(mk(32, OSP_MEM_RAW)); // allocator state survives OOM
}
// --- probe-miss safety (foreign pointers) ------------------------------------
// --- LIST_HDR_EXTERN: the finalizer sees the intact body, then data drops -----

// --- allocator refusal ---------------------------------------------------------
#define ABORTED_STATUS 42
#define REFUSED_BYTES ((size_t)1u << 20) // larger than any pooled size class
static void flush_coverage_and_exit(int signal_number) {
  (void)signal_number;
  OSP_GCOV_DUMP();
  _exit(ABORTED_STATUS);
}
// What the last guarded child wrote to stderr.
static char g_seen[256];
// A fatal guard runs in a forked child, which must end with `expected`, naming what ran out.
static void dies_naming(void (*scenario)(void), const char *message, int expected) {
  int err[2];
  char *seen = g_seen;
  int status;
  CHECK(pipe(err) == 0);
  (void)fflush(NULL);
  pid_t child = fork();
  CHECK(child >= 0);
  if (child == 0) {
    (void)dup2(err[1], STDERR_FILENO);
    (void)signal(SIGABRT, flush_coverage_and_exit);
    scenario();
    _exit(0); // no abort: the status check below fails
  }
  close(err[1]);
  ssize_t n = read(err[0], seen, sizeof g_seen - 1);
  close(err[0]);
  CHECK(n > 0);
  seen[n] = '\0';
  CHECK(strstr(seen, message) != NULL);
  CHECK(waitpid(child, &status, 0) == child);
  CHECK(WIFEXITED(status) && WEXITSTATUS(status) == expected);
}
static void exhaust_worklist(void) {
  g_refuse_realloc = 1;
  g_wl_top = g_wl_cap; // the next push has to grow
  arc_wl_push((uintptr_t)1);
}
static void exhaust_registry(void) {
  g_refuse_calloc = 1;
  g_used = g_cap; // the next insert has to grow
  (void)osp_alloc_tagged((int64_t)REFUSED_BYTES, OSP_MEM_RAW);
}
static void t_allocator_refusal(void) {
  const size_t stride = 2 * sizeof(uintptr_t), limit = SIZE_MAX / stride;
  CHECK(arc_wl_growth_bytes(0) == 256 * sizeof(uintptr_t));
  CHECK(arc_wl_growth_bytes(limit) == limit * stride);
  CHECK(arc_wl_growth_bytes(limit + 1) == 0);
  CHECK(arc_wl_growth_bytes(limit + 2) == 0);
  CHECK(arc_wl_growth_bytes(SIZE_MAX) == 0);
  size_t live = osp_arc_live_objects();
  g_refuse_malloc = 1;
  CHECK(osp_alloc((int64_t)REFUSED_BYTES) == NULL);
  CHECK(osp_alloc_tagged((int64_t)REFUSED_BYTES, OSP_MEM_RAW) == NULL);
  CHECK(osp_alloc_tagged_noinit((int64_t)REFUSED_BYTES, OSP_MEM_RAW) == NULL);
  CHECK(osp_arc_malloc(REFUSED_BYTES) == NULL);
  g_refuse_malloc = 0;
  CHECK(osp_arc_live_objects() == live); // a refusal registers nothing
  void *after = mk(REFUSED_BYTES, OSP_MEM_RAW); // and the allocator still works
  CHECK(osp_arc_live_objects() == live + 1);
  osp_release(after);
  CHECK(osp_arc_live_objects() == live);
  dies_naming(exhaust_worklist, "ARC release worklist exhausted", ABORTED_STATUS);
  dies_naming(exhaust_registry, "ARC allocation registry exhausted", ABORTED_STATUS);
  CHECK(g_wl_top <= g_wl_cap && g_used < g_cap); // the children changed only themselves
}
// [GC-ARC-PERCEUS] A cluster whose home is the last bucket wraps through zero.
// Use registry keys directly to make the collision independent of malloc and ASLR.
static void t_wrapped_cluster(void) {
  uintptr_t keys[3];
  CHECK(arc_find(8) == NULL);
  CHECK(arc_grow());
  uintptr_t key = 8;
  for (size_t i = 0; i < 3; ++i) {
    while (arc_hash(key, g_cap) != g_cap - 1) key += 8;
    keys[i] = key;
    CHECK(arc_lookup_body(key) == NULL);
    arc_insert(key);
    CHECK(*arc_missed(key) != key);
    key += 8;
  }
  // [MEM-ARC-RECENT] A member a lookup confirmed answers from the recent cache, a non-member
  // twice from the missed cache, and removal clears the recent entry it leaves behind.
  uintptr_t stranger = key;
  CHECK(arc_lookup_body(keys[0]) == arc_hdr((void *)keys[0]) && *arc_recent(keys[0]) == keys[0]);
  CHECK(arc_lookup_body(keys[0]) == arc_hdr((void *)keys[0]));
  CHECK(arc_lookup_body(stranger) == NULL && *arc_missed(stranger) == stranger);
  CHECK(arc_lookup_body(stranger) == NULL);
  for (size_t i = 0; i < 3; ++i) {
    CHECK(arc_find(keys[i]) == &g_tab[g_cap - 1]);
    arc_remove(arc_find(keys[i]));
    CHECK(arc_find(keys[i]) == NULL && g_present == 2 - i);
  }
  CHECK(*arc_recent(keys[0]) != keys[0] && arc_lookup_body(keys[0]) == NULL);
}

// [MEM-HEAP-CEILING] A heap that keeps everything ends at its ceiling, naming the limit, a
// peak within one block of it and the request it refused. One block no heap under the
// ceiling could hold is refused instead, garbage never counts against a live program,
// and dead blocks parked for reuse are handed back before the ceiling stops anything.
#define CEILING_ROOM ((size_t)1 << 20)
#define CEILING_BLOCK ((size_t)64 << 10)
#define CEILING_PARKED 12000
static void fill_past_the_ceiling(void) {
  g_heap_peak = g_arc_resident; // the ceiling starts here, so the peak does too
  g_heap_limit = g_arc_resident + CEILING_ROOM;
  for (;;) {
    (void)osp_alloc((int64_t)CEILING_BLOCK);
  }
}
static void t_heap_ceiling(void) {
  static void *parked[CEILING_PARKED];
  size_t unlimited = g_heap_limit, limit = 0, peak = 0, request = 0;
  dies_naming(fill_past_the_ceiling, "osprey: heap limit exceeded: backend=arc", OSP_HEAP_EXHAUSTED_STATUS);
  CHECK(sscanf(g_seen, "osprey: heap limit exceeded: backend=arc limit_bytes=%zu peak_bytes=%zu request_bytes=%zu",
               &limit, &peak, &request) == 3);
  CHECK(limit == g_arc_resident + CEILING_ROOM && request == CEILING_BLOCK);
  CHECK(peak <= limit && peak > limit - CEILING_BLOCK - OSP_ARC_HDR - OSP_POOL_GRAIN);
  g_heap_limit = limit;
  g_heap_peak = g_arc_resident;
  CHECK(osp_alloc((int64_t)(2 * limit)) == NULL && osp_arc_malloc(2 * limit) == NULL);
  for (int i = 0; i < 128; i++) {
    osp_release(mk(CEILING_BLOCK, OSP_MEM_RAW)); // eight ceilings of garbage
  }
  for (size_t width = 48; width <= 80; width += 32) { // three quarters of the room, twice over
    for (int i = 0; i < CEILING_PARKED * 48 / (int)width; i++) {
      parked[i] = mk(width, OSP_MEM_RAW);
    }
    for (int i = 0; i < CEILING_PARKED * 48 / (int)width; i++) {
      osp_release(parked[i]);
    }
  }
  CHECK(g_arc_resident <= limit && g_heap_peak <= limit);
  g_heap_limit = unlimited;
}

int main(void) {
  // Precedes the first allocation: arc_arm_debug() latches once, forever.
  (void)setenv("OSPREY_ARC_DEBUG", "2", 1);
  t_wrapped_cluster();
  t_alloc_and_tags(); t_alloc_sizes_oom();
  t_retain_release(); t_release_unique_collect(); t_reclaim_reuse();
  t_scribble_bypasses_pool();
  t_foreign_stack_static(); t_foreign_heap_null();
  t_immortal(); t_immortal_child(); t_set_layout();
  t_mask_drop(); t_mask_bounds_and_bits();
  t_ptr_array(); t_ptr_array_ragged();
  t_list_hdr_kinds(); t_list_hdr_edges(); t_list_hdr_extern();
  t_deep_chain(); t_alloc_noinit(); t_mask_direct(); t_wide_fanout();
  t_registry_churn(); t_slot_reuse(); t_diagnostic_counters();
  t_allocator_refusal(); t_heap_ceiling();
  // Everything below runs LOCKED (notify is monotonic): the shim suite
  // re-exercises the whole ABI down the heap_lock mutex branch.
  t_multithreaded(); t_owned_text_lifetime(); t_shim_malloc_calloc_free();
  t_shim_realloc_grow(); t_shim_realloc_edges();
  t_shim_strdup(); t_leak_report_fixtures();
  printf("[ok] memory_arc: %ld assertions\n", g_checks);
  return 0;
}
