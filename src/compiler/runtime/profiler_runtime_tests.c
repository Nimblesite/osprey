// Assertion-driven tests for the sampling CPU profiler [PROF-TEST]
// (docs/specs/tooling/0028-Profiler.md). A failed assert aborts the binary.
//
// SPEC ID CROSS-REFERENCE — every clause of 0028-Profiler.md and what enforces
// it. [PROF-TEST] splits the work: "The C runtime suite verifies thread
// registration, sample capture, stack bounds, and raw JSON output", while the
// CLI pipeline, terminal report and symbolization are the end-to-end script's.
//
//   [PROF-ACTIVATE-ENV]     body_inactive_without_env, body_rate_default_997,
//                           body_rate_clamped_low/_high, body_rate_passthrough,
//                           body_stacks_are_leaf_first (written at exit);
//                           scripts/test_profiler.sh (OSPREY_PROFILE_HZ)
//   [PROF-COLLECT-SAMPLER]  test_sample_state_encoding_matches_spec,
//                           test_macos_capture_arm_produces_samples,
//                           scripts/test_profiler.sh (per-fiber state split)
//   [PROF-COLLECT-UNWIND]   test_walk_recovers_planted_chain, _dedupes_lr,
//                           _rejects_invalid_fp, _rejects_every_misalignment,
//                           _stack_bounds_are_half_open,
//                           _requires_strict_monotonic_growth,
//                           _enforces_bounded_frame_size, _enforces_depth_cap_128,
//                           _strips_pac_bits, _failed_check_ends_rather_than_skips,
//                           _rejects_pc_below_min_code_addr,
//                           _applies_same_floor_to_pc_as_to_lr,
//                           test_frame_zero_is_the_precise_pc;
//                           scripts/test_profiler.sh (frame 0 is the precise PC)
//   [PROF-COLLECT-REGISTRY] test_slot_snapshot_generation,
//                           body_registry_noop_when_inactive,
//                           body_registry_labels_round_trip, body_label_is_bounded
//   [PROF-RAW-FORMAT]       body_raw_format_conforms, body_sample_rows_conform,
//                           body_stacks_are_leaf_first, body_raw_header_fields
//   [PROF-ATTACH-ENDPOINT]  profiler_stream_tests.c, profiler_stream_lifecycle_tests.c
//   [PROF-ATTACH-PROTOCOL]  profiler_stream_tests.c, profiler_stream_lifecycle_tests.c
//   [PROF-SYMBOLIZE-OFFLINE] scripts/test_profiler.sh (no raw hex when a
//                           symbolizer is installed) — CURRENTLY RED
//   [PROF-BUILD-MODE]       osprey-cli driver flags (not covered here)
//   [PROF-CODEGEN-FP]       osprey-codegen "frame-pointer"="all" (not here)
//   [PROF-CLI-RUN]          scripts/test_profiler.sh (four exports + folded root)
//   [PROF-CLI-REPORT]       scripts/test_profiler.sh (header, columns, no calls)
//   [PROF-VSCODE-FLAME]     vscode-extension tests (not here)
//   [PROF-VSCODE-HEAT]      vscode-extension tests (not here)
#include "profiler_tests.h"

#include <pthread.h>

volatile double g_sink = 0;

// ---- registry & end-to-end -------------------------------------------------
// [PROF-COLLECT-REGISTRY] [PROF-COLLECT-SAMPLER] [PROF-RAW-FORMAT]

// Hooks must be safe no-ops while the profiler is inactive.
static void test_hooks_inactive_noop(void) {
  assert(!osp_prof_is_active());
  osp_prof_thread_register(1, "fiber");
  osp_prof_thread_unregister();
  OspProfSnap snaps[OSP_PROF_MAX_THREADS];
  assert(osp_prof_snapshot(snaps, OSP_PROF_MAX_THREADS) == 0);
}

// A dropped sample must return the sentinel, never stack index 0.
static void test_record_drop_returns_sentinel(void) {
  assert(osp_prof_record_sample(0, 0, NULL, 0, 0) == OSP_PROF_STACK_NONE);
}

// [PROF-RAW-FORMAT] interning survives the stack table's regrowth: more
// distinct stacks than the first table holds at 3/4 load must keep their
// indices, so every sample recorded before a rehash still names its stack.
static void test_interning_survives_table_growth(void) {
  enum { STACKS = 5000 };
  osp_prof_store_reset(0);
  for (uint64_t i = 0; i < STACKS; i++) {
    const uint64_t pcs[2] = {0x100000000ULL + i * 16, 0x100000F000ULL};
    assert(osp_prof_record_sample(i, 0, pcs, 2, OSP_PROF_STATE_ONCPU) == i);
  }
  assert(osp_prof_stack_count() == STACKS);
  for (uint64_t i = 0; i < STACKS; i += 499) {
    const uint64_t pcs[2] = {0x100000000ULL + i * 16, 0x100000F000ULL};
    assert(osp_prof_record_sample(i, 0, pcs, 2, OSP_PROF_STATE_ONCPU) == i);
    uint32_t len = 0;
    assert(osp_prof_stack_pcs(i, &len)[0] == pcs[0] && len == 2);
  }
  osp_prof_samples_clear();
  assert(osp_prof_sample_count() == 0 && osp_prof_stack_count() == STACKS);
  osp_prof_store_reset(0);
  assert(osp_prof_stack_count() == 0 && osp_prof_dropped() == 0);
}

