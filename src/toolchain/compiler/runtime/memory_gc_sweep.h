// The sweep of the tracing collector, included once by memory_gc.c after the
// heap table and the pages, whose static state it shares [GC-TRACE-CONSERVATIVE].
#ifndef OSPREY_GC_SWEEP_H
#define OSPREY_GC_SWEEP_H

// Finalizers see the whole heap: one may ask the collector about a block that
// outlives this collection, so they all run before any block is freed or reused.
static void gc_finalize_dead(void) {
  for (size_t i = 0; i < g_cap; i++) {
    uint32_t mark = g_tab[i].meta.mark;
    if (g_tab[i].base != 0 && (mark & (GC_FINAL | GC_MARKED)) == GC_FINAL) {
      gc_finalize_block(mark, (void *)g_tab[i].base);
    }
  }
  gc_pages_finalize_dead();
}

// Free every unmarked table block and empty its slot. Survivors keep GC_MARKED,
// which from here on reads "not yet reseated". Sets the counts and g_bytes to
// the table's live set; the pages add theirs.
static void gc_free_dead(void) {
  g_tab_count = 0;
  g_bytes = 0;
  for (size_t i = 0; i < g_cap; i++) {
    if (g_tab[i].base == 0) {
      continue;
    }
    if (g_tab[i].meta.mark & GC_MARKED) {
      g_tab_count++;
      g_bytes += g_tab[i].meta.size;
    } else {
      free((void *)g_tab[i].base);
      g_tab[i].base = 0;
    }
  }
  g_count = g_tab_count;
}

// Lift the survivor out of slot `i` and seat it in the first slot of its probe
// run that holds no seated block. A survivor still waiting there is displaced
// and seated in turn, so the chain ends at an empty slot. A seated block never
// moves again and no probe run crosses a waiting one, so every run a lookup
// walks stays unbroken.
static void gc_reseat(size_t i) {
  OspGcEnt e = g_tab[i];
  g_tab[i].base = 0;
  while (e.base != 0) {
    e.meta.mark &= GC_PERSISTENT;
    size_t j = gc_hash(e.base, g_cap);
    while (g_tab[j].base != 0 && !(g_tab[j].meta.mark & GC_MARKED)) {
      j = (j + 1) & (g_cap - 1);
    }
    OspGcEnt waiting = g_tab[j];
    g_tab[j] = e;
    e = waiting;
  }
}

// Sweep the heap where it stands: finalize and free the unmarked, reseat the
// table's survivors, sweep the pages, and re-arm the adaptive budget at
// max(min-heap, 2× live). The table keeps the capacity this cycle's blocks
// needed, because a steady workload fills it again before the next collection;
// a fresh table per collection cost a page-faulted allocation and a rehash at
// every doubling on the way back up. It is cut back only once a whole cycle has
// peaked below it [GC-TRACE-CONSERVATIVE].
static void gc_sweep(void) {
  size_t fit = gc_fit(g_tab_count);
  gc_finalize_dead();
  gc_free_dead();
  for (size_t i = 0; i < g_cap; i++) {
    if (g_tab[i].base != 0 && (g_tab[i].meta.mark & GC_MARKED)) {
      gc_reseat(i);
    }
  }
  if (fit < g_cap) {
    (void)gc_resize(fit); // out of memory: the oversized table still serves
  }
  gc_pages_sweep();
  g_thresh = g_bytes * 2 > g_min_heap ? g_bytes * 2 : g_min_heap;
  g_scanned_since = 0;
  g_ncollect++; // always: read by osp_gc_collections() diagnostics, not just debug
  if (g_debug) {
    // env-gated diagnostic only — not on any hot path.
    fprintf(stderr, "[osp-gc] collect #%zu: %zu live objs, %zu live KiB\n",
            g_ncollect, g_count, g_bytes / 1024);
  }
}

// Pinned blocks are roots wherever they are referenced from: a runtime
// singleton held only by a global stays alive even on a host whose data
// segment this collector cannot scan.
static void gc_scan_pinned(void) {
  for (size_t i = 0; i < g_cap; i++) {
    if (g_tab[i].base != 0 && (g_tab[i].meta.mark & (GC_PINNED | GC_MARKED)) == GC_PINNED) {
      g_tab[i].meta.mark |= GC_MARKED;
      gc_ms_push(g_tab[i].base);
    }
  }
  gc_pages_scan_pinned();
}

// Scan every marked table block and page cell again: the recovery pass of
// gc_trace for entries the mark stack dropped.
static void gc_rescan_marked(void) {
  for (size_t i = 0; i < g_cap; i++) {
    const OspGcMeta *meta = &g_tab[i].meta;
    if (g_tab[i].base != 0 && (meta->mark & (GC_MARKED | GC_LEAF)) == GC_MARKED) {
      gc_scan_range(g_tab[i].base, g_tab[i].base + meta->size);
    }
  }
  gc_pages_rescan_marked();
}

#endif
