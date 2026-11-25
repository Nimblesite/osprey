// How long a fiber lives [CONCURRENCY-FIBER-LIFETIME], included once by
// fiber_runtime.c after the Fiber type.
//
// A fiber is a future (Halstead, Multilisp, TOPLAS 1985) whose storage is
// reclaimed by reachability, as Baker and Hewitt proposed for processes (MIT AI
// Memo 454, 1977): once nothing can await it and it has finished, it is
// garbage. Every party that can still touch a Fiber holds one count of
// `holders`:
//
// - the program's handle, a managed OSP_MEM_FIBER_HANDLE block the memory
//   backend reclaims like any value: ARC at its last release, the tracing GC at
//   the sweep after it becomes unreachable, ownership at its explicit free;
// - the runner, from spawn until the body has finished: its thread, the stack
//   running it in place, or the deterministic queue holding it to run later;
// - each await and probe, for as long as it reads the fiber.
//
// The last holder to let go frees the fiber, its result root and its
// synchronisation. Spawn starts with two holders, the handle and the runner.
// src/toolchain/benchmarks/proof/tools/FiberLifetime.lean proves, with Osprey's
// proof kernel, that this count frees a fiber exactly when its last holder lets
// go and that no holder ever reads a freed one.
//
// A handle's finalizer runs inside the memory backend, which may hold its heap
// lock or have stopped the world, so it takes no lock, allocates nothing and
// releases nothing (Boehm, "Destructors, finalizers, and synchronization",
// POPL 2003). It only pushes the fiber onto a lock-free list, a Treiber stack
// (IBM RJ 5118, 1986) whose one consumer takes the whole list at once, so no
// pop can meet a recycled node. The runtime drops those holds outside the
// collector, at the next spawn and at the end of main.
#ifndef OSPREY_FIBER_LIFETIME_H
#define OSPREY_FIBER_LIFETIME_H

// The program's handle: an external header whose length and data are empty,
// so every backend releases nothing of it but runs its finalizer.
typedef struct FiberHandle {
  int64_t length;
  void *data;
  Fiber *fiber;
} FiberHandle;

// Fibers whose handles died, waiting for the runtime to drop their holds.
static _Atomic(Fiber *) dead_handles;

static void fiber_hold(Fiber *fiber) {
  atomic_fetch_add_explicit(&fiber->holders, 1, memory_order_relaxed);
}

// Frees everything a fiber still owns. Only the last holder gets here.
static void fiber_free(Fiber *fiber) {
  if (fiber->result_owned && fiber->result_managed) {
    osp_release((void *)(uintptr_t)fiber->result);
  }
  osp_release(fiber->env); // never ran: the capture cell is still the runtime's
  if (fiber->handler_snapshot != NULL) {
    osp_mem_scanned_free(fiber->handler_snapshot);
  }
  pthread_mutex_destroy(&fiber->mutex);
  pthread_cond_destroy(&fiber->cond);
  osp_mem_scanned_free(fiber);
}

// Lets go of one hold; the last one frees the fiber. The release order makes
// every earlier holder's writes visible to the one that frees.
void fiber_release(Fiber *fiber) {
  if (fiber && atomic_fetch_sub_explicit(&fiber->holders, 1, memory_order_acq_rel) == 1) {
    fiber_free(fiber);
  }
}

// The handle's finalizer: hands its hold to the runtime, touching nothing else.
// A handle whose spawn was refused holds no fiber.
static void fiber_handle_finalize(void *body) {
  Fiber *fiber = ((FiberHandle *)body)->fiber;
  if (!fiber) {
    return;
  }
  Fiber *head = atomic_load_explicit(&dead_handles, memory_order_relaxed);
  do {
    fiber->dead_next = head;
  } while (!atomic_compare_exchange_weak_explicit(&dead_handles, &head, fiber,
                                                  memory_order_release, memory_order_relaxed));
}

// Drops the hold of every handle that died since the last call.
static void fiber_reclaim(void) {
  Fiber *fiber = atomic_exchange_explicit(&dead_handles, NULL, memory_order_acquire);
  while (fiber) {
    Fiber *next = fiber->dead_next;
    fiber_release(fiber);
    fiber = next;
  }
}

__attribute__((constructor)) static void fiber_lifetime_register(void) {
  osp_mem_set_finalizer(OSP_MEM_FIBER_HANDLE, fiber_handle_finalize);
}

// The fiber a handle holds, with a hold of the caller's own: a tracing
// collector may reclaim the handle while the caller still reads the fiber.
static Fiber *fiber_of_handle(const void *handle) {
  Fiber *fiber = handle ? ((const FiberHandle *)handle)->fiber : NULL;
  if (fiber) {
    fiber_hold(fiber);
  }
  return fiber;
}

#endif
