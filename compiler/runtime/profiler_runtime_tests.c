// Assertion-driven tests for the sampling CPU profiler [PROF-TEST]
// (docs/specs/0028-Profiler.md). A failed assert aborts the binary.
#include "profiler_runtime.h"

#include <assert.h>
#include <fcntl.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "test_death.h"

bool osp_prof_start(const char *out_path, uint32_t rate_hz);
void osp_prof_stop_and_dump(void);

enum { TEST_RATE_HZ = 2000, FAKE_STACK_WORDS = 64 };

static volatile double g_sink = 0;

// ---- osp_prof_walk unit tests [PROF-COLLECT-UNWIND] --------------------------

// Build a fake AAPCS64-style frame chain inside `mem` and verify the walk
// recovers exactly the planted return addresses, in leaf-first order.
static void test_walk_recovers_planted_chain(void) {
  uint64_t mem[FAKE_STACK_WORDS];
  memset(mem, 0, sizeof(mem));
  uintptr_t lo = (uintptr_t)mem;
  uintptr_t hi = (uintptr_t)(mem + FAKE_STACK_WORDS);
  // Frame records at word 0 -> word 8 -> word 16: [next_fp, return_addr].
  mem[0] = (uint64_t)(uintptr_t)&mem[8];
  mem[1] = 0x100000AAAA;
  mem[8] = (uint64_t)(uintptr_t)&mem[16];
  mem[9] = 0x100000BBBB;
  mem[16] = 0; // chain terminator (fails the bounds check)
  mem[17] = 0x100000CCCC;
  uint64_t out[OSP_PROF_MAX_FRAMES];
  int n = osp_prof_walk(0x100000F000, (uint64_t)lo, 0x100000E000, lo, hi, out,
                        OSP_PROF_MAX_FRAMES);
  assert(n == 5);
  assert(out[0] == 0x100000F000); // precise pc first
  assert(out[1] == 0x100000E000); // lr (differs from first chained ret)
  assert(out[2] == 0x100000AAAA);
  assert(out[3] == 0x100000BBBB);
  assert(out[4] == 0x100000CCCC);
}

// The lr must be deduplicated when it equals the first chained return address.
static void test_walk_dedupes_lr(void) {
  uint64_t mem[FAKE_STACK_WORDS];
  memset(mem, 0, sizeof(mem));
  uintptr_t lo = (uintptr_t)mem;
  uintptr_t hi = (uintptr_t)(mem + FAKE_STACK_WORDS);
  mem[0] = 0;
  mem[1] = 0x100000AAAA;
  uint64_t out[OSP_PROF_MAX_FRAMES];
  int n = osp_prof_walk(0x100000F000, (uint64_t)lo, 0x100000AAAA, lo, hi, out,
                        OSP_PROF_MAX_FRAMES);
  assert(n == 2);
  assert(out[0] == 0x100000F000 && out[1] == 0x100000AAAA);
}

// Out-of-bounds, misaligned, or non-monotonic frame pointers end the walk
// instead of being dereferenced.
static void test_walk_rejects_invalid_fp(void) {
  uint64_t mem[FAKE_STACK_WORDS];
  memset(mem, 0, sizeof(mem));
  uintptr_t lo = (uintptr_t)mem;
  uintptr_t hi = (uintptr_t)(mem + FAKE_STACK_WORDS);
  uint64_t out[OSP_PROF_MAX_FRAMES];
  assert(osp_prof_walk(0x100000F000, (uint64_t)(hi + 64), 0, lo, hi, out,
                       OSP_PROF_MAX_FRAMES) == 1); // fp above hi: pc only
  assert(osp_prof_walk(0x100000F000, (uint64_t)lo + 3, 0, lo, hi, out,
                       OSP_PROF_MAX_FRAMES) == 1); // misaligned fp: pc only
  mem[0] = (uint64_t)lo; // self-referencing fp must not loop forever
  mem[1] = 0x100000AAAA;
  assert(osp_prof_walk(0x100000F000, (uint64_t)lo, 0, lo, hi, out,
                       OSP_PROF_MAX_FRAMES) == 2);
}

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

