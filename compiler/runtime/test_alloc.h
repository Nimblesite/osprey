// Deterministic allocation failure, for the out-of-memory arms that are
// otherwise unreachable and therefore permanently uncovered.
//
// Every `if (buf == NULL) return NULL;` in this runtime is a real contract: the
// caller gets a null string rather than a half-built one. But nothing a test can
// ask for makes a 128-byte `malloc` fail — `setrlimit(RLIMIT_AS)` is not
// enforced on Darwin, and exhausting an overcommitting kernel for real is not a
// test — so those arms measured as dead code and the gcov gate could not tell
// them from code nobody bothered to exercise.
//
// Defining `malloc`/`realloc`/`free` in a test translation unit replaces them
// for EVERY object in that binary at static-link time, on both Mach-O and ELF.
// Disarmed — which is the default, and the state every existing test runs in —
// each one forwards to the real allocator, so a suite that never arms the
// injector behaves exactly as it did before.
//
// `dlsym` is how the real allocator is found, and on some libcs it allocates the
// first time it runs — inside the very wrapper trying to resolve it. The
// bootstrap arena breaks that cycle: a few bytes, served once, never reclaimed.
//
// ARM FROM ONE THREAD AT A TIME. The counters below are atomic, so a threaded
// binary counts blocks correctly and this header is safe to link into one; what
// is NOT a test is arming a failure while several threads are allocating, since
// the NULL then lands on whichever of them asked first. Arm in a window where
// one thread is running, and read `osp_alloc_live()` in the same window.
#ifndef OSPREY_TEST_ALLOC_H
#define OSPREY_TEST_ALLOC_H

#include <dlfcn.h>
#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

// Successes remaining before the next allocation fails. Negative is disarmed,
// which is why it cannot simply be a bool: the arm under test is often the
// SECOND allocation in a function, and failing the first would never reach it.
static _Atomic long osp_alloc_successes_left = -1;

#define OSP_BOOTSTRAP_BYTES 4096
static char osp_bootstrap_arena[OSP_BOOTSTRAP_BYTES];
static _Atomic size_t osp_bootstrap_used;

// Blocks handed out and not yet returned. An OOM arm's real contract is not
// just "returns NULL" but "owns nothing on the way out": a growth failure that
// forgets to free the buffer it was growing leaks on every failed read, and
// nothing about the return value shows it.
static _Atomic long osp_alloc_live_blocks;

static void *(*osp_real_malloc)(size_t);
static void *(*osp_real_calloc)(size_t, size_t);
static void *(*osp_real_realloc)(void *, size_t);
static void (*osp_real_free)(void *);
static int osp_alloc_resolving;

// Relational comparison of pointers into DIFFERENT objects is undefined in C,
// and every pointer this is asked about is either the arena's or the real
// allocator's. Converting both sides to uintptr_t asks the same question about
// integers, which is defined.
static inline int osp_from_bootstrap(const void *p) {
  uintptr_t at = (uintptr_t)p;
  uintptr_t base = (uintptr_t)(const void *)osp_bootstrap_arena;
  return at >= base && at < base + OSP_BOOTSTRAP_BYTES;
}

// Eight-byte aligned bump allocation. Returns NULL once the arena is spent,
// which is itself the honest answer: nothing here may grow without bound.
static inline void *osp_bootstrap_alloc(size_t bytes) {
  // Reject before rounding: `bytes + 7` wraps to a SMALL number for a request
  // near SIZE_MAX, and the rounded size would then pass the bounds check.
  if (bytes > OSP_BOOTSTRAP_BYTES) {
    return NULL;
  }
  // A zero-byte request costs a slot rather than nothing. Reserving nothing
  // makes the reservation a no-op: against a SPENT arena `0 <= 0` holds and the
  // caller is handed one-past-the-end, and against a live one two callers are
  // handed the SAME address.
  size_t aligned = bytes == 0 ? 8u : ((bytes + 7u) & ~(size_t)7u);
  // Reserve by compare-exchange, never by fetch-add. Adding first pushes
  // `osp_bootstrap_used` PAST the arena once it is spent, and the bounds check
  // then computes `OSP_BOOTSTRAP_BYTES - taken` in unsigned arithmetic: it
  // underflows to an enormous number, the check passes, and the caller — the
  // dlsym recursion this arena exists to serve — is handed a pointer beyond
  // the arena's end.
  size_t taken = atomic_load(&osp_bootstrap_used);
  while (aligned <= OSP_BOOTSTRAP_BYTES - taken) {
    if (atomic_compare_exchange_weak(&osp_bootstrap_used, &taken,
                                     taken + aligned)) {
      return osp_bootstrap_arena + taken;
    }
  }
  return NULL; // spent, and it never grows
}

