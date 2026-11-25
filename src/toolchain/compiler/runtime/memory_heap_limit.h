// The managed heap's ceiling [MEM-HEAP-CEILING]
// (docs/specs/compiler/0018-MemoryManagement.md), shared by the ARC and GC
// backends; each includes this once and holds its own heap lock around it.
//
// Neither backend had a bound: the tracing budget is max(min-heap, 2 x live)
// and ARC holds whatever is live, so a program that retains without end grew
// until the operating system killed unrelated processes (86.6 GB in one
// recorded run). The ceiling is OSPREY_HEAP_MAX_MB, or half the machine's
// physical memory when that is unset.
#ifndef OSPREY_MEMORY_HEAP_LIMIT_H
#define OSPREY_MEMORY_HEAP_LIMIT_H

#include "memory_refused.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#ifndef _WIN32
#include <unistd.h>
#endif

#define OSP_HEAP_MB_SHIFT 20

static size_t g_heap_limit = SIZE_MAX; // no ceiling until heap_limit_arm reads it
static size_t g_heap_peak = 0;         // the most bytes the managed heap has held

// Half of physical memory, or no ceiling where the host does not report it.
static size_t heap_limit_default(void) {
#if defined(_SC_PHYS_PAGES) && defined(_SC_PAGESIZE) && !defined(OSP_SYMBOLIC)
  long pages = sysconf(_SC_PHYS_PAGES);
  long page = sysconf(_SC_PAGESIZE);
  if (pages > 0 && page > 0 && (uint64_t)pages <= SIZE_MAX / (uint64_t)page) {
    return (size_t)pages * (size_t)page / 2;
  }
#endif
  return SIZE_MAX;
}

// Read the knob. Each backend calls this once, before its first allocation.
static void heap_limit_arm(void) {
  const char *mb = getenv("OSPREY_HEAP_MAX_MB");
  long long wanted = mb ? strtoll(mb, NULL, 10) : 0;
  g_heap_limit = wanted > 0 && (unsigned long long)wanted <= (SIZE_MAX >> OSP_HEAP_MB_SHIFT)
                     ? (size_t)wanted << OSP_HEAP_MB_SHIFT
                     : heap_limit_default();
}

static inline void heap_note_peak(size_t held) {
  if (held > g_heap_peak) {
    g_heap_peak = held;
  }
}

// Whether `request` more bytes would take a heap holding `held` past the ceiling.
// A symbolic run is bounded by its engine, and a symbolic size must not turn
// this guard into a path of its own.
static inline int heap_over_limit(size_t held, size_t request) {
#ifdef OSP_SYMBOLIC
  (void)held;
  (void)request;
  return 0;
#else
  return held >= g_heap_limit || request > g_heap_limit - held;
#endif
}

// A request no heap under the ceiling could hold. It is refused with NULL, like
// any size the system cannot serve, and the caller reports its own typed error.
static inline int heap_never_fits(size_t request) { return request > g_heap_limit; }

// Broken-code quarantine [FAILURE-EXPLICIT]: the heap is full of blocks that are
// still in use. Compiled code allocates at sites that have no failure branch,
// so a refusal there would be a store through NULL, and letting the heap grow
// is what takes the machine down. The run ends here instead, naming the limit
// and the peak; stdout is flushed first so what the program printed survives.
static _Noreturn void heap_exhausted(const char *backend, size_t held, size_t request) {
  heap_note_peak(held);
  fprintf(stderr, "osprey: heap limit exceeded: backend=%s limit_bytes=%zu peak_bytes=%zu request_bytes=%zu\n",
          backend, g_heap_limit, g_heap_peak, request);
  osp_memory_refusal_stop();
}

#endif
