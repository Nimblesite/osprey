// Osprey sampling CPU profiler — platform sampler backends. Implements
// [PROF-COLLECT-SAMPLER] (docs/specs/0028-Profiler.md).
//
// macOS: a dedicated sampler thread suspends each registered thread, reads its
// register state, walks the frame-pointer chain, and resumes it — no signals,
// so blocking syscalls are never EINTR-perturbed.
//
// Linux: the sampler thread directs SIGPROF at running threads; the
// async-signal-safe handler walks its own stack from ucontext into a
// per-thread SPSC ring the sampler drains. Blocked threads get one signal to
// capture their blocking stack, then "waiting" samples reuse it.
//
// Locking discipline: every per-slot sampling action happens under the
// registry lock and starts by revalidating the snapshot generation
// [PROF-COLLECT-REGISTRY] — unregister takes the same lock and always precedes
// the completion signal that gates pthread_join, so a validated slot's
// pthread/mach port is live for the duration. Lock order is registry -> malloc
// (matching the registry's own append_row), and nothing allocates while a
// target thread is suspended.
#include "profiler_runtime.h"

#if !defined(_WIN32) && !defined(__wasm__)

#include <errno.h>
#include <stdatomic.h>
#include <stdlib.h>
#include <time.h>

#if defined(__APPLE__)
#include <mach/mach.h>
#include <mach/mach_time.h>
#include <pthread/qos.h>
#else
#include <signal.h>
#include <string.h>
#include <ucontext.h>
#endif

static pthread_t g_sampler_thread;
static atomic_bool g_sampler_stop = false;

// ---- jittered pacing (McCanne-Torek randomized sampling clock) --------------

static uint64_t g_rng_state = 0x9E3779B97F4A7C15ULL;

static uint64_t rng_next(void) {
  g_rng_state ^= g_rng_state << 13;
  g_rng_state ^= g_rng_state >> 7;
  g_rng_state ^= g_rng_state << 17;
  return g_rng_state;
}

// One sampling period ±30%, re-drawn every tick so samples never phase-lock
// with periodic program activity.
static uint64_t jittered_period_ns(void) {
  uint64_t period = 1000000000ULL / osp_prof_rate_hz();
  uint64_t span = (period * 3) / 5;
  return period - span / 2 + (span ? rng_next() % span : 0);
}

// Deadline-paced wait. Relative nanosleep(1ms) on macOS routinely oversleeps
// 3-4x (timer coalescing), silently cutting the sampling rate to ~300Hz —
// absolute-deadline waits hold the requested rate. A missed deadline resets to
// "now" instead of accumulating debt (no burst of catch-up ticks).
#if defined(__APPLE__)
static mach_timebase_info_data_t g_timebase;

static void wait_until_next(uint64_t *deadline_ns) {
  *deadline_ns += jittered_period_ns();
  uint64_t now = osp_prof_mono_ns();
  if (*deadline_ns <= now || g_timebase.numer == 0) {
    *deadline_ns = now;
    return;
  }
  uint64_t delta_abs =
      (*deadline_ns - now) * g_timebase.denom / g_timebase.numer;
  mach_wait_until(mach_absolute_time() + delta_abs);
}
#else
static void wait_until_next(uint64_t *deadline_ns) {
  *deadline_ns += jittered_period_ns();
  uint64_t now = osp_prof_mono_ns();
  if (*deadline_ns <= now) {
    *deadline_ns = now;
    return;
  }
  struct timespec ts = {.tv_sec = (time_t)(*deadline_ns / 1000000000ULL),
                        .tv_nsec = (long)(*deadline_ns % 1000000000ULL)};
  while (clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &ts, NULL) == EINTR) {
  }
}
#endif

// ---- per-thread CPU time -----------------------------------------------------

