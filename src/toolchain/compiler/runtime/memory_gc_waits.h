// Fibers blocked for ever [CONCURRENCY-DEADLOCK], included once by memory_gc.c
// after its baton and scanned-block types, whose lists it reads.
//
// A fiber waiting on a channel can only be woken by an operation on that
// channel, and an operation needs the channel's handle. So a waiting fiber is
// live only if something that can still run reaches the handle. Saioc, Lee,
// Møller and Chabbi ("Dynamic Partial Deadlock Detection and Recovery via
// Garbage Collection", ASPLOS 2025) find the others with the collector's own
// marking: trace from every root but the waiting fibers' stacks and records,
// then add each waiting fiber whose channel handle was reached and trace
// again, until none is added. Whatever is left waits on a channel only waiting
// fibers can reach: it is blocked for ever.
//
// Reachability over-approximates what a program can still name, and scanning
// conservatively over-approximates reachability, so a fiber is never reported
// while it could be woken; one that is stuck may go unreported. Nothing a stuck
// fiber holds is freed: its stack and record are traced last, like any root.
// src/toolchain/benchmarks/proof/tools/FiberDeadlock.lean proves the rule with
// Osprey's proof kernel.
#ifndef OSPREY_GC_WAITS_H
#define OSPREY_GC_WAITS_H

static OspChannelWait *g_waits;        // every registered wait, under the heap lock
static _Atomic size_t g_stuck_waits;   // those a collection found blocked for ever

void osp_mem_wait_enter(OspChannelWait *wait) {
  heap_lock();
  wait->baton = gc_my_baton();
  wait->reached = false;
  wait->stuck = false;
  wait->next = g_waits;
  g_waits = wait;
  heap_unlock();
}

void osp_mem_wait_exit(OspChannelWait *wait) {
  heap_lock();
  for (OspChannelWait **link = &g_waits; *link; link = &(*link)->next) {
    if (*link == wait) {
      *link = wait->next;
      break;
    }
  }
  heap_unlock();
}

size_t osp_mem_stuck_waits(void) {
  return atomic_load_explicit(&g_stuck_waits, memory_order_relaxed);
}

// Whether a wait not yet reached defers this stack or this scanned block.
static bool gc_wait_defers(const void *baton, const void *record) {
  for (const OspChannelWait *wait = g_waits; wait; wait = wait->next) {
    if (!wait->reached && (wait->baton == baton || (record && wait->record == record))) {
      return true;
    }
  }
  return false;
}

// Traces what a waiting fiber holds: its parked stack and its record.
static void gc_scan_wait(OspChannelWait *wait) {
  const OspGcBaton *baton = wait->baton;
  if (baton) {
    gc_scan_range(baton->parked_at, baton->base);
  }
  if (wait->record) {
    const OspGcScanned *block = (const OspGcScanned *)wait->record - 1;
    gc_scan_range((uintptr_t)wait->record, (uintptr_t)wait->record + block->size);
  }
  wait->reached = true;
}

static bool gc_marked(const void *block) {
  const OspGcMeta *meta = gc_lookup((uintptr_t)block);
  return meta && (meta->mark & GC_MARKED);
}

// Adds every waiting fiber whose channel handle was reached; true if any was.
static bool gc_wake_reached_waits(void) {
  bool added = false;
  for (OspChannelWait *wait = g_waits; wait; wait = wait->next) {
    if (!wait->reached && gc_marked(wait->handle)) {
      gc_scan_wait(wait);
      added = true;
    }
  }
  return added;
}

// The waits still unreached are blocked for ever: counted once, then traced so
// nothing they hold is freed under them.
static void gc_settle_stuck_waits(void) {
  for (OspChannelWait *wait = g_waits; wait; wait = wait->next) {
    if (!wait->reached) {
      if (!wait->stuck) {
        wait->stuck = true;
        atomic_fetch_add_explicit(&g_stuck_waits, 1, memory_order_relaxed);
      }
      gc_scan_wait(wait);
    }
  }
}

// Every wait starts a collection deferred.
static void gc_defer_waits(void) {
  for (OspChannelWait *wait = g_waits; wait; wait = wait->next) {
    wait->reached = false;
  }
}

#endif
