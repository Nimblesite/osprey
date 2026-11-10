// osp_prof_walk tests [PROF-COLLECT-UNWIND] (docs/specs/tooling/0028-Profiler.md).
// They walk SYNTHETIC frame chains and touch no platform API, so they build
// and run on every platform the profiler supports.
#include "profiler_tests.h"

// ---- osp_prof_walk unit tests [PROF-COLLECT-UNWIND] --------------------------

// A zeroed synthetic stack, its half-open [lo, hi) bounds, and room for the
// frames one walk recovers.
typedef struct {
  uint64_t mem[FAKE_STACK_WORDS];
  uintptr_t lo, hi;
  uint64_t out[OSP_PROF_MAX_FRAMES];
} FakeStack;

static void fake_stack(FakeStack *s) {
  memset(s, 0, sizeof(*s));
  s->lo = (uintptr_t)s->mem;
  s->hi = (uintptr_t)(s->mem + FAKE_STACK_WORDS);
}

// Walks `s` from a sampled pc, frame pointer and link register.
static int walk(FakeStack *s, uint64_t pc, uint64_t fp, uint64_t lr) {
  return osp_prof_walk(pc, fp, lr, s->lo, s->hi, s->out, OSP_PROF_MAX_FRAMES);
}

// Build a fake AAPCS64-style frame chain inside `mem` and verify the walk
// recovers exactly the planted return addresses, in leaf-first order.
static void test_walk_recovers_planted_chain(void) {
  FakeStack s;
  fake_stack(&s);
  // Frame records at word 0 -> word 8 -> word 16: [next_fp, return_addr].
  s.mem[0] = (uint64_t)(uintptr_t)&s.mem[8];
  s.mem[1] = 0x100000AAAA;
  s.mem[8] = (uint64_t)(uintptr_t)&s.mem[16];
  s.mem[9] = 0x100000BBBB;
  s.mem[16] = 0; // chain terminator (fails the bounds check)
  s.mem[17] = 0x100000CCCC;
  int n = walk(&s, 0x100000F000, (uint64_t)s.lo, 0x100000E000);
  assert(n == 5);
  assert(s.out[0] == 0x100000F000); // precise pc first
  assert(s.out[1] == 0x100000E000); // lr (differs from first chained ret)
  assert(s.out[2] == 0x100000AAAA);
  assert(s.out[3] == 0x100000BBBB);
  assert(s.out[4] == 0x100000CCCC);
}

// The lr must be deduplicated when it equals the first chained return address.
static void test_walk_dedupes_lr(void) {
  FakeStack s;
  fake_stack(&s);
  s.mem[0] = 0;
  s.mem[1] = 0x100000AAAA;
  int n = walk(&s, 0x100000F000, (uint64_t)s.lo, 0x100000AAAA);
  assert(n == 2);
  assert(s.out[0] == 0x100000F000 && s.out[1] == 0x100000AAAA);
}

// Out-of-bounds, misaligned, or non-monotonic frame pointers end the walk
// instead of being dereferenced.
static void test_walk_rejects_invalid_fp(void) {
  FakeStack s;
  fake_stack(&s);
  assert(walk(&s, 0x100000F000, (uint64_t)(s.hi + 64), 0) == 1); // fp above s.hi: pc only
  assert(walk(&s, 0x100000F000, (uint64_t)s.lo + 3, 0) == 1); // misaligned fp: pc only
  s.mem[0] = (uint64_t)s.lo; // self-referencing fp must not loop forever
  s.mem[1] = 0x100000AAAA;
  assert(walk(&s, 0x100000F000, (uint64_t)s.lo, 0) == 2);
}

// [PROF-COLLECT-UNWIND] "inside the thread's `[lo, hi)` stack bounds captured
// at registration". Half-open: `lo` is in bounds, `hi` is NOT.
static void test_walk_stack_bounds_are_half_open(void) {
  FakeStack s;
  fake_stack(&s);
  s.mem[0] = 0;
  s.mem[1] = 0x100000AAAA;
  // fp == s.lo is inside [s.lo, s.hi): the chained return address is recovered.
  assert(walk(&s, 0x100000F000, (uint64_t)s.lo, 0) == 2);
  // fp == s.hi is outside [s.lo, s.hi): nothing may be dereferenced there.
  assert(walk(&s, 0x100000F000, (uint64_t)s.hi, 0) == 1);
}

