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

#include <pthread.h>
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

// Layout-word kinds (low 8 bits of meta) — must match osprey-codegen's
// emission (plan 0011 phase 2, Amendment 1).
enum {
  OSP_ARC_RAW = 0,             // opaque bytes: no children
  OSP_ARC_MASK = 1,            // children at the masked word offsets
  OSP_ARC_LIST_HDR_PTR = 2,    // { i64 len, i8* data }: release data[0..len), then data
  OSP_ARC_LIST_HDR_SCALAR = 3, // { i64 len, i8* data }: release data only
};

#define OSP_ARC_KIND(meta) ((int)((meta) & 0xFF))
#define OSP_ARC_MASK_BITS(meta) (((uint64_t)(meta)) >> 8)

static OspArcHdr *arc_hdr(void *body) {
  return (OspArcHdr *)(void *)((char *)body - OSP_ARC_HDR);
}

// --- live-allocation registry (open addressing, tombstone deletion) -----------

#define OSP_ARC_INIT_CAP ((size_t)1u << 14)
#define OSP_ARC_TOMB ((uintptr_t)1)

static pthread_mutex_t g_lock = PTHREAD_MUTEX_INITIALIZER;
static uintptr_t *g_tab = NULL; // body addresses; 0 = empty, 1 = tombstone
static size_t g_cap = 0;
static size_t g_used = 0; // live + tombstones (grow trigger)
static size_t g_live = 0; // live objects (leak stats)
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

// Rebuild at double capacity (also compacts tombstones away).
static int arc_grow(void) {
  size_t cap = g_cap ? g_cap * 2 : OSP_ARC_INIT_CAP;
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
  g_used = g_live;
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
}

static void arc_remove(uintptr_t *slot) {
  *slot = OSP_ARC_TOMB;
  g_live--;
}

// --- leak accounting (OSPREY_ARC_DEBUG=1) --------------------------------------

