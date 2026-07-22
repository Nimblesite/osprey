// Perceus ARC backend for Osprey — the swappable `@osp_alloc` implementation
// selected at link time by `osprey --memory=arc`. Implements [MEM-BACKENDS]
// (docs/specs/0018) and [GC-ARC-PERCEUS] (docs/plans/0011, phase 2).
//
// Algorithm: precise reference counting after Reinking, Xie, de Moura & Leijen,
// "Perceus: Garbage Free Reference Counting with Reuse", MSR-TR-2020-42
// (https://www.microsoft.com/en-us/research/wp-content/uploads/2020/11/perceus-tr-v1.pdf).
// Complete without a cycle collector because the Osprey value heap is acyclic
// [MEM-ACYCLIC] (Bacon/Cheng/Rajan duality). Each allocation carries a 16-byte
// header (TR §2.5's Koka header, widened to hold the per-site layout word):
//
//   { int64_t meta; int32_t rc; uint32_t size; }   at body-16
//
// `meta` is the layout word codegen passes to osp_alloc_tagged: low 8 bits a
// kind, upper 56 a word bitmask (bit i => the 8-byte word at body offset 8*i
// is a managed pointer). `rc` is SIGNED: rc >= 1 counts owners; rc < 0 marks
// immortal/persistent objects that dup/drop skip (the cross-fiber atomic path
// of plan 0011 M5 also lives below zero).
//
// Provenance (plan 0011 phase 2, Amendment 2): pointer slots at runtime also
// carry rodata literals, static C strings, foreign malloc/strdup memory,
// borrowed FFI pointers, and NULL. osp_retain/osp_release therefore probe an
// open-addressing registry of live ARC allocations FIRST; a probe miss means
// "not ours" and is a safe no-op. No IR special-casing, no FFI annotations —
// non-ARC pointers are unmanaged by construction.
//
// Concurrency scope (v1): every operation holds one mutex, exactly the
// memory_gc.c discipline — sound across fiber pthreads, slow but conforming.
// The non-atomic fast path keyed on [MEM-FIBER-ISOLATION] is milestone M5.

#include "memory_hooks.h"

#include <pthread.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// --- header ------------------------------------------------------------------

typedef struct {
  int64_t meta;  // low 8 bits kind, upper 56 word-pointer bitmask
  int32_t rc;    // >=1 live owners; <0 immortal/persistent (skip dup/drop)
  uint32_t size; // body bytes (leak stats + realloc)
} OspArcHdr;

#define OSP_ARC_HDR ((size_t)sizeof(OspArcHdr))

// Layout-word kinds live in memory_hooks.h — the one place codegen
// (crates/osprey-codegen/src/meta.rs) and every C runtime unit agree.

#define OSP_ARC_KIND(meta) ((int)((meta) & 0xFF))
#define OSP_ARC_MASK_BITS(meta) (((uint64_t)(meta)) >> 8)

static OspArcHdr *arc_hdr(void *body) {
  return (OspArcHdr *)(void *)((char *)body - OSP_ARC_HDR);
}

// --- live-allocation registry (open addressing, tombstone deletion) -----------

#define OSP_ARC_INIT_CAP ((size_t)1u << 14)
#define OSP_ARC_TOMB ((uintptr_t)1)

static pthread_mutex_t g_lock = PTHREAD_MUTEX_INITIALIZER;

// Adaptive locking. The heap starts single-threaded, where the sole thread
// already serializes every ARC op and the mutex is pure overhead — binarytrees
// alone runs ~19.6M locked allocs, and pthread_mutex_* was ~17% of its wall.
// osp_mem_notify_multithreaded() (memory_hooks.h) trips this flag from the
// fiber runtime *before* the pthread_create that first lets a second thread
// reach the heap. pthread_create is a full barrier, so that write
// happens-before the child's first ARC op: no unlocked op can ever race a
// locked one. The flag is monotonic (fibers never un-share the heap), and it
// only ever transitions in a single-threaded context, so within one ARC op
// arc_lock and arc_unlock always observe the same value.
static atomic_int g_arc_mt = 0;