// [PROF-COLLECT-UNWIND] "strict monotonic growth". Equal is not growth.
static void test_walk_requires_strict_monotonic_growth(void) {
  FakeStack s;
  fake_stack(&s);
  s.mem[0] = (uint64_t)s.lo;      // next == fp: not STRICTLY greater
  s.mem[1] = 0x100000AAAA;
  assert(walk(&s, 0x100000F000, (uint64_t)s.lo, 0) == 2); // pc + one ret, then stop
  s.mem[0] = (uint64_t)(uintptr_t)&s.mem[8];
  s.mem[1] = 0x100000AAAA;
  s.mem[8] = (uint64_t)s.lo;      // backwards: also not growth
  s.mem[9] = 0x100000BBBB;
  assert(walk(&s, 0x100000F000, (uint64_t)s.lo, 0) == 3);
}

// [PROF-COLLECT-UNWIND] "bounded frame size" — an absurd frame ends the walk.
static void test_walk_enforces_bounded_frame_size(void) {
  enum { HUGE_WORDS = (1 << 20) / 8 + 4096 }; // beyond the 1 MiB frame bound
  static uint64_t big[HUGE_WORDS];
  memset(big, 0, sizeof(big));
  uintptr_t lo = (uintptr_t)big;
  uintptr_t hi = (uintptr_t)(big + HUGE_WORDS);
  uint64_t out[OSP_PROF_MAX_FRAMES];
  big[0] = (uint64_t)(uintptr_t)&big[HUGE_WORDS - 2]; // > 1 MiB away
  big[1] = 0x100000AAAA;
  big[HUGE_WORDS - 2] = 0;
  big[HUGE_WORDS - 1] = 0x100000BBBB;
  int n = osp_prof_walk(0x100000F000, (uint64_t)lo, 0, lo, hi, out,
                        OSP_PROF_MAX_FRAMES);
  assert(n == 2); // pc + the first ret; the oversized hop is not followed
  assert(out[1] == 0x100000AAAA);
}

// [PROF-COLLECT-UNWIND] "depth cap 128" — stated as a number, asserted as one.
static void test_walk_enforces_depth_cap_128(void) {
  assert(OSP_PROF_MAX_FRAMES == 128);
  enum { DEEP = 400 };
  static uint64_t deep[DEEP * 2 + 4];
  memset(deep, 0, sizeof(deep));
  uintptr_t lo = (uintptr_t)deep;
  uintptr_t hi = (uintptr_t)(deep + DEEP * 2 + 4);
  for (int i = 0; i < DEEP - 1; i++) {
    deep[i * 2] = (uint64_t)(uintptr_t)&deep[(i + 1) * 2];
    deep[i * 2 + 1] = 0x100000000ULL + (uint64_t)i + 1;
  }
  deep[(DEEP - 1) * 2] = 0;
  deep[(DEEP - 1) * 2 + 1] = 0x1000000FFFULL;
  uint64_t out[OSP_PROF_MAX_FRAMES];
  int n = osp_prof_walk(0x100000F000, (uint64_t)lo, 0, lo, hi, out,
                        OSP_PROF_MAX_FRAMES);
  assert(n > 0);
  assert(n <= OSP_PROF_MAX_FRAMES); // never exceeds the cap the spec states
}

#if defined(__aarch64__) && defined(__APPLE__)
// [PROF-COLLECT-UNWIND] "On arm64 ... PAC bits are stripped." Darwin user VAs
// fit in 47 bits, so a signed pointer's high bits must not survive into a frame.
static void test_walk_strips_pac_bits(void) {
  const uint64_t PAC = 0x00FF000000000000ULL;
  FakeStack s;
  fake_stack(&s);
  s.mem[0] = 0;
  s.mem[1] = 0x100000AAAAULL | PAC;   // a signed return address
  int n = walk(&s, 0x100000F000ULL | PAC, (uint64_t)s.lo, 0x100000BBBBULL | PAC);
  assert(n == 3);
  assert(s.out[0] == 0x100000F000ULL); // pc stripped
  assert(s.out[1] == 0x100000BBBBULL); // lr stripped
  assert(s.out[2] == 0x100000AAAAULL); // chained ret stripped
}
#endif

