// Tracing garbage collector backend for Osprey — the swappable `@osp_alloc`
// implementation selected at link time by `osprey --memory=gc`. Implements
// [MEM-BACKENDS] and [GC-TRACE-CONSERVATIVE]
// (docs/specs/compiler/0018-MemoryManagement.md).
//
// Algorithm: a CONSERVATIVE, NON-MOVING mark & sweep over the managed heap
// reachable from the C stack, the machine registers, and the program's data/BSS
// segments — the Boehm-Demers-Weiser discipline (Boehm & Weiser, "Garbage
// Collection in an Uncooperative Environment", SP&E 1988), specialised to
// Osprey's acyclic value heap [MEM-ACYCLIC] where reference cycles cannot exist,
// so a single mark pass is complete (Bacon, Cheng & Rajan, "A Unified Theory of
// Garbage Collection", OOPSLA 2004). A machine word is treated as a live pointer
// iff it equals the base address of a known managed allocation; an integer that
// merely looks like a pointer keeps an object alive (sound — a reachable object
// is never freed) but can never corrupt it (the collector never moves objects).
//
// Soundness scope: collection runs only while the allocator set is the initial
// thread plus registered threads (fibers and effect continuations), and only
// while every other one is parked [MEM-EFFECT-BATON] [MEM-GC-SAFEPOINT].
// The first allocation from any other thread permanently disables collection
// [MEM-FIBER-ISOLATION]. Every allocation and collection holds the heap lock
// after the runtime announces multithreaded execution, so disabling and the heap
// table remain race-free.

#include "memory_hooks.h"
#include <pthread.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef OSP_SYMBOLIC
#pragma clang attribute push(__attribute__((annotate("osprey.gc.collector"))), apply_to = function)
#endif

#if defined(__APPLE__)
#include <mach-o/dyld.h>
#include <mach-o/getsect.h>
#endif

#if defined(_WIN32)
// Only the TEB is needed (see gc_stack_base); LEAN_AND_MEAN keeps the rest of
// the Win32 surface — and its macro pollution — out of this translation unit.
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#endif

// Word size used for conservative scanning and the candidate-pointer step.
#define OSP_GC_WORD ((uintptr_t)sizeof(void *))
// Lower bound on the allocate-since-GC budget (a nursery-like size); the
// live-set-adaptive threshold never drops below this. Overridable via
// OSPREY_GC_HEAP_MB. 4 MiB is the measured knee for allocation-churn workloads
// (binarytrees): collections are O(heap-table size), so too small a floor
// collects constantly (~53% of wall at 1 MiB) while too large a floor lets dead
// objects pile into an ever-bigger table that each sweep must walk. 4 MiB is the
// point where per-collection and per-allocation cost balance — same peak RSS as
// 1 MiB, ~15% less wall — and higher floors only trade RSS away for no CPU gain.
#define OSP_GC_MIN_HEAP_BYTES ((size_t)4u << 20)
// The table tracks only blocks past the largest size class, so it starts small.
#define OSP_GC_INIT_TABLE_CAP ((size_t)1u << 8)
#define OSP_GC_INIT_MARK_STACK ((size_t)1u << 14)

// The state of one managed block: payload size, and GC_MARKED while reached in
// the current collection beside its lifetime flags. Size as u32 (Osprey heap
// values are far below 4 GiB) keeps it at 8 bytes.
typedef struct {
  uint32_t size;
  uint32_t mark;
} OspGcMeta;

// A block of its own in the heap table: base address (the key) and state.
// base==0 marks an empty slot (no live object is at address 0).
typedef struct {
  uintptr_t base;
  OspGcMeta meta;
} OspGcEnt;

typedef struct OspGcPage OspGcPage; // memory_gc_pages.h

#define GC_MARKED 1u
#define GC_FINAL 2u
#define GC_STRING 4u
#define GC_PINNED 8u // osp_mem_immortal: a root of every collection, never swept
// [GC-TRACE-LEAF] The body holds no managed pointer (a string, a scalar list leaf): it is
// marked when reached but never scanned, so a heap of sample buffers costs a lookup per
// block, not one per word.
#define GC_LEAF 16u
#define GC_PERSISTENT (GC_FINAL | GC_STRING | GC_PINNED | GC_LEAF)
#define GC_USED 32u // a page cell that holds a block [MEM-BACKENDS-GC-HEAP]

#include "heap_lock.h"
#include "memory_heap_limit.h"
#include "run_parent_watch.h"