__attribute__((noinline)) static double busy_work(long n) {
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

static char *slurp(const char *path) {
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


// ---- SPEC VIOLATIONS: macOS stack capture -----------------------------------
// Every assertion below quotes the clause of docs/specs/0028-Profiler.md it
// enforces. They are RED and must stay red until the capture honours the spec.
// See the quarantine note on read_regs() in profiler_sampler.c for the evidence.

// [PROF-COLLECT-UNWIND] "Frame 0 is the precise PC." — validation.
//
// The leaf pc is emitted unconditionally by osp_prof_walk (profiler_runtime.c
// line ~413: `out[n++] = strip_pac(pc);`). `lr` is floored at
// OSP_PROF_MIN_CODE_ADDR on line ~420, and so is every chained return address
// in walk_chain on line ~394 — the pc, the ONE frame that decides self-time, is
// the only one trusted blindly. These two are RED for that reason alone, which
// is distinct from the frame-0/chain disagreement below: a fix for either
// leaves the other standing, so the block cannot go green by halves.
static void test_walk_rejects_pc_below_min_code_addr(void) {
  uint64_t mem[FAKE_STACK_WORDS];
  memset(mem, 0, sizeof(mem));
  uintptr_t lo = (uintptr_t)mem;
  uintptr_t hi = (uintptr_t)(mem + FAKE_STACK_WORDS);
  mem[0] = 0; // terminate the chain immediately
  mem[1] = 0x100000AAAA;
  uint64_t out[OSP_PROF_MAX_FRAMES];
  // A null pc is not an instruction. It must not become the leaf frame.
  int n = osp_prof_walk(0, (uint64_t)lo, 0x100000BBBB, lo, hi, out,
                        OSP_PROF_MAX_FRAMES);
  assert(n > 0);
  assert(out[0] != 0);
}

// The same floor `lr` gets. 42 is not a code address.
static void test_walk_applies_same_floor_to_pc_as_to_lr(void) {
  uint64_t mem[FAKE_STACK_WORDS];
  memset(mem, 0, sizeof(mem));
  uintptr_t lo = (uintptr_t)mem;
  uintptr_t hi = (uintptr_t)(mem + FAKE_STACK_WORDS);
  mem[0] = 0;
  mem[1] = 0x100000AAAA;
  uint64_t out[OSP_PROF_MAX_FRAMES];
  for (uint64_t bogus = 0; bogus < 4096; bogus += 1021) {
    int n = osp_prof_walk(bogus, (uint64_t)lo, 0x100000BBBB, lo, hi, out,
                          OSP_PROF_MAX_FRAMES);
    assert(n > 0);
    // An lr of `bogus` would be dropped here. A pc of `bogus` is kept.
    assert(out[0] != bogus);
  }
}

// [PROF-COLLECT-UNWIND] "Frame 0 is the precise PC."
//
// Precise means the instruction the sampled thread is executing. The observed
// capture on benchmarks/cases/fib violates this: the fp chain — which the same
// clause requires be validated for alignment, monotonic growth and [lo, hi)
// bounds — places the thread inside Osprey code, while frame 0 is 2 GB away in
// libsystem_kernel, reached from `fn sub(a, b) = a - b ?: 0`, which executes no
// syscall. Frame 0 and the chain cannot describe different threads: read_regs
// returns one register set. One of them is wrong, and the clause names frame 0.
static void test_frame_zero_is_the_precise_pc(void) {
  enum { CODE_LO = 0x100000000ULL, CODE_HI = 0x100100000ULL };
  const uint64_t observed_kernel_pc = 0x18030F483ULL;
  uint64_t mem[FAKE_STACK_WORDS];
  memset(mem, 0, sizeof(mem));
  uintptr_t lo = (uintptr_t)mem;
  uintptr_t hi = (uintptr_t)(mem + FAKE_STACK_WORDS);
  mem[0] = (uint64_t)(uintptr_t)&mem[8];
  mem[1] = CODE_LO + 0x2000; // `sub`
  mem[8] = 0;
  mem[9] = CODE_LO + 0x1000; // `fib`
  uint64_t out[OSP_PROF_MAX_FRAMES];
  int n = osp_prof_walk(observed_kernel_pc, (uint64_t)lo, 0, lo, hi, out,
                        OSP_PROF_MAX_FRAMES);
  assert(n >= 2);
  bool chain_in_code = true;
  for (int i = 1; i < n; i++) {
    chain_in_code &= out[i] >= CODE_LO && out[i] < CODE_HI;
  }
  assert(chain_in_code); // the validated chain agrees on where the thread is
  assert(out[0] >= CODE_LO && out[0] < CODE_HI); // frame 0 must too
}

#if defined(__APPLE__)
// [PROF-COLLECT-SAMPLER] "macOS: `thread_suspend` ->
// `thread_get_state(ARM_THREAD_STATE64)` -> frame-pointer walk ->
// `thread_resume`."
//
// That pipeline is the sole source of frame 0 on macOS, so the clause above is
// unsatisfiable until it yields the suspended thread's user-mode pc. The arm is
// quarantined; a child that runs to completion means it is live again and is
// publishing fictional self-time.
static void quarantined_capture_body(void) {
  char out[] = "/tmp/osprey-prof-quarantine-XXXXXX";
  int fd = mkstemp(out);
  if (fd >= 0) {
    (void)close(fd);
  }
  (void)osp_prof_start(out, TEST_RATE_HZ);
  // [PROF-COLLECT-REGISTRY] the sampler only samples REGISTERED threads, so
  // without this the macOS capture arm is never reached at all.
  osp_prof_thread_register(0, "main");
  g_sink += busy_work(40000000); // burn cpu so the sampler reaches read_regs
  osp_prof_thread_unregister();
  osp_prof_stop_and_dump();
  (void)unlink(out);
}

static void test_macos_capture_arm_is_quarantined(void) {
  assert(osp_death_signal(quarantined_capture_body) == SIGABRT);
}

// [PROF-TEST] "The C runtime suite verifies thread registration, sample
// capture, stack bounds, and raw JSON output."
//
// A quarantine whose reason cannot be read is not a verified capture failure,
// it is an unexplained SIGABRT. This one is GREEN on macOS, whose libc leaves
// stderr unbuffered even when redirected to a file, and it is here to stay that
// way: random_runtime.c's osp_entropy_exhausted documents a libc where a
// redirected stderr IS fully buffered, and abort() does not flush stdio, so the
// line naming the defect would die with the buffer. This arm has no fflush, so
// this assertion is what would catch that port.
static void test_quarantine_reason_survives_redirected_stderr(void) {
  char path[] = "/tmp/osprey-prof-quarantine-err-XXXXXX";
  int fd = mkstemp(path);
  assert(fd >= 0);
  pid_t pid = fork();
  assert(pid >= 0);
  if (pid == 0) {
    (void)dup2(fd, STDERR_FILENO); // a FILE => fully buffered
    (void)close(fd);
    quarantined_capture_body();
    _exit(0);
  }
  (void)osp_death_reap(pid, OSP_DEATH_BUDGET_SECONDS);
  char buf[4096];
  ssize_t got = pread(fd, buf, sizeof(buf) - 1, 0);
  (void)close(fd);
  (void)unlink(path);
  assert(got >= 0);
  buf[got] = 0;
  assert(strstr(buf, "leaf-PC capture is broken") != NULL);
}
#endif

int main(void) {
  test_walk_recovers_planted_chain();
  test_walk_dedupes_lr();
  test_walk_rejects_invalid_fp();
  test_walk_rejects_pc_below_min_code_addr();
  test_walk_applies_same_floor_to_pc_as_to_lr();
  test_frame_zero_is_the_precise_pc();
#if defined(__APPLE__)
  test_macos_capture_arm_is_quarantined();
  test_quarantine_reason_survives_redirected_stderr();
#endif
  test_hooks_inactive_noop();
  test_record_drop_returns_sentinel();
  test_boot_and_clocks();
  test_slot_snapshot_generation();
  test_end_to_end_capture();
  test_churn_under_max_rate_sampling();
  printf("profiler_runtime_tests: all tests passed (sink=%f)\n", g_sink);
  return 0;
}