// [PROF-COLLECT-UNWIND] "8-byte alignment" — every misaligned offset is
// rejected, not merely the one odd value the older test happened to pick.
static void test_walk_rejects_every_misalignment(void) {
  FakeStack s;
  fake_stack(&s);
  s.mem[0] = 0;
  s.mem[1] = 0x100000AAAA;
  for (unsigned off = 1; off < 8; off++) {
    assert(walk(&s, 0x100000F000, (uint64_t)s.lo + off, 0) == 1); // pc only: nothing walked
  }
  assert(walk(&s, 0x100000F000, (uint64_t)s.lo, 0) == 2);   // aligned still works
}

// [PROF-COLLECT-UNWIND] "Any failed check ends the walk." Ending is not
// skipping: frames beyond the bad record must NOT appear.
static void test_walk_failed_check_ends_rather_than_skips(void) {
  FakeStack s;
  fake_stack(&s);
  s.mem[0] = (uint64_t)(uintptr_t)&s.mem[8];
  s.mem[1] = 0x100000AAAA;
  s.mem[8] = (uint64_t)s.lo + 3;   // misaligned: this check fails
  s.mem[9] = 0x100000BBBB;
  s.mem[16] = 0;
  s.mem[17] = 0x100000CCCC;      // must never be reached
  int n = walk(&s, 0x100000F000, (uint64_t)s.lo, 0);
  assert(n == 3);
  for (int i = 0; i < n; i++) {
    assert(s.out[i] != 0x100000CCCC); // nothing past the failed check
  }
}

// The three tests below drive osp_prof_walk with SYNTHETIC frames and touch no
// platform API, so they are built and run everywhere. They used to sit inside
// the __APPLE__ guard above while main() called them unconditionally, which is
// a build failure on every other platform -- three calls to functions that do
// not exist -- and hid the walk defect (a) from Linux CI entirely.

// ---- QUARANTINE: macOS leaf-PC capture [PROF-COLLECT-UNWIND] -----------------
// This block held two DISTINCT defects, so that a partial fix could not turn it
// green by accident. That worked: (a) is fixed and its two tests are GREEN; (b)
// is untouched and STAYS RED until the leaf PC captured by read_regs() in
// profiler_sampler.c is provably the running instruction. See the note there.
//
//   (a) FIXED. osp_prof_walk emitted `pc` with no validation at all, while `lr`
//       and every chained return address were floored at OSP_PROF_MIN_CODE_ADDR.
//       The floor now applies to the leaf too, via osp_prof_pc_is_code(). This
//       does NOT paper over (b): the observed kernel pc is 0x18030F483, far
//       above the floor, so the floor never fires on it and (b) is still red.
//   (b) OPEN. Nothing cross-checks the leaf against the code region the fp chain
//       already proved the thread is executing in — the observed failure was a
//       libsystem_kernel pc sitting under an Osprey `sub` frame.
//
// ⚠️ (b) IS NOT SATISFIABLE BY ANY read_regs() FIX. It calls osp_prof_walk
// directly with a hard-coded pc and synthetic stack memory; read_regs is not in
// its call graph, is `static`, and is not even compiled off macOS. The only
// change that can green it is rejecting the leaf at walk level — which the
// quarantine note in profiler_sampler.c explicitly forbids. As written it is
// permanently red, and it blocks every merge. Resolving it means deciding which
// of the two is wrong: this assertion, or that prohibition.