static inline void arc_lock(void) {
  if (atomic_load_explicit(&g_arc_mt, memory_order_acquire)) {
    pthread_mutex_lock(&g_lock);
  }
}

static inline void arc_unlock(void) {
  if (atomic_load_explicit(&g_arc_mt, memory_order_acquire)) {
    pthread_mutex_unlock(&g_lock);
  }
}

static uintptr_t *g_tab = NULL; // body addresses; 0 = empty, 1 = tombstone
static size_t g_cap = 0;
static size_t g_used = 0;    // non-empty slots: live + pooled + tombstones (grow trigger)
static size_t g_live = 0;    // live objects (leak stats)
static size_t g_present = 0; // non-tombstone slots: live + pooled (table sizing)
static size_t g_live_bytes = 0;
static int g_debug_armed = 0;

static size_t arc_hash(uintptr_t body, size_t cap) {
  uint64_t h = (uint64_t)(body >> 3) * 0x9E3779B97F4A7C15ull;
  return (size_t)(h >> 32) & (cap - 1);
}

// The slot holding `body`, or NULL. Callers hold g_lock.
static uintptr_t *arc_find(uintptr_t body) {
  if (g_cap == 0 || body <= OSP_ARC_TOMB) {
    return NULL;
  }
  size_t i = arc_hash(body, g_cap);
  while (g_tab[i] != 0) {
    if (g_tab[i] == body) {
      return &g_tab[i];
    }
    i = (i + 1) & (g_cap - 1);
  }
  return NULL;
}

static void arc_place(uintptr_t *tab, size_t cap, uintptr_t body) {
  size_t i = arc_hash(body, cap);
  while (tab[i] > OSP_ARC_TOMB) {
    i = (i + 1) & (cap - 1);
  }
  tab[i] = body;
}

// The capacity the PRESENT set needs: the smallest power of two at or above the
// floor that keeps the load factor under 1/4. Sizing off `g_present` (live +
// pooled entries, i.e. every non-tombstone slot) rather than off the old
// capacity is what makes churn-heavy programs (allocate, free, repeat) cheap:
// their tombstones fill the table without growing the present set, so doubling
// on every rebuild would track TOTAL allocations instead of resident ones —
// 10M short-lived tree nodes cost a 128 MiB table instead of 256 KiB. Pooled
// blocks keep their slots (recycling pool below), so they count toward present.
static size_t arc_target_cap(void) {
  size_t cap = OSP_ARC_INIT_CAP;
  while (cap < g_present * 4 && cap < ((size_t)-1) / 2) {
    cap *= 2;
  }
  return cap;
}

// Rebuild at the live-set capacity (also compacts tombstones away).
static int arc_grow(void) {
  size_t cap = arc_target_cap();
  uintptr_t *tab = (uintptr_t *)calloc(cap, sizeof(uintptr_t));
  if (!tab) {
    return 0;
  }
  for (size_t i = 0; i < g_cap; i++) {
    if (g_tab[i] > OSP_ARC_TOMB) {
      arc_place(tab, cap, g_tab[i]);
    }
  }
  free(g_tab);
  g_tab = tab;
  g_cap = cap;
  g_used = g_present; // rebuild dropped every tombstone; only present slots remain
  return 1;
}

static void arc_insert(uintptr_t body) {
  if (g_cap == 0 || (g_used + 1) * 4 >= g_cap * 3) {
    if (!arc_grow()) {
      return; // out of table memory: object is untracked (leaks, never freed)
    }
  }
  arc_place(g_tab, g_cap, body);
  g_used++;
  g_live++;
  g_present++;
}

static void arc_remove(uintptr_t *slot) {
  *slot = OSP_ARC_TOMB;
  g_live--;
  g_present--;
}

