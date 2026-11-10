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
#include "memory_arc.c"
#undef malloc
#undef calloc
#undef realloc

_Atomic long g_checks = 0;

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
// A fatal guard runs in a forked child, which must abort naming what ran out.
static void aborts_naming(void (*scenario)(void), const char *message) {
  int err[2];
  char seen[128];
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
  ssize_t n = read(err[0], seen, sizeof seen - 1);
  close(err[0]);
  CHECK(n > 0);
  seen[n] = '\0';
  CHECK(strstr(seen, message) != NULL);
  CHECK(waitpid(child, &status, 0) == child);
  CHECK(WIFEXITED(status) && WEXITSTATUS(status) == ABORTED_STATUS);
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
  aborts_naming(exhaust_worklist, "ARC release worklist exhausted");
  aborts_naming(exhaust_registry, "ARC allocation registry exhausted");
  CHECK(g_wl_top <= g_wl_cap && g_used < g_cap); // the children changed only themselves
}
int main(void) {
  // Precedes the first allocation: arc_arm_debug() latches once, forever.
  (void)setenv("OSPREY_ARC_DEBUG", "2", 1);
  t_alloc_and_tags(); t_alloc_sizes_oom();
  t_retain_release(); t_release_unique_collect(); t_reclaim_reuse();
  t_foreign_stack_static(); t_foreign_heap_null();
  t_immortal(); t_immortal_child(); t_set_layout();
  t_mask_drop(); t_mask_bounds_and_bits();
  t_ptr_array(); t_ptr_array_ragged();
  t_list_hdr_kinds(); t_list_hdr_edges(); t_list_hdr_extern();
  t_deep_chain(); t_alloc_noinit(); t_mask_direct(); t_wide_fanout();
  t_registry_churn(); t_slot_reuse(); t_diagnostic_counters();
  t_allocator_refusal();
  // Everything below runs LOCKED (notify is monotonic): the shim suite
  // re-exercises the whole ABI down the arc_lock mutex branch.
  t_multithreaded(); t_owned_text_lifetime(); t_shim_malloc_calloc_free();
  t_shim_realloc_grow(); t_shim_realloc_edges();
  t_shim_strdup(); t_leak_report_fixtures();
  printf("[ok] memory_arc: %ld assertions\n", g_checks);
  return 0;
}
