// Host root sources for the GC backend. Included after gc_scan_range.
#ifndef OSPREY_GC_NATIVE_ROOTS_H
#define OSPREY_GC_NATIVE_ROOTS_H
#include <setjmp.h>

static void gc_scan_data(void) {
#if defined(__APPLE__)
  const struct mach_header_64 *mh =
      (const struct mach_header_64 *)_dyld_get_image_header(0);
  if (!mh) {
    return;
  }
  static const char *const sects[] = {"__data", "__bss", "__common"};
  for (size_t i = 0; i < sizeof(sects) / sizeof(sects[0]); i++) {
    unsigned long sz = 0;
    uint8_t *p = getsectiondata(mh, "__DATA", sects[i], &sz);
    if (p && sz) {
      gc_scan_range((uintptr_t)p, (uintptr_t)p + sz);
    }
  }
#elif defined(__linux__) && defined(__GLIBC__)
  // Linker-provided bounds of the executable's initialised data + BSS.
  extern char __data_start[];
  extern char _end[];
  gc_scan_range((uintptr_t)__data_start, (uintptr_t)_end);
#endif
}

// A stack address sampled before `main` runs, so it sits within a few frames of
// the true stack top. It is the fallback bound for platforms with no exact
// query; a constructor is the only point at which "address of a local" is a
// defensible answer.
static uintptr_t g_early_sp;

__attribute__((constructor)) static void gc_capture_early_sp(void) {
  uintptr_t here = 0;
  g_early_sp = (uintptr_t)&here;
}

// The highest address of the initial thread's stack. Everything from the
// collector's own frame up to this bound is scanned conservatively, so a base
// that is too LOW silently hides every root held by a caller — the collector
// then frees a still-live block and the next allocation overwrites it.
//
// The address of a local HERE is therefore never an acceptable answer:
// `gc_init_main` runs on the FIRST allocation, which is typically deep inside
// the program, so such a base sits far below the frames that hold the roots and
// the scan range misses every one of them. That is exactly what
// `memory_gc_stack_root_tests.c` pins, and exactly what Windows did — it fell
// through to the generic fallback, and unlike clang, gcc does not diagnose
// returning a local's address once it is cast to an integer, so it built
// silently and mis-collected. Windows now reads the TEB; every other platform
// without an exact query uses the pre-`main` sample instead.
static uintptr_t gc_stack_base(void) {
#if defined(__APPLE__)
  void *sa = pthread_get_stackaddr_np(pthread_self());
  return (uintptr_t)sa;
#elif defined(_WIN32)
  // NT_TIB is the first member of every TEB, and `StackBase` is the stack's
  // high bound. Reading it through NtCurrentTeb() works on every supported
  // Windows version, unlike GetCurrentThreadStackLimits (Windows 8+), whose
  // declaration is additionally gated on the _WIN32_WINNT the toolchain
  // happens to default to.
  const NT_TIB *tib = (const NT_TIB *)NtCurrentTeb();
  return (uintptr_t)tib->StackBase;
#elif defined(__linux__) && defined(__GLIBC__)
  pthread_attr_t a;
  void *addr = NULL;
  size_t sz = 0;
  if (pthread_getattr_np(pthread_self(), &a) == 0) {
    pthread_attr_getstack(&a, &addr, &sz);
    pthread_attr_destroy(&a);
    return (uintptr_t)addr + sz;
  }
  return g_early_sp;
#else
  // No exact query on this platform: use the frame captured before `main`,
  // which is within a few frames of the true top — never the first-allocation
  // frame, which can be arbitrarily deep.
  return g_early_sp;
#endif
}

static void gc_scan_roots(void) {
  jmp_buf regs;
  (void)setjmp(regs);

  volatile uintptr_t probe = 0;
  uintptr_t sp = (uintptr_t)&probe;
  uintptr_t lo = sp < g_base ? sp : g_base;
  uintptr_t hi = sp < g_base ? g_base : sp;
  gc_scan_range(lo, hi);
  gc_scan_range((uintptr_t)&regs, (uintptr_t)&regs + sizeof(regs));
  gc_scan_data();

}
#endif
