// Heap-allocation redirect for the GC backend. `-include`d (compiler flag) ahead
// of a runtime unit's own headers so its malloc/calloc/realloc/free route into
// the Osprey collector's managed heap (memory_gc.c) — making the boxed Osprey
// values those units store conservatively reachable. Implements the managed
// value-container half of [GC-TRACE-CONSERVATIVE]
// (docs/specs/0018-MemoryManagement.md).
//
// Only the value-container units (list/map) are built with this; the default
// archive never sees it, so the default backend is byte-for-byte unchanged.
#ifndef OSP_GC_SHIM_H
#define OSP_GC_SHIM_H

// Pull in the real prototypes first, THEN shadow the names. Because this header
// is force-included before the unit's own `#include <stdlib.h>`, the later
// include is a no-op (guarded) and never re-declares through the macros.
#include <stdlib.h>

void *osp_gc_malloc(size_t size);
void *osp_gc_calloc(size_t n, size_t size);
void *osp_gc_realloc(void *old, size_t size);
void osp_gc_free(void *p);

#define malloc(s) osp_gc_malloc(s)
#define calloc(n, s) osp_gc_calloc((n), (s))
#define realloc(p, s) osp_gc_realloc((p), (s))
#define free(p) osp_gc_free(p)

#endif // OSP_GC_SHIM_H
