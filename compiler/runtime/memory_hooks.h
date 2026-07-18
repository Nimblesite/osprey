// Backend-neutral memory-manager hooks C runtime units may call directly.
// Every memory backend (memory_runtime.c, memory_gc.c, memory_arc.c) defines
// all of them, so a unit compiled once links against whichever backend the
// program selected — the default/gc definitions are no-ops. Implements the
// C-runtime half of [GC-ARC-PERCEUS] (docs/plans/0011 phase 2, M4/M5) under
// the [MEM-BACKENDS] ABI (docs/specs/0018).
#ifndef OSPREY_MEMORY_HOOKS_H
#define OSPREY_MEMORY_HOOKS_H

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

#endif // OSPREY_MEMORY_HOOKS_H
