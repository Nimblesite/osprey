// How long a fiber or a channel lives [CONCURRENCY-FIBER-LIFETIME]
// [CONCURRENCY-CHANNEL-LIFETIME], included once by fiber_runtime.c before the
// Fiber and Channel types, which each begin with a Held.
//
// A fiber is a future (Halstead, Multilisp, TOPLAS 1985) and a channel a
// mailbox; both are reclaimed by reachability, as Baker and Hewitt proposed for
// processes (MIT AI Memo 454, 1977): once nothing can name one again, it is
// garbage. Every party that can still touch one holds one count of `holders`:
//
// - the program's handle, a managed OSP_MEM_FIBER_HANDLE or
//   OSP_MEM_CHANNEL_HANDLE block the memory backend reclaims like any value:
//   ARC at its last release, the tracing GC at the sweep after it becomes
//   unreachable, ownership at its explicit free;
// - a fiber's runner, from spawn until the body has finished: its thread, the
//   stack running it in place, or the deterministic queue holding it to run
//   later;
// - each await, probe, send, receive and select, for as long as it reads it.
//
// The last holder to let go frees it through its own `free`. Spawn starts with
// two holders, the handle and the runner; a channel with one, its handle.
// src/toolchain/benchmarks/proof/tools/FiberLifetime.lean proves, with Osprey's
// proof kernel, that this count frees exactly when its last holder lets go and
// that no holder ever reads a freed one.
//
// A handle's finalizer runs inside the memory backend, which may hold its heap
// lock or have stopped the world, so it takes no lock, allocates nothing and
// releases nothing (Boehm, "Destructors, finalizers, and synchronization",
// POPL 2003). It only pushes the held object onto a lock-free list, a Treiber
// stack (IBM RJ 5118, 1986) whose one consumer takes the whole list at once, so
// no pop can meet a recycled node. The runtime drops those holds outside the
// collector, at the next spawn or channel and at the end of main.
#ifndef OSPREY_HANDLE_LIFETIME_H
#define OSPREY_HANDLE_LIFETIME_H

// The counted part every fiber and channel begins with.
typedef struct Held {
  _Atomic int64_t holders;
  struct Held *dead_next;     // the dead-handle list
  void (*free)(struct Held *); // frees the object this Held begins
} Held;

// The program's handle: an external header whose length and data are empty,
// so every backend releases nothing of it but runs its finalizer.
typedef struct HeldHandle {
  int64_t length;
  void *data;
  Held *held;
} HeldHandle;

// Objects whose handles died, waiting for the runtime to drop their holds.
static _Atomic(Held *) dead_handles;

static void held_init(Held *held, int64_t holders, void (*free_held)(Held *)) {
  atomic_init(&held->holders, holders);
  held->dead_next = NULL;
  held->free = free_held;
}

static void held_hold(Held *held) {
  atomic_fetch_add_explicit(&held->holders, 1, memory_order_relaxed);
}

// Lets go of one hold; the last one frees. The release order makes every
// earlier holder's writes visible to the one that frees.
static void held_release(Held *held) {
  if (held && atomic_fetch_sub_explicit(&held->holders, 1, memory_order_acq_rel) == 1) {
    held->free(held);
  }
}

// The handle's finalizer: hands its hold to the runtime, touching nothing else.
// A handle whose creation was refused holds nothing.
static void held_handle_finalize(void *body) {
  Held *held = ((HeldHandle *)body)->held;
  if (!held) {
    return;
  }
  Held *head = atomic_load_explicit(&dead_handles, memory_order_relaxed);
  do {
    held->dead_next = head;
  } while (!atomic_compare_exchange_weak_explicit(&dead_handles, &head, held,
                                                  memory_order_release, memory_order_relaxed));
}

// Drops the hold of every handle that died since the last call, and of those
// that die as it does: freeing a fiber drops the handles its captures held.
static void held_reclaim(void) {
  Held *held = atomic_exchange_explicit(&dead_handles, NULL, memory_order_acquire);
  while (held) {
    Held *next = held->dead_next;
    held_release(held);
    held = next ? next : atomic_exchange_explicit(&dead_handles, NULL, memory_order_acquire);
  }
}

__attribute__((constructor)) static void handle_lifetime_register(void) {
  osp_mem_set_finalizer(OSP_MEM_FIBER_HANDLE, held_handle_finalize);
  osp_mem_set_finalizer(OSP_MEM_CHANNEL_HANDLE, held_handle_finalize);
}

// A fresh handle of external subtype `meta` holding nothing yet. Compiled code
// has no failure branch for one, so a refusal stops the run by `site`
// [MEM-REFUSED-STOP].
static HeldHandle *held_handle_new(int64_t meta, const char *site) {
  HeldHandle *handle = osp_alloc_tagged_noinit(sizeof(HeldHandle), meta);
  if (!handle) {
    osp_alloc_refused(site, sizeof(HeldHandle));
  }
  *handle = (HeldHandle){.length = 0, .data = NULL, .held = NULL};
  return handle;
}

// What a handle holds, with a hold of the caller's own: a tracing collector may
// reclaim the handle while the caller still reads the object.
static Held *held_of_handle(const void *handle) {
  Held *held = handle ? ((const HeldHandle *)handle)->held : NULL;
  if (held) {
    held_hold(held);
  }
  return held;
}

#endif
