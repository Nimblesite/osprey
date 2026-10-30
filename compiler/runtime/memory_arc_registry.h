// ARC-private exact membership; callers hold the ARC lock.
#ifndef OSPREY_MEMORY_ARC_REGISTRY_H
#define OSPREY_MEMORY_ARC_REGISTRY_H

static uintptr_t *g_tab = NULL; // body addresses; 0 = empty, 1 = tombstone
static size_t g_cap = 0;
static size_t g_used =
    0; // non-empty slots: live + pooled + tombstones (grow trigger)
static size_t g_present =
    0; // non-tombstone slots: live + pooled (table sizing)

#if defined(__wasm32__)
#include <stddef.h>
_Static_assert(_Alignof(max_align_t) >= 8 && OSP_ARC_HDR % 8 == 0,
               "ARC membership requires aligned malloc bodies");
#define ARC_BITMAP_PAGE_SHIFT 16
#define ARC_BITMAP_SLOT_SHIFT 3
#define ARC_BITMAP_PAGE_BYTES 1024
static unsigned char *g_arc_bitmap[65536];
static int g_arc_bitmap_enabled = 1;

static unsigned char *arc_bitmap_page(uintptr_t body, int create) {
  size_t page = body >> ARC_BITMAP_PAGE_SHIFT;
  if (!g_arc_bitmap[page] && create) {
    g_arc_bitmap[page] = calloc(ARC_BITMAP_PAGE_BYTES, 1);
    if (!g_arc_bitmap[page])
      g_arc_bitmap_enabled = 0;
  }
  return g_arc_bitmap[page];
}

static void arc_bitmap_update(uintptr_t body, int present) {
  if (!g_arc_bitmap_enabled)
    return;
  unsigned char *page = arc_bitmap_page(body, present);
  if (!page)
    return;
  size_t slot = (body & 65535) >> ARC_BITMAP_SLOT_SHIFT;
  unsigned char mask = (unsigned char)(1u << (slot & 7));
  page[slot >> 3] = present ? (page[slot >> 3] | mask)
                            : (page[slot >> 3] & (unsigned char)~mask);
}

static int arc_bitmap_contains(uintptr_t body) {
  if (body & 7)
    return 0;
  unsigned char *page = arc_bitmap_page(body, 0);
  size_t slot = (body & 65535) >> ARC_BITMAP_SLOT_SHIFT;
  return page && (page[slot >> 3] & (1u << (slot & 7)));
}
#else
static void arc_bitmap_update(uintptr_t body, int present) {
  (void)body;
  (void)present;
}
#endif

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
  g_used =
      g_present; // rebuild dropped every tombstone; only present slots remain
  return 1;
}

static void arc_insert(uintptr_t body) {
  if (g_cap == 0 || (g_used + 1) * 4 >= g_cap * 3) {
    if (!arc_grow()) {
      // Broken-code quarantine: returning an untracked body would prevent
      // release.
      fputs("osprey: ARC allocation registry exhausted\n", stderr);
      abort();
    }
  }
  arc_place(g_tab, g_cap, body);
  g_used++;
  g_present++;
  arc_bitmap_update(body, 1);
}

static void arc_remove(uintptr_t *slot) {
  uintptr_t body = *slot;
  *slot = OSP_ARC_TOMB;
  g_present--;
  arc_bitmap_update(body, 0);
}

static OspArcHdr *arc_lookup_body(uintptr_t body) {
#if defined(__wasm32__)
  if (g_arc_bitmap_enabled)
    return arc_bitmap_contains(body) ? arc_hdr((void *)body) : NULL;
#endif
  uintptr_t *slot = arc_find(body);
  return slot ? arc_hdr((void *)*slot) : NULL;
}

#endif