static void arc_report_leaks(void) {
  pthread_mutex_lock(&g_lock);
  // stderr only: stdout must stay byte-identical under [MEM-BACKENDS].
  fprintf(stderr, "[osp-arc] exit: %zu live objects, %zu KiB\n", g_live,
          g_live_bytes / 1024);
  pthread_mutex_unlock(&g_lock);
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

// Header'd, registered allocation. Callers hold g_lock.
static void *arc_alloc_locked(size_t size, int64_t meta) {
  arc_arm_debug();
  OspArcHdr *h = (OspArcHdr *)malloc(OSP_ARC_HDR + (size ? size : 1));
  if (!h) {
    return NULL;
  }
  h->meta = meta;
  h->rc = 1;
  h->size = (uint32_t)size;
  void *body = (char *)h + OSP_ARC_HDR;
  arc_insert((uintptr_t)body);
  g_live_bytes += size;
  return body;
}

static void arc_free_locked(uintptr_t *slot) {
  void *body = (void *)*slot;
  OspArcHdr *h = arc_hdr(body);
  g_live_bytes -= h->size;
  arc_remove(slot);
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

static void arc_drop_masked(char *body, uint32_t size, uint64_t mask) {
  for (unsigned i = 0; mask != 0 && (size_t)(i + 1) * 8 <= size; i++, mask >>= 1) {
    if (mask & 1) {
      void *child;
      memcpy(&child, body + (size_t)i * 8, sizeof(child));
      arc_drop_child(child);
    }
  }
}

// { i64 len, i8* data }: release elements (PTR kind only), then the array.
static void arc_drop_list_hdr(char *body, uint32_t size, int elems_are_ptrs) {
  if (size < 16) {
    return;
  }
  int64_t len;
  char *data;
  memcpy(&len, body, sizeof(len));
  memcpy(&data, body + 8, sizeof(data));
  if (data && elems_are_ptrs) {
    for (int64_t j = 0; j < len; j++) {
      void *elem;
      memcpy(&elem, data + (size_t)j * 8, sizeof(elem));
      arc_drop_child(elem);
    }
  }
  arc_drop_child(data);
}

// Free `slot`'s object and everything transitively released by it. Holds g_lock.
static void arc_release_zero_locked(uintptr_t *slot) {
  g_wl_top = 0;
  arc_wl_push(*slot);
  while (g_wl_top > 0) {
    uintptr_t body = g_wl[--g_wl_top];
    uintptr_t *s = arc_find(body);
    if (!s) {
      continue;
    }
    OspArcHdr *h = arc_hdr((void *)body);
    int kind = OSP_ARC_KIND(h->meta);
    if (kind == OSP_ARC_MASK) {
      arc_drop_masked((char *)body, h->size, OSP_ARC_MASK_BITS(h->meta));
    } else if (kind == OSP_ARC_LIST_HDR_PTR || kind == OSP_ARC_LIST_HDR_SCALAR) {
      arc_drop_list_hdr((char *)body, h->size, kind == OSP_ARC_LIST_HDR_PTR);
    }
    arc_free_locked(s);
  }
}

// --- public ABI ------------------------------------------------------------------
// Implements the [MEM-BACKENDS] C interface (docs/specs/0018 §Custom Managers).

void *osp_alloc_tagged(int64_t size, int64_t meta) {
  pthread_mutex_lock(&g_lock);
  void *p = arc_alloc_locked(size > 0 ? (size_t)size : 0, meta);
  pthread_mutex_unlock(&g_lock);
  return p;
}

void *osp_alloc(int64_t size) { return osp_alloc_tagged(size, OSP_ARC_RAW); }

// dup — probe first: only live ARC objects with a non-negative rc count.
void osp_retain(void *o) {
  if (!o) {
    return;
  }
  pthread_mutex_lock(&g_lock);
  uintptr_t *slot = arc_find((uintptr_t)o);
  if (slot) {
    OspArcHdr *h = arc_hdr((void *)*slot);
    if (h->rc >= 0) {
      h->rc++;
    }
  }
  pthread_mutex_unlock(&g_lock);
}

// drop — probe first; on rc==0 run the kind/mask worklist drop.
void osp_release(void *o) {
  if (!o) {
    return;
  }
  pthread_mutex_lock(&g_lock);
  uintptr_t *slot = arc_find((uintptr_t)o);
  if (slot) {
    OspArcHdr *h = arc_hdr((void *)*slot);
    if (h->rc > 0 && --h->rc == 0) {
      arc_release_zero_locked(slot);
    }
  }
  pthread_mutex_unlock(&g_lock);
}

// Full-collection hook: nothing to do — acyclic naive RC is already complete
// (Bacon/Cheng/Rajan; TR §2.2).
void osp_collect(void) {}

// Immortalize a shared C-runtime singleton (memory_hooks.h): rc < 0 makes
// every later dup/drop skip it, so alias returns of e.g. the empty-list
// singleton can never free it. Foreign pointers probe-miss (no-op).
void osp_mem_immortal(void *p) {
  if (!p) {
    return;
  }
  pthread_mutex_lock(&g_lock);
  uintptr_t *slot = arc_find((uintptr_t)p);
  if (slot) {
    arc_hdr((void *)*slot)->rc = -1;
  }
  pthread_mutex_unlock(&g_lock);
}

// --- allocation shim for the C runtime units (osp_arc_shim.h) ---------------------
// Value-producing units (list/map/string/json) are recompiled with their libc
// allocation calls redirected here, so runtime-minted strings and container
// nodes carry headers and live in the registry (kind RAW until milestone M4
// teaches the container ops their own dup/drop).

void *osp_arc_malloc(size_t size) {
  pthread_mutex_lock(&g_lock);
  void *p = arc_alloc_locked(size, OSP_ARC_RAW);
  pthread_mutex_unlock(&g_lock);
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
// regardless of rc (the C units own their transients). Foreign pointers fall
// through to libc free.
void osp_arc_free(void *p) {
  if (!p) {
    return;
  }
  pthread_mutex_lock(&g_lock);
  uintptr_t *slot = arc_find((uintptr_t)p);
  if (slot) {
    arc_free_locked(slot);
    pthread_mutex_unlock(&g_lock);
    return;
  }
  pthread_mutex_unlock(&g_lock);
  free(p);
}

void *osp_arc_realloc(void *old, size_t size) {
  if (!old) {
    return osp_arc_malloc(size);
  }
  pthread_mutex_lock(&g_lock);
  uintptr_t *slot = arc_find((uintptr_t)old);
  size_t oldsize = slot ? arc_hdr((void *)*slot)->size : 0;
  pthread_mutex_unlock(&g_lock);
  if (!slot) {
    return realloc(old, size); // foreign block: libc owns it
  }
  void *p = osp_arc_malloc(size);
  if (p && oldsize) {
    memcpy(p, old, oldsize < size ? oldsize : size);
  }
  if (p) {
    osp_arc_free(old);
  }
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