static inline void osp_resolve_real_allocators(void) {
  if (osp_real_malloc != NULL || osp_alloc_resolving) {
    return;
  }
  osp_alloc_resolving = 1;
  osp_real_malloc = (void *(*)(size_t))dlsym(RTLD_NEXT, "malloc");
  osp_real_calloc = (void *(*)(size_t, size_t))dlsym(RTLD_NEXT, "calloc");
  osp_real_realloc = (void *(*)(void *, size_t))dlsym(RTLD_NEXT, "realloc");
  osp_real_free = (void (*)(void *))dlsym(RTLD_NEXT, "free");
  osp_alloc_resolving = 0;
}

// True exactly once per armed allocation, counting down the successes first.
static inline int osp_alloc_should_fail(void) {
  long left = atomic_load(&osp_alloc_successes_left);
  while (left >= 0) {
    // One armed failure is handed to exactly one caller: whoever wins the
    // exchange takes it, everyone else sees the disarmed value and succeeds.
    long next = left > 0 ? left - 1 : -1;
    if (atomic_compare_exchange_weak(&osp_alloc_successes_left, &left, next)) {
      return left == 0; // ...and `left == 0` is the one that was armed for
    }
  }
  return 0;
}

/// Blocks currently outstanding through this injector.
static inline long osp_alloc_live(void) {
  return atomic_load(&osp_alloc_live_blocks);
}

/// Arm the injector: the allocation `successes` after this one returns NULL.
/// `osp_alloc_fail_next()` is `osp_alloc_fail_after(0)`.
static inline void osp_alloc_fail_after(long successes) {
  atomic_store(&osp_alloc_successes_left, successes);
}
static inline void osp_alloc_fail_next(void) { osp_alloc_fail_after(0); }
static inline void osp_alloc_fail_off(void) {
  atomic_store(&osp_alloc_successes_left, -1);
}

void *malloc(size_t bytes) {
  osp_resolve_real_allocators();
  if (osp_real_malloc == NULL) {
    return osp_bootstrap_alloc(bytes);
  }
  if (osp_alloc_should_fail()) {
    return NULL;
  }
  void *at = osp_real_malloc(bytes);
  if (at != NULL) {
    atomic_fetch_add(&osp_alloc_live_blocks, 1);
  }
  return at;
}

// calloc is not malloc-plus-memset to the injector: a unit that builds every
// node with calloc (json_runtime.c does) would otherwise have EVERY
// allocation-failure arm unreachable, and every node it frees would count as a
// block nobody allocated — a phantom leak in any test that counts them.
void *calloc(size_t count, size_t size) {
  osp_resolve_real_allocators();
  size_t bytes = count * size;
  if (size != 0 && count > (size_t)-1 / size) {
    return NULL; // the product overflowed; no allocator can honour it
  }
  if (osp_real_calloc == NULL) {
    void *at = osp_bootstrap_alloc(bytes);
    return at == NULL ? NULL : memset(at, 0, bytes);
  }
  if (osp_alloc_should_fail()) {
    return NULL;
  }
  void *at = osp_real_calloc(count, size);
  if (at != NULL) {
    atomic_fetch_add(&osp_alloc_live_blocks, 1);
  }
  return at;
}

void *realloc(void *at, size_t bytes) {
  osp_resolve_real_allocators();
  if (osp_alloc_should_fail()) {
    return NULL; // the caller still owns `at`, exactly as C requires
  }
  if (osp_from_bootstrap(at)) {
    // The arena records no block sizes, so the copy is bounded by what is
    // certainly readable: the bytes between `at` and the end of the arena.
    size_t readable = (size_t)(osp_bootstrap_arena + OSP_BOOTSTRAP_BYTES - (char *)at);
    void *grown = malloc(bytes);
    return grown == NULL ? NULL : memcpy(grown, at, bytes < readable ? bytes : readable);
  }
  if (osp_real_realloc == NULL) {
    return NULL;
  }
  void *grown = osp_real_realloc(at, bytes);
  if (grown != NULL && at == NULL) {
    atomic_fetch_add(&osp_alloc_live_blocks, 1); // realloc(NULL, n) is a malloc
  }
  return grown;
}

// strdup allocates too, and libc's own copy of it does NOT route through the
// malloc defined here: a unit that duplicates strings (json_runtime.c returns
// every scalar that way) would have an uncountable allocation and an
// injectable-looking arm that never fails. Defining it puts both back under
// this file's control.
char *strdup(const char *text) {
  size_t bytes = strlen(text) + 1;
  char *copy = (char *)malloc(bytes);
  return copy == NULL ? NULL : (char *)memcpy(copy, text, bytes);
}

void free(void *at) {
  if (at == NULL || osp_from_bootstrap(at)) {
    return; // arena bytes outlive the process; freeing them would be a wild free
  }
  osp_resolve_real_allocators();
  if (osp_real_free != NULL) {
    atomic_fetch_sub(&osp_alloc_live_blocks, 1);
    osp_real_free(at);
  }
}

#endif // OSPREY_TEST_ALLOC_H