// --- size-classed recycling pool ---------------------------------------------
// A freed body is not handed back to libc: it is pushed onto an intrusive
// per-size free-list (its first word links to the next free body) and its
// registry slot is KEPT, so a later allocation of the same rounded size pops it
// and reuses the SAME address — no malloc, no free, and no registry
// insert/remove/grow churn. binarytrees alone runs ~19.6M alloc/free pairs
// through here; recycling turns each into a stack push/pop and keeps the live
// set (one depth-13 tree at a time) resident instead of round-tripping libc.
//
// Pooled blocks carry rc == 0 (live blocks are rc >= 1, immortals rc < 0), a
// state the leak report skips. Their slots count toward g_present, never
// g_live. Sizes above the ceiling, or pushes past the retention cap, fall back
// to the original malloc/free + registry-remove path (rare, large objects).
#define OSP_ARC_POOL_GRAIN ((size_t)16)      // capacity granularity (keeps 16-align)
#define OSP_ARC_POOL_BUCKETS ((size_t)256)   // ceiling = 256 * 16 = 4096 body bytes
#define OSP_ARC_POOL_MAX_CAP (OSP_ARC_POOL_BUCKETS * OSP_ARC_POOL_GRAIN)
#define OSP_ARC_POOL_CAP_BYTES ((size_t)64u << 20) // retained-pool ceiling, then libc

static void *g_pool[OSP_ARC_POOL_BUCKETS + 1]; // free-list heads, indexed by cap/GRAIN
static size_t g_pool_bytes = 0;                // bytes parked on the free-lists

// The rounded capacity (>= GRAIN, a multiple of GRAIN) a body of `size` occupies.
// Deterministic, so a block freed to bucket k always has exactly k*GRAIN bytes
// and a request mapping to bucket k needs no more — reuse cannot under-allocate.
static size_t arc_pool_cap(size_t size) {
  size_t s = size ? size : 1;
  return (s + (OSP_ARC_POOL_GRAIN - 1)) & ~(OSP_ARC_POOL_GRAIN - 1);
}

// Pop a reusable body of exactly `cap` bytes, or NULL. Its registry slot is
// still live (never tombstoned), so the caller reuses it without re-inserting.
static void *arc_pool_pop(size_t cap) {
  size_t b = cap / OSP_ARC_POOL_GRAIN;
  void *body = g_pool[b];
  if (body) {
    g_pool[b] = *(void **)body; // unlink: the first word is the next-free link
    g_pool_bytes -= cap;
  }
  return body;
}

// Park a dead body of `cap` bytes on its free-list (slot kept). Returns 0 when
// oversized or past the retention cap, so the caller frees it to libc instead.
static int arc_pool_push(void *body, size_t cap) {
  if (cap > OSP_ARC_POOL_MAX_CAP || g_pool_bytes + cap > OSP_ARC_POOL_CAP_BYTES) {
    return 0;
  }
  size_t b = cap / OSP_ARC_POOL_GRAIN;
  *(void **)body = g_pool[b];
  g_pool[b] = body;
  g_pool_bytes += cap;
  return 1;
}

// --- leak accounting (OSPREY_ARC_DEBUG=1) --------------------------------------

// Kinds RAW..PTR_ARRAY (memory_hooks.h) — one histogram bucket per kind.
#define OSP_ARC_NKINDS 5

// OSPREY_ARC_DEBUG=2 additionally dumps every survivor — kind, size, rc, and a
// printable preview of its first bytes. That triple is what actually identifies
// a leak: a 40-byte MASK is a list header, a 24-byte MASK a map header, a RAW
// whose preview reads as text is a runtime-minted string. Triage tool, not a log.
#define OSP_ARC_PREVIEW 24

static void arc_dump_object(const void *body, const OspArcHdr *h) {
  char preview[OSP_ARC_PREVIEW + 1];
  size_t n = h->size < (uint32_t)OSP_ARC_PREVIEW ? h->size : OSP_ARC_PREVIEW;
  const unsigned char *b = (const unsigned char *)body;
  for (size_t i = 0; i < n; i++) {
    preview[i] = (b[i] >= 0x20 && b[i] < 0x7f) ? (char)b[i] : '.';
  }
  preview[n] = '\0';
  fprintf(stderr, "[osp-arc]     kind %d size %u rc %d |%s|\n",
          OSP_ARC_KIND(h->meta), h->size, h->rc, preview);
}

