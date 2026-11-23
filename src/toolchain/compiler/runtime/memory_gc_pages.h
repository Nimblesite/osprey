// Size-class pages of the tracing collector [MEM-BACKENDS-GC-HEAP]
// (docs/specs/compiler/0018-MemoryManagement.md), included once by memory_gc.c
// after the heap table, whose static state it shares.
//
// A block of at most OSP_GC_SMALL_MAX bytes is a cell of a page: an aligned
// 64 KiB region holding cells of one size, with each cell's size and flags in an
// array at the head of the page, outside the cells. An address finds its page
// by masking, so the trace reaches a cell's state without the heap table, and
// an allocation is a cell taken from a page rather than a libc block entered
// into that table. Larger blocks keep the table.
#ifndef OSPREY_GC_PAGES_H
#define OSPREY_GC_PAGES_H

#define OSP_GC_PAGE_BYTES ((size_t)1u << 16)
#define OSP_GC_PAGE_MASK (~(uintptr_t)(OSP_GC_PAGE_BYTES - 1))
#define OSP_GC_GRAIN ((size_t)16)        // cell sizes step by this; also the cell alignment
#define OSP_GC_SMALL_MAX ((size_t)1024)  // the largest block a page serves
#define OSP_GC_CLASSES (OSP_GC_SMALL_MAX / OSP_GC_GRAIN)
#define OSP_GC_PAGE_SET_INIT ((size_t)64)

typedef struct OspGcPage {
  uint32_t cell;   // bytes per cell
  uint32_t ncells; // cells the page holds
  uint32_t bump;   // cells handed out at least once; the rest were never used
  uint32_t free;   // first dead cell, as its index + 1 (0: none); a dead cell's size is the next
  uint32_t live;   // cells in use
  uint32_t finals; // cells in use that carry a finalizer
  uint32_t pinned; // a cell here is pinned
  struct OspGcPage *next; // the next page of this class with room
  uintptr_t cells;        // address of cell 0
  OspGcMeta meta[];
} OspGcPage;

static OspGcPage **g_pages = NULL; // open-addressing set of every page, keyed by address
static size_t g_pages_cap = 0;
static size_t g_npages = 0;
static OspGcPage *g_room[OSP_GC_CLASSES + 1]; // per class: pages with a cell to give
static int g_pages_off = 0; // OSPREY_GC_PAGES=0: every block is a table block

// --- the page set --------------------------------------------------------------

static void gc_pages_put(OspGcPage **set, size_t cap, OspGcPage *page) {
  size_t i = gc_hash((uintptr_t)page, cap);
  while (set[i] != NULL) {
    i = (i + 1) & (cap - 1);
  }
  set[i] = page;
}

// Keep the set under half full so a miss ends after a slot or two. Returns 0,
// leaving the set as it was, when out of memory.
static int gc_pages_reserve(void) {
  if ((g_npages + 1) * 2 <= g_pages_cap) {
    return 1;
  }
  size_t cap = g_pages_cap ? g_pages_cap * 2 : OSP_GC_PAGE_SET_INIT;
  OspGcPage **set = (OspGcPage **)calloc(cap, sizeof(OspGcPage *));
  if (!set) {
    return 0;
  }
  for (size_t i = 0; i < g_pages_cap; i++) {
    if (g_pages[i] != NULL) {
      gc_pages_put(set, cap, g_pages[i]);
    }
  }
  free(g_pages);
  g_pages = set;
  g_pages_cap = cap;
  return 1;
}

// Take `page` out of the set, closing the gap so every probe run stays unbroken.
static void gc_pages_remove(const OspGcPage *page) {
  size_t mask = g_pages_cap - 1;
  size_t gap = gc_hash((uintptr_t)page, g_pages_cap);
  while (g_pages[gap] != page) {
    gap = (gap + 1) & mask;
  }
  for (size_t j = (gap + 1) & mask; g_pages[j] != NULL; j = (j + 1) & mask) {
    size_t home = gc_hash((uintptr_t)g_pages[j], g_pages_cap);
    if (((j - home) & mask) >= ((j - gap) & mask)) {
      g_pages[gap] = g_pages[j];
      gap = j;
    }
  }
  g_pages[gap] = NULL;
  g_npages--;
}

