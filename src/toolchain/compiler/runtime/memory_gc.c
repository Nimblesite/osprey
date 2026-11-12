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
#define OSP_GC_INIT_TABLE_CAP ((size_t)1u << 14)

// One managed allocation: base address (the table key), payload size, mark bit.
// base==0 marks an empty slot (no live object is at address 0). 16 bytes — size
// as u32 (Osprey heap values are far below 4 GiB) keeps the table compact.
typedef struct {
  uintptr_t base;
  uint32_t size;
  uint32_t mark; // GC_MARKED while reached in the current collection; GC_FINAL for a finalizer
} OspGcEnt;

#define GC_MARKED 1u
#define GC_FINAL 2u
#define GC_STRING 4u
#define GC_PINNED 8u // osp_mem_immortal: a root of every collection, never swept
#define GC_PERSISTENT (GC_FINAL | GC_STRING | GC_PINNED)

#include "heap_lock.h"

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
static size_t g_count = 0;
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

// The finalizer OSP_MEM_LIST_HDR_EXTERN blocks pass through (memory_hooks.h).
static void (*g_finalizer)(void *body);

// Run a dying block's finalizer, when it carries one, over its intact words.
static void gc_finalize(const OspGcEnt *e) {
  if ((e->mark & GC_FINAL) && g_finalizer) {
    g_finalizer((void *)e->base);
  }
}

// Finalizers see the whole table: one may ask the collector about a block that
// outlives this collection, so they all run before any slot is emptied.
static void gc_finalize_dead(void) {
  for (size_t i = 0; g_finalizer && i < g_cap; i++) {
    if (g_tab[i].base != 0 && !(g_tab[i].mark & GC_MARKED)) {
      gc_finalize(&g_tab[i]);
    }
  }
}

// Free every unmarked block and empty its slot. Survivors keep GC_MARKED, which
// from here on reads "not yet reseated". Sets g_count and g_bytes to the live set.
static void gc_free_dead(void) {
  g_count = 0;
  g_bytes = 0;
  for (size_t i = 0; i < g_cap; i++) {
    if (g_tab[i].base == 0) {
      continue;
    }
    if (g_tab[i].mark & GC_MARKED) {
      g_count++;
      g_bytes += g_tab[i].size;
    } else {
      free((void *)g_tab[i].base);
      g_tab[i].base = 0;
    }
  }
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
    e.mark &= GC_PERSISTENT;
    size_t j = gc_hash(e.base, g_cap);
    while (g_tab[j].base != 0 && !(g_tab[j].mark & GC_MARKED)) {
      j = (j + 1) & (g_cap - 1);
    }
    OspGcEnt waiting = g_tab[j];
    g_tab[j] = e;
    e = waiting;
  }
}

// Sweep the table where it stands: free the unmarked, reseat the survivors, and
// re-arm the adaptive budget at max(min-heap, 2× live). The table keeps the
// capacity this cycle's allocations needed, because a steady workload fills it
// again before the next collection; a fresh table per collection cost a
// page-faulted allocation and a rehash at every doubling on the way back up. It
// is cut back only once a whole cycle has peaked below it [GC-TRACE-CONSERVATIVE].
static void gc_sweep(void) {
  size_t fit = gc_fit(g_count);
  gc_finalize_dead();
  gc_free_dead();
  for (size_t i = 0; i < g_cap; i++) {
    if (g_tab[i].base != 0 && (g_tab[i].mark & GC_MARKED)) {
      gc_reseat(i);
    }
  }
  if (fit < g_cap) {
    (void)gc_resize(fit); // out of memory: the oversized table still serves
  }
  g_thresh = g_bytes * 2 > g_min_heap ? g_bytes * 2 : g_min_heap;
  g_ncollect++; // always: read by osp_gc_collections() diagnostics, not just debug
  if (g_debug) {
    // env-gated diagnostic only — not on any hot path.
    fprintf(stderr, "[osp-gc] collect #%zu: %zu live objs, %zu live KiB\n",
            g_ncollect, g_count, g_bytes / 1024);
  }
}