// heap_lock.h governs mutual exclusion; g_off below governs whether collection
// may run once a second allocator is seen. The two are orthogonal.

static int g_init = 0;     // first allocation has run (main thread + base known)
static int g_off = 0;      // collection permanently disabled (saw a 2nd thread)
static size_t g_off_report_at = (size_t)256 << 20; // next uncollected size to log
static pthread_t g_main;   // the sole-allocator thread that may collect
#ifndef OSP_SYMBOLIC
static uintptr_t g_base;   // highest address of the main thread's stack
#endif

static OspGcEnt *g_tab = NULL; // open-addressing heap table (power-of-two cap)
static size_t g_cap = 0;
static size_t g_count = 0;     // blocks tracked, in the table and in pages
static size_t g_tab_count = 0; // of those, the ones in the table
static uintptr_t g_lowest = UINTPTR_MAX; // the lowest base ever tracked; none yet, so nothing is in range
static uintptr_t g_highest = 0;          // the highest base ever tracked

static uintptr_t *g_ms = NULL; // mark stack of pending object bases
static size_t g_ms_cap = 0;
static size_t g_ms_top = 0;

static size_t g_bytes = 0;                          // managed bytes since last GC
static size_t g_min_heap = OSP_GC_MIN_HEAP_BYTES;   // floor for the adaptive budget
static size_t g_thresh = OSP_GC_MIN_HEAP_BYTES;     // next-collection budget
static int g_debug = 0;      // OSPREY_GC_DEBUG: print per-collection stats
static size_t g_ncollect = 0;

#include "memory_gc_table.h"
#include "memory_gc_pages.h"
#include "memory_gc_sweep.h"

// --- baton threads and scanned blocks [MEM-EFFECT-BATON] -----------------------

typedef struct OspGcBaton {
  uintptr_t base;
  uintptr_t parked_at; // lowest live stack address while parked; 0 while running
  struct OspGcBaton *next;
} OspGcBaton;

typedef struct OspGcScanned {
  struct OspGcScanned *prev, *next;
  size_t size;
  uint64_t pad;
} OspGcScanned;

// The initial thread's record lives here so nothing allocates to park it.
static OspGcBaton g_main_baton;
static OspGcBaton *g_batons = NULL;
static OspGcScanned *g_scanned = NULL;
static _Thread_local OspGcBaton *g_self_baton = NULL;

static OspGcBaton *gc_my_baton(void) {
  return g_init && pthread_equal(pthread_self(), g_main) ? &g_main_baton : g_self_baton;
}

// Whether this thread may trace now: it holds the baton and nobody else can be
// mutating. A thread that is mid-handoff only delays the collection to the next
// allocation; it is never scanned while running.
static int gc_may_collect(void) {
  const OspGcBaton *self = gc_my_baton();
  if (g_off || self == NULL) {
    return 0;
  }
  if (self != &g_main_baton && g_main_baton.parked_at == 0) {
    return 0;
  }
  for (const OspGcBaton *b = g_batons; b; b = b->next) {
    if (b != self && b->parked_at == 0) {
      return 0;
    }
  }
  return 1;
}

__attribute__((unused)) static void gc_scan_others(const OspGcBaton *self) {
  if (self != &g_main_baton) {
    gc_scan_range(g_main_baton.parked_at, g_main_baton.base);
  }
  for (const OspGcBaton *b = g_batons; b; b = b->next) {
    if (b != self) {
      gc_scan_range(b->parked_at, b->base);
    }
  }
  for (const OspGcScanned *s = g_scanned; s; s = s->next) {
    gc_scan_range((uintptr_t)(s + 1), (uintptr_t)(s + 1) + s->size);
  }
}

#ifdef OSP_SYMBOLIC
#include "memory_gc_symbolic_roots.h"
#else
#include "memory_gc_native_roots.h"
#endif

// --- collect -----------------------------------------------------------------

// Caller holds g_lock. The root provider covers all active callers and globals.
static void gc_collect_locked(void) {
  heap_note_peak(g_bytes);
  g_ms_top = 0;
  gc_scan_roots();
  gc_scan_pinned();

  while (g_ms_top > 0) {
    uintptr_t w = g_ms[--g_ms_top];
    const OspGcMeta *e = gc_lookup(w);
    if (e && !(e->mark & GC_LEAF)) {
      gc_scan_range(w, w + e->size);
    }
  }
  gc_sweep();
}

