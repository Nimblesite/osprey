// Backend-neutral memory-manager hooks C runtime units may call directly.
// Every memory backend (GC, ARC and ownership) defines
// all of them, so a unit compiled once links against whichever backend the
// program selected. General retain/release are no-ops in default/GC; the
// default backend still reclaims codegen-proved unique values. Implements the
// C-runtime half of [GC-ARC-PERCEUS] under the [MEM-BACKENDS] ABI
// (docs/specs/compiler/0018-MemoryManagement.md).
#ifndef OSPREY_MEMORY_HOOKS_H
#define OSPREY_MEMORY_HOOKS_H

#include <stddef.h>
#include <stdint.h>

// --- layout words -----------------------------------------------------------
// The single source of truth for the `meta` word every backend's
// osp_alloc_tagged takes: low 8 bits a kind, upper 56 a word bitmask (bit i =>
// the pointer-sized slot at body offset sizeof(void*)*i holds a managed pointer). The ARC backend
// stores it in the object header and walks it at drop; default/gc ignore it.
// src/toolchain/crates/osprey-codegen/src/meta.rs emits the same numbers for codegen's own
// allocations — a mismatch corrupts the heap silently, so both sides name
// these constants rather than literals. [MEM-BACKENDS]
#define OSP_MEM_RAW 0             // opaque bytes: no children
#define OSP_MEM_MASK 1            // children at the masked word offsets
#define OSP_MEM_LIST_HDR_PTR 2    // { i64 len, ptr data }: release data[0..len), then data
#define OSP_MEM_LIST_HDR_SCALAR 3 // { i64 len, ptr data }: release data only
#define OSP_MEM_PTR_ARRAY 4       // every 8-byte word in [0, size) is a child
#define OSP_MEM_MASK_DIRECT 5     // MASK whose children codegen PROVED are ARC bodies or NULL: drop reads their headers, no registry probe
#define OSP_MEM_LIST_HDR_EXTERN 6 // { i64 len, ptr data, ... }: run the finalizer over the intact body, then release data only
// Immutable text: an exact allocation of byte length + one C terminator.
// Requires 1..UINT32_MAX bytes and tracked ownership; allocation fails rather
// than returning an untracked value whose full length could not be recovered.
#define OSP_MEM_STRING 7
// An EXTERN body's subtype, bits 8..15 of its meta, picks its finalizer
// ([MEM-BACKENDS-FINALIZERS]): 0 is a plain external header, 1 a GPU buffer
// (gpu_runtime.c), 2 a fiber handle (fiber_lifetime.h).
#define OSP_MEM_EXTERN_SUBTYPE(meta) (((uint64_t)(meta) >> 8) & 0xFF)
#define OSP_MEM_EXTERN_SUBTYPES 4
#define OSP_MEM_GPU_BUFFER (OSP_MEM_LIST_HDR_EXTERN | (1 << 8))
#define OSP_MEM_FIBER_HANDLE (OSP_MEM_LIST_HDR_EXTERN | (2 << 8))
// -1 means no live STRING allocation owns this address (never an empty value).
int64_t osp_mem_string_length(const void *p);

// The bit marking the pointer-sized slot at byte offset `off` (use with offsetof so
// the layout can never drift from the struct).
#define OSP_MEM_WORD(off) ((uint64_t)1 << ((off) / sizeof(void *)))
// A OSP_MEM_MASK layout word over the given word bits.
#define OSP_MEM_LAYOUT(bits) ((int64_t)(((uint64_t)(bits) << 8) | OSP_MEM_MASK))

// Called once at main entry, before any user code. Anchors the memory backend
// object into the link — static archives extract only referenced members, so
// a program with zero heap allocations would otherwise link no backend at all
// — and lets a backend arm exit-time accounting unconditionally: the ARC leak
// sentinel must print (under OSPREY_ARC_DEBUG) even for a program that never
// allocates, or the harness cannot tell "no leaks" from "never audited".
void osp_mem_boot(void);
// Guards the calling thread's stack: recursion past it ends by name (stack_guard.h).
void osp_stack_guard_arm(void);

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

// The finalizer every OSP_MEM_LIST_HDR_EXTERN body of `meta`'s subtype passes
// through on its way out of the heap, before its data is released: a unit whose
// objects own a resource beyond the heap (a GPU buffer's copy on the device, a
// fiber) registers one, and the resource dies with its owner. ARC runs it at
// the owner's last release, ownership at the explicit free, and the tracing GC
// at the sweep that reclaims it. One per subtype, so units never replace each
// other's; ownership drops GPU buffers by their layout whatever is
// registered. A finalizer may run with the heap lock held, so it must not
// allocate, release or take a lock the heap can wait on: it hands its work to
// its unit instead (Boehm, POPL 2003). [MEM-BACKENDS-FINALIZERS]
typedef void (*OspFinalizer)(void *body);
void osp_mem_set_finalizer(int64_t meta, OspFinalizer finalize);

// dup / drop (ARC: registry-probed reference counting; others: no-ops).
void osp_retain(void *o);
void osp_release(void *o);

// Release a reference codegen PROVES is the only one (a fresh, never-bound,
// never-stored value consumed in the expression that created it). ARC performs
// its ordinary release. GC and ownership leave the object intact. Ownership
// lowering calls osp_ownership_free for the last use. The separate symbol lets
// ARC and GC codegen declare LLVM's
// allocator free-pair attributes (allockind("free") / allocptr), so -O2 can
// delete a provably-dead alloc+release pair outright — restoring the
// [MEM-OWNERSHIP] static-free ideal for per-operation Result blocks. Those
// attributes let LLVM assume the pointee is dead after the call, which is only
// true when rc == 1 — hence the uniqueness proof at every emission site.
void osp_release_unique(void *o);

// Ownership-only codegen entry for a proved last use. Walk owned children
// before reclaiming their parent. [MEM-OWNERSHIP-STATIC-SAFETY]
void osp_ownership_free(void *o);

// Mark a shared C-runtime singleton immortal (ARC: rc < 0 — dup/drop skip it
// forever). For objects returned from multiple call sites by construction,
// e.g. the empty-list singleton.
void osp_mem_immortal(void *p);

// Announce that a second thread is about to be able to touch the heap. The
// fiber runtime calls this once, immediately before the pthread_create that
// spawns a real fiber thread. ARC uses it to trip from its single-threaded
// lock-free fast path to a mutexed one; the tracing collector takes its heap lock from then on.
// Idempotent and monotonic. [MEM-BACKENDS]
void osp_mem_notify_multithreaded(void);

// Runtime-internal blocks that hold managed pointers only the runtime can see
// (handler stacks, operation mailboxes, fiber results, channel buffers). The collector scans them as roots until
// they are freed; every other backend treats them as plain malloc.
void *osp_mem_scanned_alloc(size_t size);
void *osp_mem_scanned_realloc(void *block, size_t size);
void osp_mem_scanned_free(void *block);

// Fibers and effect continuations register their threads [MEM-EFFECT-BATON]
// [MEM-GC-SAFEPOINT]. Tracing can stay on while every other registered thread is
// parked, because a parked stack cannot change.
// `prepare` runs on the host before the first body thread exists; `park` names
// the lowest stack address holding the waiter's spilled registers.
void osp_mem_baton_prepare(void);
void osp_mem_baton_thread_enter(void);
void osp_mem_baton_thread_exit(void);
void osp_mem_baton_park(const void *lowest);
void osp_mem_baton_unpark(void);

#endif // OSPREY_MEMORY_HOOKS_H
