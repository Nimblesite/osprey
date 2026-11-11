// Spec-derived conformance tests of the profiler [PROF-TEST]. Every assertion
// quotes the clause of docs/specs/tooling/0028-Profiler.md it enforces and runs
// in a forked child, so one body's session cannot leak into the next.
#include "profiler_tests.h"

#include "test_death.h"

// ---- SPEC-DERIVED CONFORMANCE: docs/specs/tooling/0028-Profiler.md ------------------
// Every assertion below quotes the clause it enforces and names its spec ID.
// Nothing here asserts behaviour the spec does not state.

// [PROF-ACTIVATE-ENV] "The profiler is compiled into every runtime archive and
// is off by default. It activates only when the environment variable
// `OSPREY_PROFILE=<path>` is set at process start".
static void body_inactive_without_env(void) {
  unsetenv("OSPREY_PROFILE");
  assert(!osp_prof_is_active()); // off by default
  osp_prof_boot();
  assert(!osp_prof_is_active()); // no OSPREY_PROFILE => still off
}

// [PROF-ACTIVATE-ENV] "`OSPREY_PROFILE_HZ=<n>` overrides the sampling rate
// (clamped to 10..10000, default 997)." `hz == NULL` boots with the variable
// unset: the spec's stated default 997.
static void rate_env_yields(const char *hz, uint32_t expect) {
  char path[] = "/tmp/osprey-prof-rate-XXXXXX";
  int fd = mkstemp(path);
  assert(fd >= 0);
  (void)close(fd);
  setenv("OSPREY_PROFILE", path, 1);
  if (hz != NULL) {
    setenv("OSPREY_PROFILE_HZ", hz, 1);
  } else {
    unsetenv("OSPREY_PROFILE_HZ");
  }
  osp_prof_boot();
  assert(osp_prof_rate_hz() == expect);
  (void)unlink(path);
}
static void body_rate_default_997(void) { rate_env_yields(NULL, 997); }
static void body_rate_clamped_low(void) { rate_env_yields("5", 10); }
static void body_rate_clamped_high(void) { rate_env_yields("99999", 10000); }
static void body_rate_passthrough(void) { rate_env_yields("2000", 2000); }

// [PROF-COLLECT-REGISTRY] "`osp_prof_thread_register(fiber_id, label)` /
// `osp_prof_thread_unregister()` are no-ops until a session or the live-attach
// endpoint enables registration".
static void body_registry_noop_when_inactive(void) {
  unsetenv("OSPREY_PROFILE");
  assert(!osp_prof_is_active());
  osp_prof_thread_register(0, "main");   // must not activate anything
  assert(osp_prof_self_slot() == NULL);  // and must not register
  osp_prof_thread_register(-1, "effect");
  osp_prof_thread_unregister();
  osp_prof_thread_unregister();          // unbalanced: still a no-op
  assert(!osp_prof_is_active());
}

static void test_activation_and_registry_conform_to_spec(void) {
  assert(osp_death_signal(body_inactive_without_env) == 0);
  assert(osp_death_signal(body_rate_default_997) == 0);
  assert(osp_death_signal(body_rate_clamped_low) == 0);
  assert(osp_death_signal(body_rate_clamped_high) == 0);
  assert(osp_death_signal(body_rate_passthrough) == 0);
  assert(osp_death_signal(body_registry_noop_when_inactive) == 0);
}

// ---- shared session drivers [PROF-RAW-FORMAT] -----------------------------
// Every body below dumps to a fresh output path — a stale dump from an
// earlier run must never be read as this run's [PROF-ACTIVATE-ENV] — then
// reads the dump back and releases it; must_find asserts a named key exists.
static void start_fresh(const char *out) {
  (void)unlink(out);
  assert(osp_prof_start(out, TEST_RATE_HZ));
}

