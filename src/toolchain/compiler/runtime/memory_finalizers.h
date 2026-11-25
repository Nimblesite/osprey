// One finalizer per external subtype, shared by every memory backend
// [MEM-BACKENDS-FINALIZERS]. A subtype names a kind of OSP_MEM_LIST_HDR_EXTERN
// body (a GPU buffer, a fiber handle); each unit that owns such bodies
// registers its own, so one unit's registration can never replace another's.
// The table is atomic: a finalizer is registered before the first body of its
// subtype exists, and may be read by a collecting thread at any time after.
#ifndef OSPREY_MEMORY_FINALIZERS_H
#define OSPREY_MEMORY_FINALIZERS_H

#include "memory_hooks.h"
#include <stdatomic.h>

static _Atomic(OspFinalizer) g_extern_finalizers[OSP_MEM_EXTERN_SUBTYPES];

void osp_mem_set_finalizer(int64_t meta, OspFinalizer finalize) {
  uint64_t subtype = OSP_MEM_EXTERN_SUBTYPE(meta);
  if (subtype < OSP_MEM_EXTERN_SUBTYPES) {
    atomic_store_explicit(&g_extern_finalizers[subtype], finalize, memory_order_release);
  }
}

// Runs the finalizer registered for `subtype`, if there is one.
static void finalize_subtype(uint64_t subtype, void *body) {
  OspFinalizer finalize = subtype < OSP_MEM_EXTERN_SUBTYPES
                              ? atomic_load_explicit(&g_extern_finalizers[subtype], memory_order_acquire)
                              : NULL;
  if (finalize) {
    finalize(body);
  }
}

#endif
