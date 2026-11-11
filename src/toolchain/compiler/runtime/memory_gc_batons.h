// The public face of baton threads and scanned blocks [MEM-EFFECT-BATON],
// included once by memory_gc.c after its safepoint, whose static state it
// shares. A registered thread parks with the lowest live address of its stack
// so a collection scans exactly what it may still read; a scanned block is a
// malloc'd region traced as roots until it is freed.
#ifndef OSPREY_GC_BATONS_H
#define OSPREY_GC_BATONS_H

void *osp_mem_scanned_alloc(size_t size) {
  OspGcScanned *s = (OspGcScanned *)malloc(sizeof(OspGcScanned) + size);
  if (s == NULL) {
    return NULL;
  }
  memset(s, 0, sizeof(OspGcScanned) + size);
  s->size = size;
  heap_lock();
  s->next = g_scanned;
  if (g_scanned) {
    g_scanned->prev = s;
  }
  g_scanned = s;
  heap_unlock();
  return s + 1;
}

void osp_mem_scanned_free(void *block) {
  if (block == NULL) {
    return;
  }
  OspGcScanned *s = (OspGcScanned *)block - 1;
  heap_lock();
  if (s->prev) {
    s->prev->next = s->next;
  } else {
    g_scanned = s->next;
  }
  if (s->next) {
    s->next->prev = s->prev;
  }
  heap_unlock();
  free(s);
}

void *osp_mem_scanned_realloc(void *block, size_t size) {
  void *grown = osp_mem_scanned_alloc(size);
  if (grown != NULL && block != NULL) {
    size_t old = ((OspGcScanned *)block - 1)->size;
    memcpy(grown, block, old < size ? old : size);
    osp_mem_scanned_free(block);
  }
  return grown;
}

// The host fixes the initial thread before any body thread can allocate first.
void osp_mem_baton_prepare(void) {
  heap_lock();
  if (!g_init) {
    gc_init_main();
  }
  heap_unlock();
}

void osp_mem_baton_thread_enter(void) {
#if defined(__APPLE__) || defined(_WIN32) || (defined(__linux__) && defined(__GLIBC__))
  OspGcBaton *self = (OspGcBaton *)calloc(1, sizeof(OspGcBaton));
  if (self == NULL) {
    return; // Unregistered: its first allocation disables collection, which is safe.
  }
#ifndef OSP_SYMBOLIC
  self->base = gc_stack_base();
#endif
  heap_lock();
  self->next = g_batons;
  g_batons = self;
  g_self_baton = self;
  heap_unlock();
#endif
  // Elsewhere its stack bounds can't be read, so it stays unregistered: its first
  // allocation turns collection off, which is safe.
}

void osp_mem_baton_thread_exit(void) {
  OspGcBaton *self = g_self_baton;
  if (self == NULL) {
    return;
  }
  heap_lock();
  for (OspGcBaton **link = &g_batons; *link; link = &(*link)->next) {
    if (*link == self) {
      *link = self->next;
      break;
    }
  }
  g_self_baton = NULL;
  gc_wake_safepoint();
  heap_unlock();
  free(self);
}

void osp_mem_baton_park(const void *lowest) {
  heap_lock();
  OspGcBaton *self = gc_my_baton();
  if (self != NULL) {
    self->parked_at = (uintptr_t)lowest;
  }
  gc_wake_safepoint();
  heap_unlock();
}

void osp_mem_baton_unpark(void) {
  heap_lock();
  OspGcBaton *self = gc_my_baton();
  if (self != NULL) {
    self->parked_at = 0;
  }
  heap_unlock();
}

#endif
