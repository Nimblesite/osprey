// Heap-allocation redirect for the ARC backend. `-include`d (compiler flag)
// ahead of a runtime unit's own headers so its malloc/calloc/realloc/free/
// strdup route into the ARC allocator (memory_arc.c) — giving runtime-minted
// strings and container nodes the 16-byte Perceus header and a registry entry,
// so codegen-emitted osp_release calls on them are precise instead of foreign
// probe-misses. Implements the shimmed-unit half of [GC-ARC-PERCEUS]
// (docs/plans/0011, phase 2 milestone M1).
//
// Only the value-producing units (list/map/hamt/string/string-list/json) are
// built with this; the default and gc archives never see it.
#ifndef OSP_ARC_SHIM_H
#define OSP_ARC_SHIM_H

// Pull in the real prototypes first, THEN shadow the names. Because this header
// is force-included before the unit's own `#include <stdlib.h>`/`<string.h>`,
// the later includes are no-ops (guarded) and never re-declare through the
// macros.
#include <stdlib.h>
#include <string.h>

void *osp_arc_malloc(size_t size);
void *osp_arc_calloc(size_t n, size_t size);
void *osp_arc_realloc(void *old, size_t size);
void osp_arc_free(void *p);
char *osp_arc_strdup(const char *s);

#define malloc(s) osp_arc_malloc(s)
#define calloc(n, s) osp_arc_calloc((n), (s))
#define realloc(p, s) osp_arc_realloc((p), (s))
#define free(p) osp_arc_free(p)
#define strdup(s) osp_arc_strdup(s)

#endif // OSP_ARC_SHIM_H