static void arc_report_leaks(void) {
  const char *level = getenv("OSPREY_ARC_DEBUG");
  int dump = (level != NULL && level[0] >= '2');
  arc_lock();
  size_t count[OSP_ARC_NKINDS] = {0};
  size_t bytes[OSP_ARC_NKINDS] = {0};
  // Immortals (rc<0 — the empty-list singleton and friends) are excluded from
  // the leak count on purpose: they are unreclaimable BY DESIGN, so counting
  // them would put a permanent floor under the [GC-ARC-PERCEUS] "zero leaked
  // language values" gate the differential harness reads off this line.
  size_t immortal = 0;
  for (size_t i = 0; i < g_cap; i++) {
    if (g_tab[i] > OSP_ARC_TOMB) {
      OspArcHdr *h = arc_hdr((void *)g_tab[i]);
      int kind = OSP_ARC_KIND(h->meta);
      int k = (kind >= 0 && kind < OSP_ARC_NKINDS) ? kind : 0;
      if (h->rc == 0) {
        continue; // parked on the recycling pool — resident, not leaked
      }
      if (h->rc < 0) {
        immortal++;
        continue;
      }
      count[k]++;
      bytes[k] += h->size;
      if (dump) {
        arc_dump_object((const void *)g_tab[i], h);
      }
    }
  }
  // stderr only: stdout must stay byte-identical under [MEM-BACKENDS].
  fprintf(stderr, "[osp-arc] exit: %zu live objects, %zu KiB (+%zu immortal)\n",
          g_live - immortal, g_live_bytes / 1024, immortal);
  for (int k = 0; k < OSP_ARC_NKINDS; k++) {
    if (count[k]) {
      fprintf(stderr, "[osp-arc]   kind %d: %zu objects, %zu bytes\n", k,
              count[k], bytes[k]);
    }
  }
  arc_unlock();
}

static void arc_arm_debug(void) {
  if (!g_debug_armed) {
    g_debug_armed = 1;
    if (getenv("OSPREY_ARC_DEBUG")) {
      (void)atexit(arc_report_leaks);
    }
  }
}

// --- allocation ----------------------------------------------------------------

// The header records `size` in a uint32_t, so that is the hard per-object
// ceiling. Rejecting above it does double duty: it keeps h->size EXACT (a
// truncated size would make the drop walk read the wrong number of words and
// g_live_bytes drift), and it makes `OSP_ARC_HDR + size` unable to wrap —
// without the check, size = SIZE_MAX gives malloc(15), which SUCCEEDS and
// returns a body sitting entirely outside the block.
#define OSP_ARC_MAX_BODY ((size_t)UINT32_MAX)

// A body the drop walk will READ as pointers must start zeroed: a field the
// producer never stores would otherwise be walked as a garbage address. RAW
// bodies are never walked, so they keep malloc's speed.
static void arc_init_body(void *body, int64_t meta, size_t size) {
  OspArcHdr *h = arc_hdr(body);
  h->meta = meta;
  h->rc = 1;
  h->size = (uint32_t)size;
  if (OSP_ARC_KIND(meta) != OSP_MEM_RAW) {
    memset(body, 0, size);
  }
  g_live_bytes += size;
}

// Header'd, registered allocation. Callers hold g_lock. Recycles a same-sized
// pooled block when one is parked (no malloc, no registry insert); otherwise
// mallocs the ROUNDED capacity so the block is reusable by any request in its
// bucket, and registers it.
static void *arc_alloc_locked(size_t size, int64_t meta) {
  arc_arm_debug();
  if (size > OSP_ARC_MAX_BODY) {
    return NULL;
  }
  size_t cap = arc_pool_cap(size);
  if (cap <= OSP_ARC_POOL_MAX_CAP) {
    void *body = arc_pool_pop(cap);
    if (body) {
      g_live++; // slot already present; only the live count moves
      arc_init_body(body, meta, size);
      return body;
    }
  }
  OspArcHdr *h = (OspArcHdr *)malloc(OSP_ARC_HDR + cap);
  if (!h) {
    return NULL;
  }
  void *body = (char *)h + OSP_ARC_HDR;
  arc_insert((uintptr_t)body);
  arc_init_body(body, meta, size);
  return body;
}