// A live session [PROF-COLLECT-SAMPLER]: `main` registered and burning CPU.
static char *capture_dump(const char *out) {
  start_fresh(out);
  osp_prof_thread_register(0, "main");
  g_sink += busy_work(20000000);
  osp_prof_thread_unregister();
  osp_prof_stop_and_dump();
  return slurp(out);
}

// An idle session: nothing registered, so only header keys [PROF-RAW-FORMAT].
static char *idle_dump(const char *out) {
  start_fresh(out);
  osp_prof_stop_and_dump();
  return slurp(out);
}

static const char *must_find(const char *json, const char *key) {
  const char *p = strstr(json, key);
  assert(p != NULL);
  return p;
}

// Drives must_find over a NULL-terminated table of one clause's keys.
static void keys_present(const char *json, const char *const *keys) {
  for (size_t i = 0; keys[i] != NULL; i++) {
    must_find(json, keys[i]);
  }
}

// Releases the JSON a *_dump above handed back.
static void release_dump(const char *out, char *json) {
  free(json);
  (void)unlink(out);
}

// [PROF-RAW-FORMAT] The dumped JSON carries every key the spec names, and
// [PROF-COLLECT-REGISTRY] the label of the registering call site.
// [PROF-COLLECT-SAMPLER] "State is on-CPU or waiting"; [PROF-RAW-FORMAT]
// "`samples` rows are `[t_rel_ns, thread_index, stack_index, state]` with
// state `0` = on-CPU, `1` = waiting."
//
// RED WHILE QUARANTINED: this needs a live capture, and the macOS capture arm
// aborts. It is the spec obligation that cannot be verified until the leaf pc
// is trustworthy, which is precisely the point of keeping it here.
static void body_raw_format_conforms(void) {
  const char *out = "/tmp/osprey_rawformat_spec.json";
  char *json = capture_dump(out);
  const char *const keys[] = {"\"version\":1",  "\"pid\":",     "\"rate_hz\":2000",
                              "\"platform\":",  "\"images\":",   "\"threads\":",
                              "\"stacks\":",    "\"samples\":",  "\"dropped\":",
                              NULL};
  keys_present(json, keys);
  must_find(json, "\"label\":\"main\""); // [PROF-COLLECT-REGISTRY]
  release_dump(out, json);
}

static void body_sample_rows_conform(void) {
  const char *out = "/tmp/osprey_samplerows_spec.json";
  char *json = capture_dump(out);
  const char *p = must_find(json, "\"samples\":[") + strlen("\"samples\":[");
  long rows = 0;
  while (*p && *p != ']') {
    if (*p == '[') {
      long long f[4] = {-1, -1, -1, -1};
      int got = sscanf(p, "[%lld,%lld,%lld,%lld]", &f[0], &f[1], &f[2], &f[3]);
      assert(got == 4);                       // exactly four fields
      assert(f[3] == OSP_PROF_STATE_ONCPU || f[3] == OSP_PROF_STATE_WAITING);
      assert(f[1] >= 0 && f[2] >= 0);
      rows++;
      while (*p && *p != ']') p++;
    }
    p++;
  }
  assert(rows > 0);
  release_dump(out, json);
}

static void test_raw_format_conforms_to_spec(void) {
  assert(osp_death_signal(body_raw_format_conforms) == 0);
  assert(osp_death_signal(body_sample_rows_conform) == 0);
}

