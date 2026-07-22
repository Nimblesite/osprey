// Tracing garbage collector backend for Osprey — the swappable `@osp_alloc`
// implementation selected at link time by `osprey --memory=gc`. Implements
// [MEM-BACKENDS] (docs/specs/0018) and [GC-TRACE-CONSERVATIVE]
// (docs/specs/0018-MemoryManagement.md [MEM-BACKENDS]).
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
// Soundness scope (v1): collection runs ONLY while the process is effectively
// single-threaded (the main thread is the sole allocator). The first allocation
// from any other thread permanently disables collection — a fiber's heap is
// isolated [MEM-FIBER-ISOLATION] and a precise per-fiber collector is future
// work (the precise per-fiber collector of [MEM-BACKENDS]). Every allocation and
// the whole collection run
// hold one mutex, so disabling and the heap table are race-free.

#include <pthread.h>
#include <setjmp.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(__APPLE__)
#include <mach-o/dyld.h>
#include <mach-o/getsect.h>
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
  uint32_t mark;
} OspGcEnt;

static pthread_mutex_t g_lock = PTHREAD_MUTEX_INITIALIZER;

// Adaptive locking, identical in spirit to the ARC backend: the heap starts
// single-threaded, where the sole allocator already serializes every op and the
// mutex is pure overhead. osp_mem_notify_multithreaded() (memory_hooks.h) trips
// this flag from the fiber/websocket/process runtimes *before* the pthread_create
// that first lets a second thread reach the heap; pthread_create is a full
// barrier, so no unlocked op can race a locked one. The flag is monotonic and
// only ever transitions single-threaded, so lock and unlock observe it equal
// within one op. Orthogonal to g_off: the flag governs mutual exclusion, g_off
// governs whether collection may run once a second allocator is seen.
static atomic_int g_gc_mt = 0;

static inline void gc_lock(void) {
  if (atomic_load_explicit(&g_gc_mt, memory_order_acquire)) {
    pthread_mutex_lock(&g_lock);
  }
}

static inline void gc_unlock(void) {
  if (atomic_load_explicit(&g_gc_mt, memory_order_acquire)) {
    pthread_mutex_unlock(&g_lock);
  }
}

static int g_init = 0;     // first allocation has run (main thread + base known)
static int g_off = 0;      // collection permanently disabled (saw a 2nd thread)
static pthread_t g_main;   // the sole-allocator thread that may collect
static uintptr_t g_base;   // highest address of the main thread's stack

static OspGcEnt *g_tab = NULL; // open-addressing heap table (power-of-two cap)
static size_t g_cap = 0;
static size_t g_count = 0;

static uintptr_t *g_ms = NULL; // mark stack of pending object bases
static size_t g_ms_cap = 0;
static size_t g_ms_top = 0;

static size_t g_bytes = 0;                          // managed bytes since last GC
static size_t g_min_heap = OSP_GC_MIN_HEAP_BYTES;   // floor for the adaptive budget
static size_t g_thresh = OSP_GC_MIN_HEAP_BYTES;     // next-collection budget
static int g_debug = 0;      // OSPREY_GC_DEBUG: print per-collection stats
static size_t g_ncollect = 0;

// --- heap table --------------------------------------------------------------

static size_t gc_hash(uintptr_t base, size_t cap) {
  // base is at least 8-byte aligned; drop the dead low bits, then Fibonacci mix.
  uint64_t h = (uint64_t)(base >> 3) * 0x9E3779B97F4A7C15ull;
  return (size_t)(h >> 32) & (cap - 1);
}

static OspGcEnt *gc_lookup(uintptr_t w) {
  if (g_cap == 0) {
    return NULL;
  }
  size_t i = gc_hash(w, g_cap);
  while (g_tab[i].base != 0) {
    if (g_tab[i].base == w) {
      return &g_tab[i];
    }
    i = (i + 1) & (g_cap - 1);
  }
  return NULL;
}

// Place (base,size) into `tab` (no growth, no duplicates) — used by both the
// live insert path and the post-sweep rebuild.
static void gc_place(OspGcEnt *tab, size_t cap, uintptr_t base, size_t size) {
  size_t i = gc_hash(base, cap);
  while (tab[i].base != 0) {
    i = (i + 1) & (cap - 1);
  }
  tab[i].base = base;
  tab[i].size = (uint32_t)size;
  tab[i].mark = 0;
}

static int gc_grow(void) {
  size_t cap = g_cap ? g_cap * 2 : OSP_GC_INIT_TABLE_CAP;
  OspGcEnt *tab = (OspGcEnt *)calloc(cap, sizeof(OspGcEnt));
  if (!tab) {
    return 0;
  }
  for (size_t i = 0; i < g_cap; i++) {
    if (g_tab[i].base != 0) {
      gc_place(tab, cap, g_tab[i].base, g_tab[i].size);
    }
  }
  free(g_tab);
  g_tab = tab;
  g_cap = cap;
  return 1;
}