// The page `w` points into, or NULL.
static OspGcPage *gc_page_of(uintptr_t w) {
  if (g_npages == 0) {
    return NULL;
  }
  uintptr_t key = w & OSP_GC_PAGE_MASK;
  for (size_t i = gc_hash(key, g_pages_cap); g_pages[i] != NULL; i = (i + 1) & (g_pages_cap - 1)) {
    if ((uintptr_t)g_pages[i] == key) {
      return g_pages[i];
    }
  }
  return NULL;
}

// The state of the cell whose first byte is `w`, or NULL when `w` is anything
// else: a byte inside a cell, the page's header, a dead or never-used cell.
static OspGcMeta *gc_page_cell(OspGcPage *page, uintptr_t w) {
  if (w < page->cells) {
    return NULL;
  }
  uintptr_t offset = w - page->cells;
  uintptr_t index = offset / page->cell;
  if (index >= page->bump || index * page->cell != offset) {
    return NULL;
  }
  OspGcMeta *meta = &page->meta[index];
  return (meta->mark & GC_USED) ? meta : NULL;
}

static uintptr_t gc_cell_at(const OspGcPage *page, uint32_t index) {
  return page->cells + (uintptr_t)index * page->cell;
}

// --- allocation ----------------------------------------------------------------

// Aligned storage for one page. A symbolic run keeps every block in the table,
// where the engine tracks each one as an object of its own.
static OspGcPage *gc_page_map(void) {
#if defined(OSP_SYMBOLIC)
  return NULL;
#elif defined(_WIN32)
  return (OspGcPage *)_aligned_malloc(OSP_GC_PAGE_BYTES, OSP_GC_PAGE_BYTES);
#else
  void *page = NULL;
  return posix_memalign(&page, OSP_GC_PAGE_BYTES, OSP_GC_PAGE_BYTES) == 0 ? (OspGcPage *)page : NULL;
#endif
}

static void gc_page_unmap(OspGcPage *page) {
#if defined(_WIN32)
  _aligned_free(page);
#else
  free(page);
#endif
}

// A fresh page of `cell`-byte cells, entered in the set. The header and the
// state array come first, then the cells from the next aligned address.
static OspGcPage *gc_page_new(size_t cell) {
  OspGcPage *page = gc_pages_reserve() ? gc_page_map() : NULL;
  if (!page) {
    return NULL;
  }
  uintptr_t start = (uintptr_t)page;
  size_t count = (OSP_GC_PAGE_BYTES - sizeof(OspGcPage)) / (cell + sizeof(OspGcMeta));
  uintptr_t cells = (start + sizeof(OspGcPage) + count * sizeof(OspGcMeta) + OSP_GC_GRAIN - 1) & ~(OSP_GC_GRAIN - 1);
  count -= cells + count * cell > start + OSP_GC_PAGE_BYTES; // the alignment took at most one cell
  *page = (OspGcPage){.cell = (uint32_t)cell, .ncells = (uint32_t)count, .cells = cells};
  gc_pages_put(g_pages, g_pages_cap, page);
  g_npages++;
  g_lowest = start < g_lowest ? start : g_lowest;
  g_highest = start + OSP_GC_PAGE_BYTES > g_highest ? start + OSP_GC_PAGE_BYTES : g_highest;
  return page;
}

// A cell for `size` bytes under `flags`, or NULL when no page can be had (the
// caller then tracks a block of its own). Dead cells go first, lowest address
// first; then the cells never used.
static void *gc_page_alloc(size_t size, uint32_t flags) {
  size_t bucket = size ? (size + OSP_GC_GRAIN - 1) / OSP_GC_GRAIN : 1;
  OspGcPage *page = g_room[bucket];
  if (!page) {
    page = gc_page_new(bucket * OSP_GC_GRAIN);
    if (!page) {
      return NULL;
    }
    g_room[bucket] = page;
  }
  uint32_t index = page->free ? page->free - 1 : page->bump++;
  if (page->free) {
    page->free = page->meta[index].size;
  }
  if (!page->free && page->bump == page->ncells) {
    g_room[bucket] = page->next;
  }
  page->meta[index] = (OspGcMeta){.size = (uint32_t)size, .mark = flags | GC_USED};
  page->live++;
  page->finals += (flags & GC_FINAL) != 0;
  uintptr_t cell = gc_cell_at(page, index);
  return (void *)cell;
}