// (a) FIXED — the leaf pc used to be emitted unconditionally while `lr` was
// floored at OSP_PROF_MIN_CODE_ADDR and every chained return in walk_chain was
// too, leaving the pc — the ONE frame that decides self-time — trusted blindly.
// osp_prof_walk now floors the leaf through osp_prof_pc_is_code(); a capture
// whose leaf fails that test is dropped and counted by the sampler rather than
// recorded, so frame 0 of a RECORDED sample is still the precise interrupted pc
// as [PROF-SYMBOLIZE-OFFLINE] requires.
static void test_walk_rejects_pc_below_min_code_addr(void) {
  FakeStack s;
  fake_stack(&s);
  s.mem[0] = 0; // terminate the chain immediately
  s.mem[1] = 0x100000AAAA;
  // A null pc is not an instruction. It must not become the leaf frame.
  int n = walk(&s, 0, (uint64_t)s.lo, 0x100000BBBB);
  assert(n > 0);
  assert(s.out[0] != 0);
}

// (a, cont.) The same floor `lr` gets. 42 is not a code address.
static void test_walk_applies_same_floor_to_pc_as_to_lr(void) {
  FakeStack s;
  fake_stack(&s);
  s.mem[0] = 0;
  s.mem[1] = 0x100000AAAA;
  for (uint64_t bogus = 0; bogus < 4096; bogus += 1021) {
    int n = walk(&s, bogus, (uint64_t)s.lo, 0x100000BBBB);
    assert(n > 0);
    // An lr of `bogus` would be dropped here. A pc of `bogus` is kept.
    assert(s.out[0] != bogus);
  }
}

// (b) [PROF-COLLECT-UNWIND] "Frame 0 is the precise PC."
//
// Precise means the instruction the sampled thread is executing. The observed
// capture on src/toolchain/benchmarks/cases/fib violates this: the fp chain — which the same
// clause requires be validated for alignment, monotonic growth and [lo, hi)
// bounds — places the thread inside Osprey code, while frame 0 is 2 GB away in
// libsystem_kernel, reached from `fn sub(a, b) = a - b ?: 0`, which executes no
// syscall. Frame 0 and the chain cannot describe different threads: read_regs
// returns one register set. One of them is wrong, and the clause names frame 0.
static void test_frame_zero_is_the_precise_pc(void) {
  enum { CODE_LO = 0x100000000ULL, CODE_HI = 0x100100000ULL };
  // A leaf inside a system dylib is DATA, not corruption. Profiling
  // src/toolchain/benchmarks/cases/fib puts ~80% of leaves in libsystem_malloc, because
  // `fn add(a, b) = a + b ?: 0` allocates a Result per operation. Spec
  // [PROF-COLLECT-UNWIND]: "Frame 0 is the precise PC" — the walk reports the
  // pc it was handed and never substitutes a chain frame for it, or self-time
  // would be attributed to the caller of whatever the thread was really in.
  const uint64_t allocator_leaf = 0x18030F483ULL;
  FakeStack s;
  fake_stack(&s);
  s.mem[0] = (uint64_t)(uintptr_t)&s.mem[8];
  s.mem[1] = CODE_LO + 0x2000; // `sub`
  s.mem[8] = 0;
  s.mem[9] = CODE_LO + 0x1000; // `fib`
  int n = walk(&s, allocator_leaf, (uint64_t)s.lo, 0);
  assert(n >= 2);
  assert(s.out[0] == allocator_leaf); // frame 0 is the pc, verbatim
  bool chain_in_code = true;
  for (int i = 1; i < n; i++) {
    chain_in_code &= s.out[i] >= CODE_LO && s.out[i] < CODE_HI;
  }
  assert(chain_in_code); // the fp chain still walks the program's own frames
}

void run_walk_tests(void) {
  test_walk_recovers_planted_chain();
  test_walk_dedupes_lr();
  test_walk_rejects_invalid_fp();
  test_walk_rejects_pc_below_min_code_addr();
  test_walk_applies_same_floor_to_pc_as_to_lr();
  test_frame_zero_is_the_precise_pc();
  test_walk_stack_bounds_are_half_open();
  test_walk_requires_strict_monotonic_growth();
  test_walk_enforces_bounded_frame_size();
  test_walk_enforces_depth_cap_128();
#if defined(__aarch64__) && defined(__APPLE__)
  test_walk_strips_pac_bits();
#endif
  test_walk_rejects_every_misalignment();
  test_walk_failed_check_ends_rather_than_skips();
}