// [PROF-RAW-FORMAT] "`stacks` are leaf-first raw return addresses." Recorded
// directly through osp_prof_record_sample, whose contract is the same word
// ("Intern `pcs` (leaf-first)"), so this verifies the FORMAT without needing a
// live capture — it is therefore NOT blocked by the macOS capture quarantine.
// [PROF-ACTIVATE-ENV] "the raw profile is written to `<path>` at normal exit".
static void body_stacks_are_leaf_first(void) {
  const char *out = "/tmp/osprey_leaffirst_spec.json";
  // No thread is registered, so the sampler has nothing to suspend.
  start_fresh(out);
  const uint64_t frames[3] = {0x100000AAAA, 0x100000BBBB, 0x100000CCCC};
  assert(osp_prof_record_sample(1000, 0, frames, 3, OSP_PROF_STATE_ONCPU) !=
         OSP_PROF_STACK_NONE);
  osp_prof_stop_and_dump();
  char *json = slurp(out); // written at exit, per [PROF-ACTIVATE-ENV]
  char d0[32], d1[32], d2[32]; // the raw file stores decimal addresses
  snprintf(d0, sizeof(d0), "%llu", (unsigned long long)frames[0]);
  snprintf(d1, sizeof(d1), "%llu", (unsigned long long)frames[1]);
  snprintf(d2, sizeof(d2), "%llu", (unsigned long long)frames[2]);
  const char *leaf = strstr(json, d0);
  const char *mid = strstr(json, d1);
  const char *root = strstr(json, d2);
  assert(leaf && mid && root);
  assert(leaf < mid && mid < root); // leaf-first, exactly as the clause says
  release_dump(out, json);
}

// [PROF-RAW-FORMAT] The `images` entries carry `path`, `base` and `slide`; the
// file names the `exe`; `start_unix_ns`/`end_unix_ns` bracket the run; and
// there is "A single top-level `dropped` counter".
static void body_raw_header_fields(void) {
  const char *out = "/tmp/osprey_rawheader_spec.json";
  char *json = idle_dump(out);
  const char *const keys[] = {"\"exe\":",     "\"start_unix_ns\":", "\"end_unix_ns\":",
                              "\"images\":",   "\"path\":",         "\"base\":",
                              "\"slide\":",    NULL};
  keys_present(json, keys);
  const char *d = must_find(json, "\"dropped\":");
  assert(strstr(d + 1, "\"dropped\":") == NULL); // "A single top-level" counter
  release_dump(out, json);
}

// [PROF-RAW-FORMAT] "`text`/`text_size` are the start and byte length of the
// image's executable range". [PROF-SYMBOLIZE-OFFLINE] decides which image owns
// a pc by containment in that range, so a range that does not really contain
// the image's code hands every one of its samples to a neighbouring image --
// which is exactly how a libsystem_malloc leaf was reported as
// `task_get_special_port`. Presence of the two keys proves nothing; only
// containment does.
//
// The first row is this process's own executable on BOTH producers
// (`_dyld_image_count` index 0, and `dl_iterate_phdr`'s first callback), so its
// range must contain the address of a function compiled into this very binary.
static void body_image_ranges_locate_this_program_code(void) {
  const char *out = "/tmp/osprey_imagerange_spec.json";
  char *json = idle_dump(out);
  const char *p = must_find(json, "\"text\":");
  uint64_t text = 0;
  uint64_t size = 0;
  assert(sscanf(p, "\"text\":%" SCNu64 ",\"text_size\":%" SCNu64, &text,
                &size) == 2);
  // 0 means "the producer could not determine the range", never "empty image".
  assert(size > 0);
  const uint64_t here =
      (uint64_t)(uintptr_t)&body_image_ranges_locate_this_program_code;
  assert(here >= text);
  assert(here - text < size);
  release_dump(out, json);
}

// [PROF-COLLECT-REGISTRY] "Call sites: the main thread (label `main`, fiber 0),
// `fiber_thread_func` (label `fiber`), and effect continuation threads (label
// `effect`, fiber -1)." Every label the spec names must round-trip.
// [PROF-RAW-FORMAT] `threads` rows are `{"fiber":N,"label":"..."}`.
static void body_registry_labels_round_trip(void) {
  const char *out = "/tmp/osprey_labels_spec.json";
  start_fresh(out);
  osp_prof_thread_register(0, "main");
  osp_prof_thread_unregister();
  osp_prof_thread_register(1, "fiber");
  osp_prof_thread_unregister();
  osp_prof_thread_register(-1, "effect");
  osp_prof_thread_unregister();
  osp_prof_stop_and_dump();
  char *json = slurp(out);
  must_find(json, "\"label\":\"main\"");
  must_find(json, "\"label\":\"fiber\"");
  must_find(json, "\"label\":\"effect\"");
  must_find(json, "\"fiber\":0");   // main
  must_find(json, "\"fiber\":-1");  // effect continuation
  release_dump(out, json);
}

