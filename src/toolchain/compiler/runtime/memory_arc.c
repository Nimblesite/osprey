// Perceus ARC backend for Osprey — the swappable `@osp_alloc` implementation
// selected at link time by `osprey --memory=arc`. Implements [MEM-BACKENDS]
// and [GC-ARC-PERCEUS] (docs/specs/compiler/0018-MemoryManagement.md).
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
// immortal/persistent objects that dup/drop skip.
//
// Pointer slots at runtime also carry rodata literals, static C strings,
// foreign malloc/strdup memory,
// borrowed FFI pointers, and NULL. osp_retain/osp_release therefore probe an
// exact live-allocation registry FIRST; WASM uses a bitmap mirror for the hot
// membership check. A miss means
// "not ours" and is a safe no-op. No IR special-casing, no FFI annotations —
// non-ARC pointers are unmanaged by construction.
//
// Concurrency scope: operations use a lock-free fast path until the fiber
// runtime announces its first pthread, then use one mutex across fiber threads
// [MEM-FIBER-ISOLATION].

#define OSP_HEAP_TAG "arc"
#define OSP_HEAP_DEBUG_ENV "OSPREY_ARC_DEBUG"
// A reference count reaching zero ends the block; immortals (rc < 0) never do.
#define OSP_HEAP_DROP_LAST(h) ((h)->rc >= 0 && --(h)->rc == 0)

#include "memory_tracked_heap.h"

<<<<<<< HEAD
static void (*g_finalizer)(void *body);
void osp_mem_set_finalizer(void (*finalize)(void *body)) { g_finalizer = finalize; }
static void finalize_external(void *body) {
  if (g_finalizer) g_finalizer(body);
=======
#include "memory_finalizers.h"
static void finalize_external(void *body) {
  finalize_subtype(OSP_MEM_EXTERN_SUBTYPE(arc_hdr(body)->meta), body);
>>>>>>> origin/fixes
}

// dup — probe first: only live ARC objects with a non-negative rc count.
void osp_retain(void *o) {
  if (!o) {
    return;
  }
  heap_lock();
  OspArcHdr *h = arc_lookup_body((uintptr_t)o);
  if (h && h->rc >= 0) {
    h->rc++;
  }
  heap_unlock();
}

void osp_release_unique(void *body) { tracked_drop(body); }
