// A refused allocation where the caller has no failure branch [FAILURE-EXPLICIT]
// (docs/specs/compiler/0018-MemoryManagement.md). Shared by the heap ceiling
// and the container runtimes, so every such stop has one status and one shape.
#ifndef OSPREY_MEMORY_REFUSED_H
#define OSPREY_MEMORY_REFUSED_H

#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>

// Exit status of a program stopped for memory (EX_OSERR in sysexits.h).
#define OSP_HEAP_EXHAUSTED_STATUS 71

// End an allocation refusal through the native status or the symbolic
// runtime's modeled termination [FAILURE-EXPLICIT].
_Noreturn static inline void osp_memory_refusal_stop(void) {
  (void)fflush(NULL);
#ifdef OSP_SYMBOLIC
  abort();
#else
  _Exit(OSP_HEAP_EXHAUSTED_STATUS);
#endif
}

// Broken-code quarantine [FAILURE-EXPLICIT]: builtins such as listSort return a
// value, never a Result, so compiled code has no branch to take when the heap
// refuses one of their blocks. Writing through the NULL would crash with no
// cause; the run ends here instead, naming the builtin and the request, with
// stdout flushed first so what the program printed survives (#409).
_Noreturn static inline void osp_alloc_refused(const char *site, size_t request) {
  fprintf(stderr, "osprey: allocation refused: %s request_bytes=%zu\n", site, request);
  osp_memory_refusal_stop();
}

// `block`, or the quarantine stop above when the allocator refused it.
static inline void *osp_alloc_checked(void *block, const char *site, size_t request) {
  if (block == NULL) osp_alloc_refused(site, request);
  return block;
}

#endif