// [PROF-COLLECT-REGISTRY] slot capacity, and the label field width the header
// fixes at OSP_PROF_LABEL_MAX: a longer label must be truncated, never
// overflowed, so the raw JSON stays well-formed [PROF-RAW-FORMAT].
static void body_label_is_bounded(void) {
  const char *out = "/tmp/osprey_labelmax_spec.json";
  assert(OSP_PROF_MAX_THREADS == 1024);
  assert(OSP_PROF_LABEL_MAX == 15);
  start_fresh(out);
  osp_prof_thread_register(7, "an-extremely-long-label-beyond-the-cap");
  osp_prof_thread_unregister();
  osp_prof_stop_and_dump();
  char *json = slurp(out);
  const char *l = must_find(json, "\"label\":\"an-extremely");
  const char *close = strchr(l + strlen("\"label\":\""), '"');
  assert(close != NULL);
  assert((size_t)(close - (l + strlen("\"label\":\""))) <= OSP_PROF_LABEL_MAX);
  release_dump(out, json);
}

// [PROF-COLLECT-SAMPLER] "Samples record `(t_ns, thread, stack, state)`. State
// is on-CPU or waiting" — the two states the enum fixes at 0 and 1, matching
// [PROF-RAW-FORMAT] "state `0` = on-CPU, `1` = waiting".
static void test_sample_state_encoding_matches_spec(void) {
  assert(OSP_PROF_STATE_ONCPU == 0);
  assert(OSP_PROF_STATE_WAITING == 1);
}

static void test_raw_format_fields_conform_to_spec(void) {
  assert(osp_death_signal(body_stacks_are_leaf_first) == 0);
  assert(osp_death_signal(body_raw_header_fields) == 0);
  assert(osp_death_signal(body_registry_labels_round_trip) == 0);
  assert(osp_death_signal(body_label_is_bounded) == 0);
  assert(osp_death_signal(body_image_ranges_locate_this_program_code) == 0);
}

// ---- macOS stack capture: quarantine guards ---------------------------------
// The defect these guard is NOT reproducible from this suite: the leaf pc is
// captured correctly for a plain cc-built binary (measured 17/17 unique stacks
// resolving to the test image via dladdr). It reproduces only for an
// Osprey-compiled program, so the assertion that pins it lives in
// scripts/tests/test_profiler.sh, which [PROF-TEST] designates for exactly that:
// "The end-to-end profiler script runs an example under `--profile` and parses
// every export." That assertion was measured RED before the quarantine landed
// (72.2% of self-samples on kernel or unsymbolized leaves) and is now masked by
// the abort below, which fires first.
//
// Ground truth for the defect, from macOS's own sampler on the compiled
// src/toolchain/benchmarks/cases/fib binary:
//
//     549 main (in fib) + 36
//       531 fib (in fib) + 124        <- every frame in the Osprey image
//
// `sample` attributes 549/549 to Osprey code and nothing to libsystem, while
// the Osprey profiler reported 49.3% self in task_get_special_port. The two
// cannot both be right, and `sample` is not the one under test.

#if defined(__APPLE__)
// [PROF-COLLECT-SAMPLER] "macOS: `thread_suspend` ->
// `thread_get_state(ARM_THREAD_STATE64)` -> frame-pointer walk ->
// `thread_resume`."
//
// That pipeline is the sole source of frame 0 on macOS, so it is asserted here
// end to end: the child must run to completion and its dump must carry real
// stacks. A child that dies, or one that emits an envelope with empty arrays,
// means the arm stopped sampling and every later percentage is vacuous.
// Where the forked capture child hands its dump path back to the parent.
#define OSP_CAPTURE_PATH_HANDOFF "/tmp/osprey-prof-capture-path"