// Reclaim a dead body: park it on the recycling pool (slot kept, O(1)) when it
// fits, else remove it from the registry and hand the block back to libc. The
// hot path (small nodes) never probes the registry — the worklist already holds
// the body, and a pooled slot is left in place for the next same-sized alloc.
static void arc_free_or_recycle(void *body) {
  OspArcHdr *h = arc_hdr(body);
  g_live_bytes -= h->size;
  h->rc = 0; // pooled sentinel (leak report skips it); harmless if freed below
  if (arc_pool_push(body, arc_pool_cap(h->size))) {
    g_live--; // slot stays present; only the live count moves
    return;
  }
  uintptr_t *slot = arc_find((uintptr_t)body);
  if (slot) {
    arc_remove(slot); // g_live-- and g_present--
  } else {
    g_live--; // defensive: a registered body is always found
  }
  free(h);
}

// --- release: worklist drop over the kind/mask layout ---------------------------

// Bounded push-down list of bodies whose rc reached zero (no C recursion, no
// re-entrant locking). On OOM the remaining children simply leak — sound.
static uintptr_t *g_wl = NULL;
static size_t g_wl_cap = 0;
static size_t g_wl_top = 0;

static void arc_wl_push(uintptr_t body) {
  if (g_wl_top == g_wl_cap) {
    size_t cap = g_wl_cap ? g_wl_cap * 2 : 256;
    uintptr_t *wl = (uintptr_t *)realloc(g_wl, cap * sizeof(uintptr_t));
    if (!wl) {
      return; // drop the child on the floor: it leaks, never corrupts
    }
    g_wl = wl;
    g_wl_cap = cap;
  }
  g_wl[g_wl_top++] = body;
}

// Decrement a candidate child; queue it for freeing when it hits zero.
// A probe miss (foreign/rodata/NULL) or an immortal (rc<0) is a no-op.
static void arc_drop_child(void *child) {
  uintptr_t *slot = arc_find((uintptr_t)child);
  if (!slot) {
    return;
  }
  OspArcHdr *h = arc_hdr((void *)*slot);
  if (h->rc < 0) {
    return;
  }
  if (--h->rc == 0) {
    arc_wl_push(*slot);
  }
}

// Bodies are 16-byte aligned (16-byte header + malloc's 16-byte guarantee), so
// every 8-byte word is aligned and a pointer field is a direct load. memcpy of
// 8 bytes is worse than it looks: -D_FORTIFY_SOURCE routes it through
// __memcpy_chk, which never inlines and tails into _platform_memmove — ~7% of
// binarytrees wall was spent copying one pointer at a time in the drop walk.
static inline void *arc_load_child(const char *at) {
  return *(void *const *)(const void *)at;
}

static void arc_drop_masked(char *body, uint32_t size, uint64_t mask) {
  for (unsigned i = 0; mask != 0 && (size_t)(i + 1) * 8 <= size; i++, mask >>= 1) {
    if (mask & 1) {
      arc_drop_child(arc_load_child(body + (size_t)i * 8));
    }
  }
}

// { i64 len, i8* data }: release elements (PTR kind only), then the array.
static void arc_drop_list_hdr(char *body, uint32_t size, int elems_are_ptrs) {
  if (size < 16) {
    return;
  }
  int64_t len;
  memcpy(&len, body, sizeof(len));
  char *data = (char *)arc_load_child(body + 8);
  if (data && elems_are_ptrs) {
    for (int64_t j = 0; j < len; j++) {
      arc_drop_child(arc_load_child(data + (size_t)j * 8));
    }
  }
  arc_drop_child(data);
}

