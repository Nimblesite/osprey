// KLEE roots supplied by the LLVM root pass [SYMEX-GC-ROOTS].
// Keep the real collector: these ranges replace host stack/register discovery.
#ifndef OSPREY_GC_SYMBOLIC_ROOTS_H
#define OSPREY_GC_SYMBOLIC_ROOTS_H
#include "klee/hunt_mock.h"

typedef struct {
  uintptr_t address;
  size_t bytes;
} OspGcRootRange;

typedef struct OspGcRootFrame {
  struct OspGcRootFrame *previous;
  size_t count;
  OspGcRootRange *ranges;
} OspGcRootFrame;

typedef struct OspGcGlobalRootTable {
  struct OspGcGlobalRootTable *next;
  size_t count;
  OspGcRootRange *ranges;
} OspGcGlobalRootTable;

extern OspGcRootFrame *osp_gc_root_frames;
extern OspGcGlobalRootTable *osp_gc_global_root_tables;
OspGcRootFrame *osp_gc_root_frames;
OspGcGlobalRootTable *osp_gc_global_root_tables;

static void gc_scan_root_ranges(const OspGcRootRange *ranges, size_t count) {
  for (size_t i = 0; i < count; i++) {
    if (ranges[i].address != 0 && ranges[i].bytes != 0) {
      gc_scan_range(ranges[i].address, ranges[i].address + ranges[i].bytes);
    }
  }
}

static void gc_scan_roots(void) {
  if (!osp_gc_root_frames && !osp_gc_global_root_tables) {
    // Scanning no roots would free live objects. Missing instrumentation is fatal.
    osp_hunt_unsupported(__FILE__, __LINE__, "GC root instrumentation missing");
  }
  for (OspGcRootFrame *frame = osp_gc_root_frames; frame; frame = frame->previous) {
    gc_scan_root_ranges(frame->ranges, frame->count);
  }
  for (OspGcGlobalRootTable *table = osp_gc_global_root_tables; table; table = table->next) {
    gc_scan_root_ranges(table->ranges, table->count);
  }
}
#endif
