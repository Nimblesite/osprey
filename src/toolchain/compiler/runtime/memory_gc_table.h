// The managed heap's open-addressing table and the mark stack, included once by
// memory_gc.c after its globals, whose static state it shares [GC-TRACE-CONSERVATIVE].
// A word is a pointer iff it is the base of a tracked block; marking pushes
// bases and scanning a range pushes every word that names one.
#ifndef OSPREY_GC_TABLE_H
#define OSPREY_GC_TABLE_H

// --- heap table --------------------------------------------------------------

static size_t gc_hash(uintptr_t base, size_t cap) {
  // base is at least 8-byte aligned; drop the dead low bits, then the full murmur3
  // finalizer. glibc hands out long runs at a few strides, and a single multiply
  // turns some of those, depending on where ASLR put the heap, into runs of adjacent
  // slots: a miss walked ~290 slots at the table's load rather than ~7.
  uint64_t h = (uint64_t)(base >> 3);
  h = (h ^ (h >> 33)) * 0xFF51AFD7ED558CCDull;
  h = (h ^ (h >> 33)) * 0xC4CEB9FE1A85EC53ull;
  return (size_t)(h ^ (h >> 33)) & (cap - 1);
}

static OspGcPage *gc_page_of(uintptr_t w);
static OspGcMeta *gc_page_cell(OspGcPage *page, uintptr_t w);

// The state of the block whose first byte is `w`: a cell of a page, or a block of
// its own in the table. NULL for every other word.
static OspGcMeta *gc_lookup(uintptr_t w) {
  // Most scanned words are zeros, small integers and floats. The bounds turn them
  // away before the table, where every zero would walk whatever cluster sits at its
  // slot (2,600 slots in a Signal Flow suite on Linux, for every zero on the heap).
  if (w < g_lowest || w > g_highest) {
    return NULL;
  }
  OspGcPage *page = gc_page_of(w);
  if (page) {
    return gc_page_cell(page, w);
  }
  for (size_t i = gc_hash(w, g_cap); g_cap != 0 && g_tab[i].base != 0; i = (i + 1) & (g_cap - 1)) {
    if (g_tab[i].base == w) {
      return &g_tab[i].meta;
    }
  }
  return NULL;
}

// The smallest table that holds `count` objects under the 3/4 load ceiling.
static size_t gc_fit(size_t count) {
  size_t cap = OSP_GC_INIT_TABLE_CAP;
  while (cap * 3 <= (count + 1) * 4) {
    cap *= 2;
  }
  return cap;
}

// Place (base,size) into `tab` (no growth, no duplicates) — used by both the
// live insert path and a resize.
static void gc_place(OspGcEnt *tab, size_t cap, uintptr_t base, size_t size, uint32_t flags) {
  size_t i = gc_hash(base, cap);
  while (tab[i].base != 0) {
    i = (i + 1) & (cap - 1);
  }
  tab[i].base = base;
  tab[i].meta = (OspGcMeta){.size = (uint32_t)size, .mark = flags};
}

// Move every tracked block into a fresh table of `cap` slots, dropping the
// collection marks. Returns 0, leaving the table as it was, when out of memory.
static int gc_resize(size_t cap) {
  OspGcEnt *tab = (OspGcEnt *)calloc(cap, sizeof(OspGcEnt));
  if (!tab) {
    return 0;
  }
  for (size_t i = 0; i < g_cap; i++) {
    if (g_tab[i].base != 0) {
      gc_place(tab, cap, g_tab[i].base, g_tab[i].meta.size, g_tab[i].meta.mark & GC_PERSISTENT);
    }
  }
  free(g_tab);
  g_tab = tab;
  g_cap = cap;
  return 1;
}

// Track a fresh block under its lifetime `flags`. Returns 0 when the table
// cannot grow: the block is then untracked, and never freed.
static int gc_insert(uintptr_t base, size_t size, uint32_t flags) {
  if ((g_cap == 0 || (g_tab_count + 1) * 4 >= g_cap * 3) &&
      !gc_resize(g_cap ? g_cap * 2 : OSP_GC_INIT_TABLE_CAP)) {
    return 0;
  }
  gc_place(g_tab, g_cap, base, size, flags);
  g_tab_count++;
  g_count++;
  g_lowest = base < g_lowest ? base : g_lowest;
  g_highest = base > g_highest ? base : g_highest;
  return 1;
}

// --- mark --------------------------------------------------------------------

static void gc_ms_push(uintptr_t w) {
  if (g_ms_top == g_ms_cap) {
    size_t cap = g_ms_cap ? g_ms_cap * 2 : OSP_GC_INIT_MARK_STACK;
    uintptr_t *ms = (uintptr_t *)realloc(g_ms, cap * sizeof(uintptr_t));
    if (!ms) {
      return; // drop the candidate; conservative scanning will re-find it later
    }
    g_ms = ms;
    g_ms_cap = cap;
  }
  g_ms[g_ms_top++] = w;
}

// A word that may alias any object, so the scan reads it without aliasing UB.
typedef uintptr_t __attribute__((may_alias)) OspGcWord;

// Treat every aligned word in [lo,hi) as a candidate pointer; mark & enqueue any
// that names a managed object. A conservative scan reads whole stacks, including
// the redzones AddressSanitizer plants between locals, so this one function is
// exempt from it and loads the word directly (a memcpy would be intercepted).
__attribute__((no_sanitize("address")))
static void gc_scan_range(uintptr_t lo, uintptr_t hi) {
  uintptr_t p = (lo + (OSP_GC_WORD - 1)) & ~(OSP_GC_WORD - 1);
  for (; p + OSP_GC_WORD <= hi; p += OSP_GC_WORD) {
    uintptr_t w = *(const OspGcWord *)p;
    OspGcMeta *e = gc_lookup(w);
    if (e && !(e->mark & GC_MARKED)) {
      e->mark |= GC_MARKED;
      gc_ms_push(w);
    }
  }
}

#endif