// Caller holds g_lock. The root provider covers all active callers and globals.
// Pinned blocks are roots wherever they are referenced from: a runtime
// singleton held only by a global stays alive even on a host whose data
// segment this collector cannot scan.
static void gc_scan_pinned(void) {
  for (size_t i = 0; i < g_cap; i++) {
    if (g_tab[i].base != 0 && (g_tab[i].mark & GC_PINNED) && !(g_tab[i].mark & GC_MARKED)) {
      g_tab[i].mark |= GC_MARKED;
      gc_ms_push(g_tab[i].base);
    }
  }
}

static void gc_collect_locked(void) {
  g_ms_top = 0;
  gc_scan_roots();
  gc_scan_pinned();

  while (g_ms_top > 0) {
    uintptr_t w = g_ms[--g_ms_top];
    OspGcEnt *e = gc_lookup(w);
    if (e) {
      gc_scan_range(w, w + e->size);
    }
  }
  gc_sweep();
}

#ifdef OSP_SYMBOLIC
// Symbolic runs have one thread, so nothing is ever waited for.
static const int g_collection_wanted = 0;
static const size_t g_retry_at = 0;
static void gc_safepoint(void) {
  if (gc_may_collect()) {
    gc_collect_locked();
  }
}
static void gc_wake_safepoint(void) {}
#else
#include "memory_gc_safepoint.h"
#endif

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
  g_init = 1;
}

// Maybe collect, then hand back `size` bytes of managed storage tracked under
// `flags`, stamped as the block enters the table rather than looked up again. Once an
// unregistered thread has allocated, g_off is latched on and collection stays
// disabled for everyone.
//
// Reclamation is left to libc malloc/free (via gc_sweep), NOT an intrusive
// recycling pool: a tracing collector cannot reclaim a body until the next
// collection (~a heap-budget of allocation later), so pooled reuse would cycle
// a cold multi-MiB working set, while libc's per-size free-lists hand back the
// just-freed, cache-hot block the next tree reuses. Measured: a pool here made
// binarytrees ~12x SLOWER for zero reclaim benefit. [MEM-BACKENDS]
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
  void *p = malloc(size ? size : 1);
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
// still extracts this object. The collector arms lazily; nothing to do here.
void osp_mem_boot(void) {}

// [PORT-STRING-NUL-OWNERSHIP] Length lives in the existing bounded allocation entry.
void *osp_alloc_tagged(int64_t size, int64_t meta) {
  bool text = (meta & 0xFF) == OSP_MEM_STRING;
  if ((size > 0 && (uint64_t)size > SIZE_MAX) ||
      (text && (size < 1 || (uint64_t)size > UINT32_MAX))) return NULL;
  uint32_t flags = text ? GC_STRING : ((meta & 0xFF) == OSP_MEM_LIST_HDR_EXTERN ? GC_FINAL : 0);
  heap_lock();
  void *p = gc_managed_alloc(size > 0 ? (size_t)size : 0, flags);
  heap_unlock();
  return p;
}

int64_t osp_mem_string_length(const void *p) {
  int held = g_lock_owned;
  if (!held) heap_lock();
  OspGcEnt *e = gc_lookup((uintptr_t)p);
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
  OspGcEnt *e = gc_lookup((uintptr_t)p);
  if (e) e->mark |= GC_PINNED;
  if (!held) heap_unlock();
}
// Multithreaded-heap trip (memory_hooks.h): a second thread is about to touch
// the heap — switch every op from lock-free to mutex-guarded (see g_heap_mt).
void osp_mem_notify_multithreaded(void) {
  atomic_store_explicit(&g_heap_mt, 1, memory_order_release);
}
#include "memory_gc_batons.h"

// Lifetime flags mirror the layout kinds without enlarging the GC entry.
void osp_mem_set_layout(void *p, int64_t meta) {
  heap_lock();
  OspGcEnt *e = gc_lookup((uintptr_t)p);
  uint32_t flags = (meta & 0xFF) == OSP_MEM_STRING ? GC_STRING : 0;
  if (e) e->mark = (e->mark & ~GC_STRING) | flags; // Existing finalizer ownership is unchanged.
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

// Collections run since start. Lets a test confirm a collection actually fired
// (or, after the multithread latch, that collection stopped firing).
size_t osp_gc_collections(void) { return gc_snapshot(&g_ncollect); }

#include "memory_gc_libc.h"

#ifdef OSP_SYMBOLIC
#pragma clang attribute pop
#endif
