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

// dup — probe first: only live ARC objects with a non-negative rc count.
void osp_retain(void *o) {
  if (!o) {
    return;
  }
  heap_lock();
  OspArcHdr *h = arc_lookup_body((uintptr_t)o);
  if (h) {
    if (h->rc >= 0) {
      h->rc++;
    }
  }
  heap_unlock();
}

// drop — probe first; on rc==0 run the kind/mask worklist drop.
void osp_release(void *o) {
  if (!o) {
    return;
  }
  heap_lock();
  OspArcHdr *h = arc_lookup_body((uintptr_t)o);
  if (h) {
    if (h->rc > 0 && --h->rc == 0) {
      arc_release_zero_locked((uintptr_t)o);
    }
  }
  heap_unlock();
}

// Codegen-proved-unique drop (memory_hooks.h): the caller proved rc == 1, so
// this is exactly osp_release — the distinct symbol only exists to carry
// LLVM's free-pair attributes so -O2 can delete non-escaping alloc+release
// pairs before they ever reach the runtime.
void osp_release_unique(void *o) { osp_release(o); }