static void gc_insert(uintptr_t base, size_t size) {
  if (g_cap == 0 || (g_count + 1) * 4 >= g_cap * 3) {
    if (!gc_grow()) {
      return; // out of table memory: object is simply untracked (never freed)
    }
  }
  gc_place(g_tab, g_cap, base, size);
  g_count++;
}

// --- mark --------------------------------------------------------------------

static void gc_ms_push(uintptr_t w) {
  if (g_ms_top == g_ms_cap) {
    size_t cap = g_ms_cap ? g_ms_cap * 2 : OSP_GC_INIT_TABLE_CAP;
    uintptr_t *ms = (uintptr_t *)realloc(g_ms, cap * sizeof(uintptr_t));
    if (!ms) {
      return; // drop the candidate; conservative scanning will re-find it later
    }
    g_ms = ms;
    g_ms_cap = cap;
  }
  g_ms[g_ms_top++] = w;
}

// Treat every aligned word in [lo,hi) as a candidate pointer; mark & enqueue any
// that names a managed object. memcpy reads the word without aliasing UB.
static void gc_scan_range(uintptr_t lo, uintptr_t hi) {
  uintptr_t p = (lo + (OSP_GC_WORD - 1)) & ~(OSP_GC_WORD - 1);
  for (; p + OSP_GC_WORD <= hi; p += OSP_GC_WORD) {
    uintptr_t w;
    memcpy(&w, (const void *)p, sizeof(w));
    OspGcEnt *e = gc_lookup(w);
    if (e && !e->mark) {
      e->mark = 1;
      gc_ms_push(w);
    }
  }
}

static void gc_scan_data(void) {
#if defined(__APPLE__)
  const struct mach_header_64 *mh =
      (const struct mach_header_64 *)_dyld_get_image_header(0);
  if (!mh) {
    return;
  }
  static const char *const sects[] = {"__data", "__bss", "__common"};
  for (size_t i = 0; i < sizeof(sects) / sizeof(sects[0]); i++) {
    unsigned long sz = 0;
    uint8_t *p = getsectiondata(mh, "__DATA", sects[i], &sz);
    if (p && sz) {
      gc_scan_range((uintptr_t)p, (uintptr_t)p + sz);
    }
  }
#elif defined(__linux__) && defined(__GLIBC__)
  // Linker-provided bounds of the executable's initialised data + BSS.
  extern char __data_start[];
  extern char _end[];
  gc_scan_range((uintptr_t)__data_start, (uintptr_t)_end);
#endif
}

// --- collect -----------------------------------------------------------------

// Rebuild the table keeping only marked objects; free the rest; re-arm the
// adaptive budget at max(min-heap, 2× live).
static void gc_sweep(void) {
  size_t live = 0;
  size_t live_bytes = 0;
  for (size_t i = 0; i < g_cap; i++) {
    if (g_tab[i].base != 0 && g_tab[i].mark) {
      live++;
      live_bytes += g_tab[i].size;
    }
  }
  size_t cap = OSP_GC_INIT_TABLE_CAP;
  while (cap * 3 <= (live + 1) * 4) {
    cap *= 2;
  }
  OspGcEnt *tab = (OspGcEnt *)calloc(cap, sizeof(OspGcEnt));
  if (!tab) {
    // Can't rebuild: keep everything (just clear marks). Sound, no reclamation.
    for (size_t i = 0; i < g_cap; i++) {
      g_tab[i].mark = 0;
    }
    return;
  }
  for (size_t i = 0; i < g_cap; i++) {
    if (g_tab[i].base == 0) {
      continue;
    }
    if (g_tab[i].mark) {
      gc_place(tab, cap, g_tab[i].base, g_tab[i].size);
    } else {
      free((void *)g_tab[i].base);
    }
  }
  free(g_tab);
  g_tab = tab;
  g_cap = cap;
  g_count = live;
  g_bytes = live_bytes;
  g_thresh = live_bytes * 2 > g_min_heap ? live_bytes * 2 : g_min_heap;
  if (g_debug) {
    g_ncollect++;
    // env-gated diagnostic only — not on any hot path.
    fprintf(stderr, "[osp-gc] collect #%zu: %zu live objs, %zu live KiB\n",
            g_ncollect, live, live_bytes / 1024);
  }
}

// Caller holds g_lock and is the main thread with collection enabled. setjmp
// flushes callee-saved registers onto this frame; the stack scan from a local
// below covers this frame (jmp_buf included) plus everything up to g_base.
static void gc_collect_locked(void) {
  jmp_buf regs;
  (void)setjmp(regs);
  g_ms_top = 0;

  volatile uintptr_t probe = 0;
  uintptr_t sp = (uintptr_t)&probe;
  uintptr_t lo = sp < g_base ? sp : g_base;
  uintptr_t hi = sp < g_base ? g_base : sp;
  gc_scan_range(lo, hi);
  gc_scan_range((uintptr_t)&regs, (uintptr_t)&regs + sizeof(regs));
  gc_scan_data();

  while (g_ms_top > 0) {
    uintptr_t w = g_ms[--g_ms_top];
    OspGcEnt *e = gc_lookup(w);
    if (e) {
      gc_scan_range(w, w + e->size);
    }
  }
  gc_sweep();
}