// A flat array of managed pointers whose length the header's `size` carries —
// the out-of-line internals of the value containers (a HAMT node's `children`,
// a trie node's slots). The mask cannot express these: it is 56 bits wide and a
// collision array is unbounded. Zero words (allocation slack) probe-miss.
static void arc_drop_ptr_array(char *body, uint32_t size) {
  for (size_t off = 0; off + 8 <= size; off += 8) {
    arc_drop_child(arc_load_child(body + off));
  }
}

// Free `start`'s object and everything transitively released by it. Holds
// g_lock. Bodies enter the worklist already confirmed managed (the initial
// caller found the slot; arc_drop_child pushes only a slot it just matched and
// decremented to zero), so no per-node re-probe is needed — the drop walk reads
// the header directly and reclaims via arc_free_or_recycle.
static void arc_release_zero_locked(uintptr_t start) {
  g_wl_top = 0;
  arc_wl_push(start);
  while (g_wl_top > 0) {
    uintptr_t body = g_wl[--g_wl_top];
    OspArcHdr *h = arc_hdr((void *)body);
    int kind = OSP_ARC_KIND(h->meta);
    if (kind == OSP_MEM_MASK) {
      arc_drop_masked((char *)body, h->size, OSP_ARC_MASK_BITS(h->meta));
    } else if (kind == OSP_MEM_LIST_HDR_PTR || kind == OSP_MEM_LIST_HDR_SCALAR) {
      arc_drop_list_hdr((char *)body, h->size, kind == OSP_MEM_LIST_HDR_PTR);
    } else if (kind == OSP_MEM_PTR_ARRAY) {
      arc_drop_ptr_array((char *)body, h->size);
    }
    arc_free_or_recycle((void *)body);
  }
}

// --- public ABI ------------------------------------------------------------------
// Implements the [MEM-BACKENDS] C interface (docs/specs/0018 §Custom Managers).

void *osp_alloc_tagged(int64_t size, int64_t meta) {
  arc_lock();
  void *p = arc_alloc_locked(size > 0 ? (size_t)size : 0, meta);
  arc_unlock();
  return p;
}

void *osp_alloc(int64_t size) { return osp_alloc_tagged(size, OSP_MEM_RAW); }

// dup — probe first: only live ARC objects with a non-negative rc count.
void osp_retain(void *o) {
  if (!o) {
    return;
  }
  arc_lock();
  uintptr_t *slot = arc_find((uintptr_t)o);
  if (slot) {
    OspArcHdr *h = arc_hdr((void *)*slot);
    if (h->rc >= 0) {
      h->rc++;
    }
  }
  arc_unlock();
}

// drop — probe first; on rc==0 run the kind/mask worklist drop.
void osp_release(void *o) {
  if (!o) {
    return;
  }
  arc_lock();
  uintptr_t *slot = arc_find((uintptr_t)o);
  if (slot) {
    OspArcHdr *h = arc_hdr((void *)*slot);
    if (h->rc > 0 && --h->rc == 0) {
      arc_release_zero_locked(*slot);
    }
  }
  arc_unlock();
}

// Codegen-proved-unique drop (memory_hooks.h): the caller proved rc == 1, so
// this is exactly osp_release — the distinct symbol only exists to carry
// LLVM's free-pair attributes so -O2 can delete non-escaping alloc+release
// pairs before they ever reach the runtime.
void osp_release_unique(void *o) { osp_release(o); }

// Full-collection hook: nothing to do — acyclic naive RC is already complete
// (Bacon/Cheng/Rajan; TR §2.2).
void osp_collect(void) {}

// Trip the adaptive lock (memory_hooks.h): the fiber runtime calls this before
// the pthread_create that first lets a second thread reach the heap. Monotonic
// and idempotent — once shared, the heap stays locked for the process.
void osp_mem_notify_multithreaded(void) {
  atomic_store_explicit(&g_arc_mt, 1, memory_order_release);
}