// The dump the last capture child wrote must carry real stacks. A capture that
// walked nothing still emits the envelope, so the arrays are what is asserted.
static void assert_capture_dump_has_samples(void) {
  FILE *hand = fopen(OSP_CAPTURE_PATH_HANDOFF, "r");
  assert(hand != NULL);
  char path[256] = {0};
  assert(fgets(path, (int)sizeof(path), hand) != NULL);
  (void)fclose(hand);
  path[strcspn(path, "\n")] = '\0';
  FILE *dump = fopen(path, "r");
  assert(dump != NULL);
  static char body[1 << 20];
  size_t n = fread(body, 1, sizeof(body) - 1, dump);
  body[n] = '\0';
  (void)fclose(dump);
  (void)unlink(path);
  (void)unlink(OSP_CAPTURE_PATH_HANDOFF);
  must_find(body, "\"stacks\":[[");
  must_find(body, "\"samples\":[[");
}

static void macos_capture_body(void) {
  char out[] = "/tmp/osprey-prof-capture-XXXXXX";
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
  // The dump IS the evidence the assertion reads; the parent unlinks it.
  FILE *hand = fopen(OSP_CAPTURE_PATH_HANDOFF, "w");
  if (hand) {
    (void)fputs(out, hand);
    (void)fclose(hand);
  }
}

#endif // __APPLE__ -- macos_capture_body drives the macOS capture arm.

#if defined(__APPLE__)
// [PROF-COLLECT-UNWIND] The macOS capture arm must RUN and produce samples.
// It once aborted here, quarantined on the belief that thread_get_state
// returned a kernel pc for a user-mode thread. It does not: a probe that
// suspended a thread spinning in pure integer arithmetic read a pc inside that
// function 40 times out of 40, and dladdr on 160 captured leaves from
// src/toolchain/benchmarks/cases/fib resolved 128 of them inside libsystem_malloc — real
// allocator frames, because `a + b ?: 0` allocates a Result per operation.
// The surprising leaf was the `?:` tax measured accurately, not a bad capture.
static void test_macos_capture_arm_produces_samples(void) {
  assert(osp_death_signal(macos_capture_body) == 0); // must not abort
  assert_capture_dump_has_samples();
}

// [PROF-TEST] "The C runtime suite verifies thread registration, sample
// capture, stack bounds, and raw JSON output."
//
// The capture arm must survive a REDIRECTED stderr. macOS libc leaves stderr
// unbuffered even when redirected to a file, so this passes here; it is kept
// because random_runtime.c documents a libc where a redirected stderr IS fully
// buffered, and a port to that libc must not lose the dump this asserts on.
static void test_capture_arm_survives_redirected_stderr(void) {
  char path[] = "/tmp/osprey-prof-capture-err-XXXXXX";
  int fd = mkstemp(path);
  assert(fd >= 0);
  pid_t pid = fork();
  assert(pid >= 0);
  if (pid == 0) {
    (void)dup2(fd, STDERR_FILENO); // a FILE => fully buffered
    (void)close(fd);
    macos_capture_body();
    _exit(0);
  }
  assert(osp_death_reap(pid, OSP_DEATH_BUDGET_SECONDS) == 0);
  (void)close(fd);
  (void)unlink(path);
  assert_capture_dump_has_samples();
}
#endif

void run_spec_tests(void) {
  test_activation_and_registry_conform_to_spec();
  test_raw_format_conforms_to_spec();
  test_raw_format_fields_conform_to_spec();
  test_sample_state_encoding_matches_spec();
#if defined(__APPLE__)
  test_macos_capture_arm_produces_samples();
  test_capture_arm_survives_redirected_stderr();
#endif
}