static uintptr_t gc_stack_base(void) {
#if defined(__APPLE__)
  return (uintptr_t)pthread_get_stackaddr_np(pthread_self());
#elif defined(__linux__) && defined(__GLIBC__)
  pthread_attr_t a;
  void *addr = NULL;
  size_t sz = 0;
  if (pthread_getattr_np(pthread_self(), &a) == 0) {
    pthread_attr_getstack(&a, &addr, &sz);
    pthread_attr_destroy(&a);
    return (uintptr_t)addr + sz;
  }
  return (uintptr_t)&a;
#else
  uintptr_t here = 0;
  return (uintptr_t)&here;
#endif
}

// --- allocation core (g_lock held) -------------------------------------------

// Maybe collect, then hand back `size` bytes of managed, tracked storage.
static void *gc_managed_alloc(size_t size) {
  if (!g_init) {
    g_main = pthread_self();
    g_base = gc_stack_base();
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
  } else if (!pthread_equal(pthread_self(), g_main)) {
    g_off = 1; // a second thread allocates: disable collection forever
  }
  if (!g_off && g_bytes > g_thresh && pthread_equal(pthread_self(), g_main)) {
    gc_collect_locked();
  }
  void *p = malloc(size ? size : 1);
  if (!p) {
    return NULL;
  }
  gc_insert((uintptr_t)p, size);
  g_bytes += size;
  return p;
}

// --- public ABI --------------------------------------------------------------
// Implements the [MEM-BACKENDS] C interface (docs/specs/0018 §Custom Managers).

void *osp_alloc(int64_t size) {
  gc_lock();
  void *p = gc_managed_alloc(size > 0 ? (size_t)size : 0);
  gc_unlock();
  return p;
}

// Layout-carrying allocation: the meta word (kind + pointer mask) is only
// meaningful to the ARC backend — the conservative collector scans words, so
// it needs no layout and treats this as a plain managed allocation.
void *osp_alloc_tagged(int64_t size, int64_t meta) {
  (void)meta;
  return osp_alloc(size);
}

// Fully-initialized-by-caller twin (memory_hooks.h). The conservative collector
// never relies on pre-zeroed bodies, so it is identical to osp_alloc_tagged.
void *osp_alloc_tagged_noinit(int64_t size, int64_t meta) {
  (void)meta;
  return osp_alloc(size);
}

// Reference-count hooks are no-ops under tracing (Bacon/Cheng/Rajan duality):
// the collector reclaims, so dup/drop carry no work.
void osp_retain(void *o) { (void)o; }
void osp_release(void *o) { (void)o; }
// Codegen-proved-unique drop (memory_hooks.h) — a no-op like osp_release; the
// separate symbol carries LLVM free-pair attributes in the emitted IR.
void osp_release_unique(void *o) { (void)o; }
// Singleton-immortality hook (memory_hooks.h) — meaningful only under ARC.
void osp_mem_immortal(void *p) { (void)p; }
// Multithreaded-heap trip (memory_hooks.h): a second thread is about to touch
// the heap — switch every op from lock-free to mutex-guarded (see g_gc_mt).
void osp_mem_notify_multithreaded(void) {
  atomic_store_explicit(&g_gc_mt, 1, memory_order_release);
}
// Layout-word stamp (memory_hooks.h) — meaningful only under ARC.
void osp_mem_set_layout(void *p, int64_t meta) {
  (void)p;
  (void)meta;
}

void osp_collect(void) {
  gc_lock();
  if (g_init && !g_off && pthread_equal(pthread_self(), g_main)) {
    gc_collect_locked();
  }
  gc_unlock();
}

// Value-container runtime units (list/map) are compiled against osp_gc_shim.h so
// their nodes live in the managed heap and the collector scans the boxed Osprey
// values they hold. free is a no-op — the collector owns reclamation.

void *osp_gc_malloc(size_t size) {
  gc_lock();
  void *p = gc_managed_alloc(size);
  gc_unlock();
  return p;
}

void *osp_gc_calloc(size_t n, size_t size) {
  size_t total = n * size;
  if (n != 0 && total / n != size) {
    return NULL; // overflow
  }
  gc_lock();
  void *p = gc_managed_alloc(total);
  gc_unlock();
  if (p) {
    memset(p, 0, total);
  }
  return p;
}

void *osp_gc_realloc(void *old, size_t size) {
  if (!old) {
    return osp_gc_malloc(size);
  }
  gc_lock();
  OspGcEnt *e = gc_lookup((uintptr_t)old);
  size_t oldsize = e ? e->size : 0;
  void *p = gc_managed_alloc(size); // `old` stays reachable on the caller frame
  gc_unlock();
  if (p && oldsize) {
    memcpy(p, old, oldsize < size ? oldsize : size);
  }
  return p; // old block is reclaimed by a later collection
}

void osp_gc_free(void *p) { (void)p; }