// Immortalize a shared C-runtime singleton (memory_hooks.h): rc < 0 makes
// every later dup/drop skip it, so alias returns of e.g. the empty-list
// singleton can never free it. Foreign pointers probe-miss (no-op).
void osp_mem_immortal(void *p) {
  if (!p) {
    return;
  }
  arc_lock();
  uintptr_t *slot = arc_find((uintptr_t)p);
  if (slot) {
    arc_hdr((void *)*slot)->rc = -1;
  }
  arc_unlock();
}

// Stamp a shim-allocated object with its layout word (memory_hooks.h): the
// value-container units allocate through the zeroing `calloc` redirect and tag
// afterwards, which is what keeps a PTR_ARRAY walk over allocation slack safe.
void osp_mem_set_layout(void *p, int64_t meta) {
  if (!p) {
    return;
  }
  arc_lock();
  uintptr_t *slot = arc_find((uintptr_t)p);
  if (slot) {
    arc_hdr((void *)*slot)->meta = meta;
  }
  arc_unlock();
}

// --- allocation shim for the C runtime units (osp_arc_shim.h) ---------------------
// Value-producing units (list/map/string/json) are recompiled with their libc
// allocation calls redirected here, so runtime-minted strings and container
// nodes carry headers and live in the registry (kind RAW until milestone M4
// teaches the container ops their own dup/drop).

void *osp_arc_malloc(size_t size) {
  arc_lock();
  void *p = arc_alloc_locked(size, OSP_MEM_RAW);
  arc_unlock();
  return p;
}

void *osp_arc_calloc(size_t n, size_t size) {
  size_t total = n * size;
  if (n != 0 && total / n != size) {
    return NULL; // overflow
  }
  void *p = osp_arc_malloc(total);
  if (p) {
    memset(p, 0, total);
  }
  return p;
}

// Manual free from C runtime code is authoritative: it drops the object
// regardless of rc (the C units own their transients) and releases whatever its
// layout word says it owns, so freeing a tagged transient can never strand its
// children. Foreign pointers fall through to libc free.
void osp_arc_free(void *p) {
  if (!p) {
    return;
  }
  arc_lock();
  uintptr_t *slot = arc_find((uintptr_t)p);
  if (slot) {
    // Immortals (rc < 0 — the empty-list singleton and friends) are returned
    // from many call sites by construction: "authoritative" does not extend to
    // them, or the next caller reads freed memory.
    if (arc_hdr((void *)*slot)->rc >= 0) {
      arc_release_zero_locked(*slot);
    }
    arc_unlock();
    return;
  }
  arc_unlock();
  free(p);
}

void *osp_arc_realloc(void *old, size_t size) {
  if (!old) {
    return osp_arc_malloc(size);
  }
  arc_lock();
  uintptr_t *slot = arc_find((uintptr_t)old);
  size_t oldsize = slot ? arc_hdr((void *)*slot)->size : 0;
  int64_t oldmeta = slot ? arc_hdr((void *)*slot)->meta : (int64_t)OSP_MEM_RAW;
  arc_unlock();
  if (!slot) {
    return realloc(old, size); // foreign block: libc owns it
  }
  void *p = osp_arc_malloc(size);
  if (!p) {
    return NULL;
  }
  if (oldsize) {
    memcpy(p, old, oldsize < size ? oldsize : size);
  }
  // The copy ALIASES old's children, so ownership moves wholesale: the new
  // block takes the layout word, and old's is stripped to RAW before it dies —
  // otherwise its drop walk would decrement children the copy never dup'd.
  osp_mem_set_layout(p, oldmeta);
  arc_lock();
  uintptr_t *dead = arc_find((uintptr_t)old);
  if (dead) {
    arc_hdr((void *)*dead)->meta = (int64_t)OSP_MEM_RAW;
  }
  arc_unlock();
  osp_arc_free(old);
  return p;
}

char *osp_arc_strdup(const char *s) {
  size_t len = strlen(s) + 1;
  char *p = (char *)osp_arc_malloc(len);
  if (p) {
    memcpy(p, s, len);
  }
  return p;
}
