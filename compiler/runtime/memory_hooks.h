// Backend-neutral memory-manager hooks C runtime units may call directly.
// Every memory backend (memory_runtime.c, memory_gc.c, memory_arc.c) defines
// all of them, so a unit compiled once links against whichever backend the
// program selected — the default/gc definitions are no-ops. Implements the
// C-runtime half of [GC-ARC-PERCEUS] (docs/plans/0011 phase 2, M4/M5) under
// the [MEM-BACKENDS] ABI (docs/specs/0018).
#ifndef OSPREY_MEMORY_HOOKS_H
#define OSPREY_MEMORY_HOOKS_H

#include <stdint.h>

// --- layout words -----------------------------------------------------------
// The single source of truth for the `meta` word every backend's
// osp_alloc_tagged takes: low 8 bits a kind, upper 56 a word bitmask (bit i =>
// the 8-byte word at body offset 8*i holds a managed pointer). The ARC backend
// stores it in the object header and walks it at drop; default/gc ignore it.
// crates/osprey-codegen/src/meta.rs emits the same numbers for codegen's own
// allocations — a mismatch corrupts the heap silently, so both sides name
// these constants rather than literals. [MEM-BACKENDS]
#define OSP_MEM_RAW 0             // opaque bytes: no children
#define OSP_MEM_MASK 1            // children at the masked word offsets
#define OSP_MEM_LIST_HDR_PTR 2    // { i64 len, ptr data }: release data[0..len), then data
#define OSP_MEM_LIST_HDR_SCALAR 3 // { i64 len, ptr data }: release data only
#define OSP_MEM_PTR_ARRAY 4       // every 8-byte word in [0, size) is a child
#define OSP_MEM_MASK_DIRECT 5     // MASK whose children codegen PROVED are ARC bodies or NULL: drop reads their headers, no registry probe

// The bit marking the 8-byte word at byte offset `off` (use with offsetof so
// the layout can never drift from the struct).
#define OSP_MEM_WORD(off) ((uint64_t)1 << ((off) / 8))
// A OSP_MEM_MASK layout word over the given word bits.
#define OSP_MEM_LAYOUT(bits) ((int64_t)(((uint64_t)(bits) << 8) | OSP_MEM_MASK))

// Layout-carrying allocation (every backend defines it; default/gc ignore meta).
void *osp_alloc_tagged(int64_t size, int64_t meta);

// Layout-carrying allocation the caller PROMISES to fully initialize before the
// block can be dropped: every masked word is stored, so the ARC backend skips
// the drop-safety pre-zeroing osp_alloc_tagged does. default/gc never zero, so
// for them this is exactly osp_alloc_tagged. Constructor blocks qualify — they
// store the tag and every field in the same region, so nothing walks the block
// while a masked word is still garbage. [GC-ARC-PERCEUS]
void *osp_alloc_tagged_noinit(int64_t size, int64_t meta);

// Stamp an already-allocated object with its layout word. The value-container
// units allocate through osp_arc_shim.h's calloc redirect — which zeroes, and
// zeroed slack words are what makes a PTR_ARRAY walk safe — so they tag after
// the fact rather than switching to osp_alloc_tagged. No-op off ARC.
void osp_mem_set_layout(void *p, int64_t meta);

// dup / drop (ARC: registry-probed reference counting; others: no-ops).
void osp_retain(void *o);
void osp_release(void *o);

// Release a reference codegen PROVES is the only one (a fresh, never-bound,
// never-stored value consumed in the expression that created it). Runtime
// behaviour is identical to osp_release; the separate symbol exists so
// codegen can declare it with LLVM's allocator free-pair attributes
// (allockind("free") / allocptr) and -O2 can delete a provably-dead
// alloc+release pair outright — restoring the [MEM-OWNERSHIP] static-free
// ideal for per-operation Result blocks. Those attributes let LLVM assume
// the pointee is dead after the call, which is only true when rc == 1 —
// hence the uniqueness proof requirement on every emission site.
void osp_release_unique(void *o);

// Mark a shared C-runtime singleton immortal (ARC: rc < 0 — dup/drop skip it
// forever). For objects returned from multiple call sites by construction,
// e.g. the empty-list singleton.
void osp_mem_immortal(void *p);

// Announce that a second thread is about to be able to touch the heap. The
// fiber runtime calls this once, immediately before the pthread_create that
// spawns a real fiber thread. ARC uses it to trip from its single-threaded
// lock-free fast path to a mutexed one; default/gc are unsynchronized no-ops.
// Idempotent and monotonic. [MEM-BACKENDS]
void osp_mem_notify_multithreaded(void);

#endif // OSPREY_MEMORY_HOOKS_H
