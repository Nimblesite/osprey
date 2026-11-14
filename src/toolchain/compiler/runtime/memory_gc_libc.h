// The malloc-shaped entry points osp_gc_shim.h maps a container unit's
// allocations onto, included once by memory_gc.c, whose heap lock and table it
// shares. Value-container runtime units (list/map) are compiled against
// osp_gc_shim.h so their nodes live in the managed heap and the collector scans
// the boxed Osprey values they hold. free is a no-op — the collector owns
// reclamation.
#ifndef OSPREY_GC_LIBC_H
#define OSPREY_GC_LIBC_H

void *osp_gc_malloc(size_t size) {
  heap_lock();
  void *p = gc_managed_alloc(size, 0);
  heap_unlock();
  return p;
}

void *osp_gc_calloc(size_t n, size_t size) {
  size_t total = n * size;
  if (n != 0 && total / n != size) {
    return NULL; // overflow
  }
  heap_lock();
  void *p = gc_managed_alloc(total, 0);
  heap_unlock();
  if (p) {
    memset(p, 0, total);
  }
  return p;
}

void *osp_gc_realloc(void *old, size_t size) {
  if (!old) {
    return osp_gc_malloc(size);
  }
  heap_lock();
  OspGcEnt *e = gc_lookup((uintptr_t)old);
  size_t oldsize = e ? e->size : 0;
  void *p = gc_managed_alloc(size, 0); // `old` stays reachable on the caller frame
  heap_unlock();
  if (p && oldsize) {
    memcpy(p, old, oldsize < size ? oldsize : size);
  }
  return p; // old block is reclaimed by a later collection
}

void osp_gc_free(void *p) { (void)p; }

#endif