// --- collection ----------------------------------------------------------------

// Pinned cells are roots of every collection, like pinned table blocks.
static void gc_pages_scan_pinned(void) {
  for (size_t i = 0; i < g_pages_cap; i++) {
    OspGcPage *page = g_pages[i];
    for (uint32_t c = 0; page && page->pinned && c < page->bump; c++) {
      if ((page->meta[c].mark & (GC_USED | GC_PINNED | GC_MARKED)) == (GC_USED | GC_PINNED)) {
        page->meta[c].mark |= GC_MARKED;
        gc_ms_push(gc_cell_at(page, c));
      }
    }
  }
}

// Scan every marked cell again [GC-TRACE-CONSERVATIVE]: see gc_rescan_marked.
static void gc_pages_rescan_marked(void) {
  for (size_t i = 0; i < g_pages_cap; i++) {
    const OspGcPage *page = g_pages[i];
    for (uint32_t c = 0; page && c < page->bump; c++) {
      if ((page->meta[c].mark & (GC_USED | GC_MARKED | GC_LEAF)) == (GC_USED | GC_MARKED)) {
        gc_scan_range(gc_cell_at(page, c), gc_cell_at(page, c) + page->meta[c].size);
      }
    }
  }
}

// Run the finalizer of every dying cell that carries one, before any cell is
// reused: a finalizer may ask the collector about a block that outlives it.
static void gc_pages_finalize_dead(void (*finalize)(void *body)) {
  for (size_t i = 0; i < g_pages_cap; i++) {
    OspGcPage *page = g_pages[i];
    for (uint32_t c = 0; page && page->finals && c < page->bump; c++) {
      if ((page->meta[c].mark & (GC_USED | GC_FINAL | GC_MARKED)) == (GC_USED | GC_FINAL)) {
        uintptr_t cell = gc_cell_at(page, c);
        finalize((void *)cell);
      }
    }
  }
}

// Unmark the survivors and thread every other cell onto the page's dead list,
// lowest address first. Adds the survivors to g_count and g_bytes.
static void gc_page_sweep(OspGcPage *page) {
  uint32_t dead = 0;
  for (uint32_t c = page->bump; c-- > 0;) {
    OspGcMeta *meta = &page->meta[c];
    if (meta->mark & GC_MARKED) {
      meta->mark &= ~GC_MARKED;
      g_count++;
      g_bytes += meta->size;
      continue;
    }
    if (meta->mark & GC_USED) {
      page->live--;
      page->finals -= (meta->mark & GC_FINAL) != 0;
    }
    *meta = (OspGcMeta){.size = dead, .mark = 0};
    dead = c + 1;
  }
  page->free = dead;
}

// Sweep every page, hand back the ones left with no live cell, and list the
// rest that have room under their class.
static void gc_pages_sweep(void) {
  OspGcPage *empty = NULL;
  memset(g_room, 0, sizeof g_room);
  for (size_t i = 0; i < g_pages_cap; i++) {
    OspGcPage *page = g_pages[i];
    if (!page) {
      continue;
    }
    gc_page_sweep(page);
    OspGcPage **list = page->live == 0 ? &empty : &g_room[page->cell / OSP_GC_GRAIN];
    if (page->live == 0 || page->free || page->bump < page->ncells) {
      page->next = *list;
      *list = page;
    }
  }
  while (empty) {
    OspGcPage *next = empty->next;
    gc_pages_remove(empty);
    gc_page_unmap(empty);
    empty = next;
  }
}

#endif