__attribute__((noinline)) double busy_work(long n) {
  double acc = 0;
  for (long i = 1; i <= n; i++) {
    acc += (double)i / (double)(i + 1);
  }
  return acc;
}

static void *busy_thread(void *arg) {
  (void)arg;
  osp_prof_thread_register(42, "fiber");
  g_sink += busy_work(60000000);
  osp_prof_thread_unregister();
  return NULL;
}

// The boot anchor stays inert without OSPREY_PROFILE, both clocks behave
// (monotonic never regresses; busy work consumes measurable CPU time), and the
// registry primitives are callable from an unregistered thread.
static void test_boot_and_clocks(void) {
  osp_prof_boot();
  osp_prof_boot(); // idempotent
  assert(!osp_prof_is_active());
  uint64_t t0 = osp_prof_mono_ns();
  uint64_t t1 = osp_prof_mono_ns();
  assert(t0 > 0 && t1 >= t0);
  uint64_t cpu0 = osp_prof_self_cpu_ns();
  g_sink += busy_work(2000000);
  uint64_t cpu1 = osp_prof_self_cpu_ns();
  assert(cpu1 > 0 && cpu1 >= cpu0);
  assert(osp_prof_self_slot() == NULL); // never registered on this path
  osp_prof_registry_lock();
  osp_prof_registry_unlock();
  osp_prof_note_drop(); // inactive: safe, just a counter
}

// [PROF-COLLECT-REGISTRY] Threads registered before a session must retain
// their identity when that session assigns its sample rows.
static void test_threads_registered_between_sessions_retain_identity(void) {
  enum { FIBER_ID = 73 };
  osp_prof_registry_enable();
  osp_prof_thread_register(FIBER_ID, "waiting");
  OspProfSlot *self = osp_prof_self_slot();
  assert(self != NULL && self->index == OSP_PROF_ROW_NONE);
  OspProfSnap snap;
  assert(osp_prof_snapshot(&snap, 1) == 0);
  assert(osp_prof_session_start(TEST_RATE_HZ));
  assert(osp_prof_snapshot(&snap, 1) == 1 && snap.slot == self);
  assert(self->index < osp_prof_row_count());
  assert(osp_prof_row_fiber(self->index) == FIBER_ID);
  assert(strcmp(osp_prof_row_label(self->index), "waiting") == 0);
  osp_prof_session_stop();
  osp_prof_thread_unregister();
  assert(!osp_prof_is_active() && osp_prof_self_slot() == NULL);
}

// Registered-slot introspection and generation validation: the self slot
// carries the exact fiber id/label and sane stack bounds; a snapshot entry is
// live until unregister and DEAD after; record_repeat appends against a stack
// interned by record_sample (called before any slot is registered, so this
// test remains the sampler's single record producer).
static void test_slot_snapshot_generation(void) {
  const char *out = "/tmp/osprey_profiler_slots_test.json";
  unlink(out);
  assert(osp_prof_start(out, 200));
  uint64_t pcs[2] = {0x100000F000, 0x100000E000};
  uint32_t stack = osp_prof_record_sample(osp_prof_mono_ns(), 0, pcs, 2,
                                          OSP_PROF_STATE_ONCPU);
  assert(stack != OSP_PROF_STACK_NONE);
  osp_prof_record_repeat(osp_prof_mono_ns(), 0, stack, OSP_PROF_STATE_WAITING);
  osp_prof_note_drop();
  osp_prof_thread_register(7, "fiber");
  OspProfSlot *self = osp_prof_self_slot();
  assert(self != NULL);
  assert(self->fiber_id == 7);
  assert(strcmp(self->label, "fiber") == 0);
  assert(self->stack_lo < self->stack_hi);
  OspProfSnap snaps[OSP_PROF_MAX_THREADS];
  int n = osp_prof_snapshot(snaps, OSP_PROF_MAX_THREADS);
  assert(n >= 1);
  OspProfSnap mine = {0};
  for (int i = 0; i < n; i++) {
    if (snaps[i].slot == self) {
      mine = snaps[i];
    }
  }
  assert(mine.slot == self);
  osp_prof_registry_lock();
  assert(osp_prof_snap_live(&mine));
  osp_prof_registry_unlock();
  osp_prof_thread_unregister();
  osp_prof_registry_lock();
  assert(!osp_prof_snap_live(&mine)); // generation advanced: stale snapshot
  osp_prof_registry_unlock();
  osp_prof_stop_and_dump();
  assert(!osp_prof_is_active());
  unlink(out);
}