#if defined(__APPLE__)
// CLOCK_THREAD_CPUTIME_ID under-reports on Apple Silicon; thread_info is the
// reliable source.
static uint64_t mach_thread_cpu_ns(uint32_t port) {
  thread_basic_info_data_t info;
  mach_msg_type_number_t count = THREAD_BASIC_INFO_COUNT;
  if (thread_info(port, THREAD_BASIC_INFO, (thread_info_t)&info, &count) !=
      KERN_SUCCESS) {
    return 0;
  }
  uint64_t us = (uint64_t)info.user_time.seconds * 1000000ULL +
                (uint64_t)info.user_time.microseconds +
                (uint64_t)info.system_time.seconds * 1000000ULL +
                (uint64_t)info.system_time.microseconds;
  return us * 1000ULL;
}

uint64_t osp_prof_self_cpu_ns(void) {
  return mach_thread_cpu_ns(pthread_mach_thread_np(pthread_self()));
}
#else
static uint64_t pthread_cpu_ns(pthread_t thread) {
  clockid_t cid;
  struct timespec ts;
  if (pthread_getcpuclockid(thread, &cid) != 0 ||
      clock_gettime(cid, &ts) != 0) {
    return 0;
  }
  return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

uint64_t osp_prof_self_cpu_ns(void) { return pthread_cpu_ns(pthread_self()); }
#endif

// A thread is "on CPU" for this tick when it consumed at least half of the
// wall time since its previous observation.
static uint8_t classify_tick(OspProfSlot *slot, uint64_t now, uint64_t cpu) {
  uint64_t wall_d = now >= slot->wall_ns_prev ? now - slot->wall_ns_prev : 0;
  uint64_t cpu_d = cpu >= slot->cpu_ns_prev ? cpu - slot->cpu_ns_prev : 0;
  slot->wall_ns_prev = now;
  slot->cpu_ns_prev = cpu;
  if (wall_d == 0 || cpu_d * 2 >= wall_d) {
    return OSP_PROF_STATE_ONCPU;
  }
  return OSP_PROF_STATE_WAITING;
}

static void remember_stack(OspProfSlot *slot, uint32_t stack) {
  if (stack != OSP_PROF_STACK_NONE) {
    slot->last_stack = stack;
    slot->has_last_stack = true;
  }
}

#if defined(__APPLE__)

// ---- macOS: suspend / read state / walk / resume ----------------------------

typedef struct MachRegs {
  uint64_t pc, fp, lr;
  bool ok;
} MachRegs;

static MachRegs read_regs(uint32_t port) {
  MachRegs r = {0, 0, 0, false};
#if defined(__aarch64__)
  arm_thread_state64_t st;
  mach_msg_type_number_t count = ARM_THREAD_STATE64_COUNT;
  if (thread_get_state(port, ARM_THREAD_STATE64, (thread_state_t)&st, &count) ==
      KERN_SUCCESS) {
#if defined(arm_thread_state64_get_pc)
    r.pc = (uint64_t)arm_thread_state64_get_pc(st);
    r.fp = (uint64_t)arm_thread_state64_get_fp(st);
    r.lr = (uint64_t)arm_thread_state64_get_lr(st);
#else
    r.pc = st.__pc;
    r.fp = st.__fp;
    r.lr = st.__lr;
#endif
    r.ok = true;
  }
#else
  x86_thread_state64_t st;
  mach_msg_type_number_t count = x86_THREAD_STATE64_COUNT;
  if (thread_get_state(port, x86_THREAD_STATE64, (thread_state_t)&st, &count) ==
      KERN_SUCCESS) {
    r.pc = st.__rip;
    r.fp = st.__rbp;
    r.lr = 0;
    r.ok = true;
  }
#endif
  return r;
}

// Capture one thread's stack while it is suspended. Returns the frame count.
// Runs under the registry lock; allocates nothing.
static int capture_suspended(OspProfSlot *slot, uint64_t *frames) {
  if (thread_suspend(slot->mach_port) != KERN_SUCCESS) {
    return 0;
  }
  MachRegs regs = read_regs(slot->mach_port);
  int n = 0;
  if (regs.ok) {
    n = osp_prof_walk(regs.pc, regs.fp, regs.lr, slot->stack_lo, slot->stack_hi,
                      frames, OSP_PROF_MAX_FRAMES);
  }
  thread_resume(slot->mach_port);
  return n;
}

static void sample_thread(const OspProfSnap *snap) {
  osp_prof_registry_lock();
  if (!osp_prof_snap_live(snap)) {
    osp_prof_registry_unlock();
    return;
  }
  OspProfSlot *slot = snap->slot;
  uint64_t now = osp_prof_mono_ns();
  uint8_t state = classify_tick(slot, now, mach_thread_cpu_ns(slot->mach_port));
  uint64_t frames[OSP_PROF_MAX_FRAMES];
  int n = capture_suspended(slot, frames);
  if (n > 0) { // record allocates — only after the thread is resumed
    uint32_t stack =
        osp_prof_record_sample(now, slot->index, frames, (uint32_t)n, state);
    remember_stack(slot, stack);
  }
  osp_prof_registry_unlock();
}

static bool platform_setup(void) {
  return mach_timebase_info(&g_timebase) == KERN_SUCCESS;
}
static void platform_teardown(void) {}

#else

// ---- Linux: directed SIGPROF into per-thread SPSC rings ----------------------

enum { OSP_PROF_RING_ENTRIES = 8 };

typedef struct RingEntry {
  uint64_t t_ns;
  uint32_t row; // thread row stamped at capture, immune to slot recycling
  uint32_t n;
  uint64_t pcs[OSP_PROF_MAX_FRAMES];
} RingEntry;

typedef struct Ring {
  _Atomic uint32_t head; // written by the signal handler only
  _Atomic uint32_t tail; // written by the sampler thread only
  _Atomic uint64_t drops;
  RingEntry entries[OSP_PROF_RING_ENTRIES];
} Ring;

static void sigprof_handler(int sig, siginfo_t *si, void *uctx) {
  (void)sig;
  (void)si;
  int saved_errno = errno;
  OspProfSlot *slot = osp_prof_self_slot();
  // The active check rejects a signal that was in flight across this thread's
  // unregister — by then the slot may be recycled with another thread's stack
  // bounds, which must never guide a walk of THIS thread's stack.
  Ring *ring = (slot && slot->active) ? slot->ring : NULL;
  if (ring) {
    uint32_t head = atomic_load_explicit(&ring->head, memory_order_relaxed);
    uint32_t tail = atomic_load_explicit(&ring->tail, memory_order_acquire);
    if (head - tail < OSP_PROF_RING_ENTRIES) {
      RingEntry *e = &ring->entries[head % OSP_PROF_RING_ENTRIES];
      ucontext_t *uc = uctx;
#if defined(__aarch64__)
      uint64_t pc = uc->uc_mcontext.pc;
      uint64_t fp = uc->uc_mcontext.regs[29];
      uint64_t lr = uc->uc_mcontext.regs[30];
#else
      uint64_t pc = (uint64_t)uc->uc_mcontext.gregs[REG_RIP];
      uint64_t fp = (uint64_t)uc->uc_mcontext.gregs[REG_RBP];
      uint64_t lr = 0;
#endif
      e->t_ns = osp_prof_mono_ns();
      e->row = slot->index;
      int n = osp_prof_walk(pc, fp, lr, slot->stack_lo, slot->stack_hi, e->pcs,
                            OSP_PROF_MAX_FRAMES);
      e->n = n > 0 ? (uint32_t)n : 0;
      atomic_store_explicit(&ring->head, head + 1, memory_order_release);
    } else {
      atomic_fetch_add_explicit(&ring->drops, 1, memory_order_relaxed);
    }
  }
  errno = saved_errno;
}

// Drain the entries the last signals produced. `state` is the classification
// the sampler had decided when it sent those signals. Runs under the registry
// lock (registry -> malloc order matches append_row).
static void drain_ring(OspProfSlot *slot, uint8_t state) {
  Ring *ring = slot->ring;
  uint32_t head = atomic_load_explicit(&ring->head, memory_order_acquire);
  uint32_t tail = atomic_load_explicit(&ring->tail, memory_order_relaxed);
  while (tail != head) {
    RingEntry *e = &ring->entries[tail % OSP_PROF_RING_ENTRIES];
    if (e->n > 0) {
      uint32_t stack =
          osp_prof_record_sample(e->t_ns, e->row, e->pcs, e->n, state);
      if (e->row == slot->index) { // never latch a previous occupant's stack
        remember_stack(slot, stack);
      }
    }
    tail++;
  }
  atomic_store_explicit(&ring->tail, tail, memory_order_release);
  uint64_t drops = atomic_exchange_explicit(&ring->drops, 0, memory_order_relaxed);
  for (; drops > 0; drops--) {
    osp_prof_note_drop();
  }
}

static void sample_thread(const OspProfSnap *snap) {
  osp_prof_registry_lock();
  if (!osp_prof_snap_live(snap)) {
    osp_prof_registry_unlock();
    return;
  }
  OspProfSlot *slot = snap->slot;
  if (!slot->ring) {
    slot->ring = calloc(1, sizeof(Ring));
  }
  if (!slot->ring) {
    osp_prof_registry_unlock();
    return;
  }
  drain_ring(slot, slot->signal_state);
  uint64_t now = osp_prof_mono_ns();
  uint8_t state = classify_tick(slot, now, pthread_cpu_ns(slot->pthread));
  if (state == OSP_PROF_STATE_ONCPU) {
    slot->signal_state = OSP_PROF_STATE_ONCPU;
    pthread_kill(slot->pthread, SIGPROF);
  } else if (!slot->has_last_stack) {
    // One signal to capture the blocking stack; afterwards waiting samples
    // reuse it without perturbing the blocked syscall again.
    slot->signal_state = OSP_PROF_STATE_WAITING;
    pthread_kill(slot->pthread, SIGPROF);
  } else {
    osp_prof_record_repeat(now, slot->index, slot->last_stack,
                           OSP_PROF_STATE_WAITING);
  }
  osp_prof_registry_unlock();
}

static bool platform_setup(void) {
  struct sigaction sa;
  memset(&sa, 0, sizeof(sa));
  sa.sa_sigaction = sigprof_handler;
  sa.sa_flags = SA_SIGINFO | SA_RESTART;
  sigemptyset(&sa.sa_mask);
  return sigaction(SIGPROF, &sa, NULL) == 0;
}

static void platform_teardown(void) {
  // Drain whatever the last signals produced before the final dump.
  OspProfSnap snaps[OSP_PROF_MAX_THREADS];
  int n = osp_prof_snapshot(snaps, OSP_PROF_MAX_THREADS);
  for (int i = 0; i < n; i++) {
    osp_prof_registry_lock();
    if (osp_prof_snap_live(&snaps[i]) && snaps[i].slot->ring) {
      drain_ring(snaps[i].slot, snaps[i].slot->signal_state);
    }
    osp_prof_registry_unlock();
  }
}

#endif // platform split

// ---- sampler thread lifecycle ------------------------------------------------

static void sampler_tick(void) {
  OspProfSnap snaps[OSP_PROF_MAX_THREADS];
  int n = osp_prof_snapshot(snaps, OSP_PROF_MAX_THREADS);
  for (int i = 0; i < n; i++) {
    sample_thread(&snaps[i]);
  }
}

static void *sampler_main(void *arg) {
  (void)arg;
#if defined(__APPLE__)
  // Keep the sampler off low-latency-hostile scheduling tiers so ticks land
  // on time; the sampled program's threads are untouched.
  pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0);
#endif
  uint64_t deadline = osp_prof_mono_ns();
  while (!atomic_load_explicit(&g_sampler_stop, memory_order_relaxed)) {
    wait_until_next(&deadline);
    sampler_tick();
  }
  platform_teardown();
  return NULL;
}

bool osp_prof_sampler_start(void) {
  if (!platform_setup()) {
    return false;
  }
  g_rng_state ^= osp_prof_mono_ns() | 1ULL;
  atomic_store(&g_sampler_stop, false);
  return pthread_create(&g_sampler_thread, NULL, sampler_main, NULL) == 0;
}

void osp_prof_sampler_stop(void) {
  atomic_store(&g_sampler_stop, true);
  pthread_join(g_sampler_thread, NULL);
}

#else

typedef int osp_prof_sampler_unused; // profiling is POSIX-only

#endif // !_WIN32 && !__wasm__