#include "memory_gc_safepoint.h"

// --- allocation core (g_lock held) -------------------------------------------

// First-allocation setup: latch the sole collecting thread + its stack base and
// read the env knobs. Runs once (g_init), under g_lock.
static void gc_init_main(void) {
  g_main = pthread_self();
#ifndef OSP_SYMBOLIC
  g_base = gc_stack_base();
  g_main_baton.base = g_base;
#endif
  const char *mb = getenv("OSPREY_GC_HEAP_MB");
  if (mb) {
    long v = strtol(mb, NULL, 10);
    if (v > 0) {
      g_min_heap = (size_t)v << 20;
      g_thresh = g_min_heap;
    }
  }
  g_debug = getenv("OSPREY_GC_DEBUG") != NULL;
  const char *pages = getenv("OSPREY_GC_PAGES");
  g_pages_off = pages != NULL && strcmp(pages, "0") == 0;
  heap_limit_arm();
  g_init = 1;
}

// Maybe collect, then hand back `size` bytes of managed storage tracked under
// `flags`, stamped as the block enters the table rather than looked up again. Once an
// unregistered thread has allocated, g_off is latched on and collection stays
// disabled for everyone.
//
// A small block is a cell of a size-class page [MEM-BACKENDS-GC-HEAP]: no libc call and
// no table slot, and the sweep hands its cell to the next block of that size. A
// block past the largest class, or any block when no page can be had, is a libc
// block of its own in the table, freed by the sweep. [MEM-BACKENDS]
static void *gc_managed_alloc(size_t size, uint32_t flags) {
  if (!g_init) {
    gc_init_main();
  } else if (gc_my_baton() == NULL && !g_off) {
    g_off = 1; // a thread outside the baton allocates: disable collection forever
    if (g_debug) {
      fprintf(stderr, "[osp-gc] collection disabled: a non-baton thread allocated after %zu collections\n",
              g_ncollect);
    }
  }
  if (g_off && g_bytes >= g_off_report_at) {
    // Always on, and only once it matters: a runaway must name its own cause in
    // the log without every short threaded program printing a line.
    fprintf(stderr, "[osp-gc] collection is off (a non-baton thread allocated): %zu MiB uncollected\n",
            g_bytes >> 20);
    g_off_report_at *= 2;
  }
  if (g_collection_wanted || (g_bytes > g_thresh && g_bytes >= g_retry_at)) {
    gc_safepoint();
  }
  if (heap_over_limit(g_bytes, size) && !gc_make_room(size)) {
    return NULL;
  }
  void *p = size <= OSP_GC_SMALL_MAX && !g_pages_off ? gc_page_alloc(size, flags) : NULL;
  if (p) {
    g_count++;
    g_bytes += size;
    return p;
  }
  p = malloc(size ? size : 1);
  if (!p) {
    return NULL;
  }
  if (!gc_insert((uintptr_t)p, size, flags) && (flags & GC_STRING)) {
    free(p); // a string's length lives in its entry: untracked, it is unusable
    return NULL;
  }
  g_bytes += size;
  return p;
}

// --- public ABI --------------------------------------------------------------
// Implements the [MEM-BACKENDS] C interface (docs/specs/compiler/0018-MemoryManagement.md §Custom Managers).

void *osp_alloc(int64_t size) { return osp_alloc_tagged(size, OSP_MEM_RAW); }

// Link anchor (memory_hooks.h): main calls this so an allocation-free program
// still extracts this object. The collector arms lazily; only the runner watch starts here.
void osp_mem_boot(void) { run_parent_watch(); }

// [PORT-STRING-NUL-OWNERSHIP] Length lives in the existing bounded allocation entry.
void *osp_alloc_tagged(int64_t size, int64_t meta) {
  bool text = (meta & 0xFF) == OSP_MEM_STRING;
  if ((size > 0 && (uint64_t)size > SIZE_MAX) ||
      (text && (size < 1 || (uint64_t)size > UINT32_MAX))) return NULL;
  uint32_t flags = text ? (GC_STRING | GC_LEAF) : ((meta & 0xFF) == OSP_MEM_LIST_HDR_EXTERN ? GC_FINAL : 0);
  heap_lock();
  void *p = gc_managed_alloc(size > 0 ? (size_t)size : 0, flags);
  heap_unlock();
  return p;
}