char *slurp(const char *path) {
  FILE *f = fopen(path, "r");
  assert(f != NULL);
  assert(fseek(f, 0, SEEK_END) == 0);
  long size = ftell(f);
  assert(size > 0);
  rewind(f);
  char *buf = malloc((size_t)size + 1);
  assert(buf != NULL);
  assert(fread(buf, 1, (size_t)size, f) == (size_t)size);
  buf[size] = '\0';
  fclose(f);
  return buf;
}

static void test_end_to_end_capture(void) {
  const char *out = "/tmp/osprey_profiler_test.json";
  unlink(out);
  assert(osp_prof_start(out, TEST_RATE_HZ));
  assert(osp_prof_is_active());
  assert(osp_prof_rate_hz() == TEST_RATE_HZ);
  osp_prof_thread_register(0, "main");
  pthread_t t;
  assert(pthread_create(&t, NULL, busy_thread, NULL) == 0);
  g_sink += busy_work(30000000);
  assert(pthread_join(t, NULL) == 0);
  osp_prof_thread_unregister();
  osp_prof_stop_and_dump();
  assert(!osp_prof_is_active());

  char *json = slurp(out);
  assert(strstr(json, "\"version\":1") != NULL);
  assert(strstr(json, "\"rate_hz\":2000") != NULL);
  assert(strstr(json, "\"label\":\"main\"") != NULL);
  assert(strstr(json, "\"label\":\"fiber\"") != NULL);
  assert(strstr(json, "\"images\":[{") != NULL);
  // Real samples were captured: a non-empty samples array with a stack row.
  assert(strstr(json, "\"samples\":[[") != NULL);
  assert(strstr(json, "\"stacks\":[[") != NULL);
  free(json);
  unlink(out);

  // EVERY dump must be well-formed, not just a process's first. The image list
  // is emitted by a `dl_iterate_phdr` callback, and holding its "already wrote
  // one" flag in a function-local static made each later capture open the array
  // with a separator — `"images":[,{...}` — which no JSON reader accepts, so a
  // second profile lost the image list its addresses symbolize against. Assert
  // a second capture in the SAME process is shaped like the first, so the
  // defect cannot come back disguised as suite ordering [PROF-RAW-FORMAT].
  assert(osp_prof_start(out, TEST_RATE_HZ));
  osp_prof_thread_register(0, "main");
  g_sink += busy_work(200000);
  osp_prof_thread_unregister();
  osp_prof_stop_and_dump();
  json = slurp(out);
  assert(strstr(json, "\"images\":[{") != NULL);
  assert(strstr(json, "\"images\":[,") == NULL);
  free(json);
  unlink(out);
}

static void *churn_thread(void *arg) {
  (void)arg;
  osp_prof_thread_register(99, "fiber");
  g_sink += busy_work(20000);
  osp_prof_thread_unregister();
  return NULL;
}

// Registration churn under max-rate sampling: hammers the snapshot-vs-
// unregister window (a slot can be unregistered, joined, and recycled between
// a sampler snapshot and the sample). The generation-validated locking in
// sample_thread must make this safe [PROF-COLLECT-REGISTRY].
static void test_churn_under_max_rate_sampling(void) {
  enum { BATCH = 8, ROUNDS = 40 };
  const char *out = "/tmp/osprey_profiler_churn_test.json";
  unlink(out);
  assert(osp_prof_start(out, 10000));
  for (int round = 0; round < ROUNDS; round++) {
    pthread_t threads[BATCH];
    for (int i = 0; i < BATCH; i++) {
      assert(pthread_create(&threads[i], NULL, churn_thread, NULL) == 0);
    }
    for (int i = 0; i < BATCH; i++) {
      assert(pthread_join(threads[i], NULL) == 0);
    }
  }
  osp_prof_stop_and_dump();
  char *json = slurp(out);
  assert(strstr(json, "\"label\":\"fiber\"") != NULL);
  free(json);
  unlink(out);
}

static void test_registry_and_capture(void) {
  test_threads_registered_between_sessions_retain_identity();
  test_slot_snapshot_generation();
  test_end_to_end_capture();
  test_churn_under_max_rate_sampling();
}

int main(void) {
  // This suite tests the OSPREY_PROFILE session and the registry; the endpoint
  // has its own suite. A forked body _exits without running atexit, so an
  // endpoint opened here would also leave its socket behind.
  setenv("OSPREY_DIAGNOSTICS", "0", 1);
  run_walk_tests();
  run_spec_tests();
  test_hooks_inactive_noop();
  test_record_drop_returns_sentinel();
  test_interning_survives_table_growth();
  test_boot_and_clocks();
  test_registry_and_capture();
  printf("profiler_runtime_tests: all tests passed (sink=%f)\n", g_sink);
  return 0;
}