int64_t osp_mem_string_length(const void *p) {
  int held = g_lock_owned;
  if (!held) heap_lock();
  OspGcMeta *e = gc_lookup((uintptr_t)p);
  int64_t length = e && (e->mark & GC_STRING) && e->size ? (int64_t)e->size - 1 : -1;
  if (!held) heap_unlock();
  return length;
}

// Finalizer registration (memory_hooks.h).
void osp_mem_set_finalizer(void (*finalize)(void *body)) { g_finalizer = finalize; }

// Fully-initialized-by-caller twin (memory_hooks.h). The conservative collector
// never relies on pre-zeroed bodies, so it is identical to osp_alloc_tagged.
void *osp_alloc_tagged_noinit(int64_t size, int64_t meta) {
  return osp_alloc_tagged(size, meta);
}

// Reference-count hooks are no-ops under tracing (Bacon/Cheng/Rajan duality):
// the collector reclaims, so dup/drop carry no work.
void osp_retain(void *o) { (void)o; }
void osp_release(void *o) { (void)o; }
// Codegen-proved-unique drop (memory_hooks.h) — a no-op like osp_release; the
// separate symbol carries LLVM free-pair attributes in the emitted IR.
void osp_release_unique(void *o) { (void)o; }
// Singleton-immortality hook (memory_hooks.h): the block is pinned, a root of
// every later collection, so a singleton the runtime keeps only in a global
// survives on every host, data-segment scan or not.
void osp_mem_immortal(void *p) {
  if (p == NULL) {
    return;
  }
  int held = g_lock_owned;
  if (!held) heap_lock();
  OspGcMeta *e = gc_lookup((uintptr_t)p);
  OspGcPage *page = gc_page_of((uintptr_t)p);
  if (e) e->mark |= GC_PINNED;
  if (e && page) page->pinned = 1;
  if (!held) heap_unlock();
}
// Multithreaded-heap trip (memory_hooks.h): a second thread is about to touch
// the heap — switch every op from lock-free to mutex-guarded (see g_heap_mt).
void osp_mem_notify_multithreaded(void) {
  atomic_store_explicit(&g_heap_mt, 1, memory_order_release);
}
#include "memory_gc_batons.h"

// Lifetime flags mirror the layout kinds without enlarging the GC entry. A layout set
// to OSP_MEM_RAW declares opaque bytes with no children (a scalar list leaf), so the
// block becomes a leaf the trace never scans [GC-TRACE-LEAF].
static uint32_t gc_layout_flags(int64_t meta) {
  switch (meta & 0xFF) {
  case OSP_MEM_STRING: return GC_STRING | GC_LEAF;
  case OSP_MEM_RAW: return GC_LEAF;
  default: return 0;
  }
}

void osp_mem_set_layout(void *p, int64_t meta) {
  heap_lock();
  OspGcMeta *e = gc_lookup((uintptr_t)p);
  // Existing finalizer ownership is unchanged.
  if (e) e->mark = (e->mark & ~(GC_STRING | GC_LEAF)) | gc_layout_flags(meta);
  heap_unlock();
}

void osp_collect(void) {
  heap_lock();
  if (g_init && gc_may_collect()) {
    gc_collect_locked();
  }
  heap_unlock();
}

// --- diagnostics (read-only snapshots; not on any hot path) ------------------
static size_t gc_snapshot(const size_t *counter) {
  heap_lock();
  size_t n = *counter;
  heap_unlock();
  return n;
}

// Objects currently tracked by the collector. Immediately after osp_collect()
// returns on the main thread this equals the reachable (live) set, so tooling
// and the conformance tests can assert that unreachable churn was reclaimed.
size_t osp_gc_live_objects(void) { return gc_snapshot(&g_count); }

// Slots in the heap table. Lets a test confirm the table keeps the capacity a
// steady workload needs across collections, and gives it back once unused.
size_t osp_gc_table_capacity(void) { return gc_snapshot(&g_cap); }

// Size-class pages currently held. Lets a test confirm small blocks fill pages,
// dead cells are reused, and a page with no live cell is handed back.
size_t osp_gc_pages(void) { return gc_snapshot(&g_npages); }

// Collections run since start. Lets a test confirm a collection actually fired
// (or, after the multithread latch, that collection stopped firing).
size_t osp_gc_collections(void) { return gc_snapshot(&g_ncollect); }

#include "memory_gc_libc.h"

#ifdef OSP_SYMBOLIC
#pragma clang attribute pop
#endif
