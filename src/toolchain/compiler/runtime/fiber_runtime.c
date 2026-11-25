#include <errno.h>
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "memory_refused.h"
#include <unistd.h>

#include "fiber_api.h"
#include "memory_hooks.h"
#include "memory_parked_wait.h"
#include "osprey_thread.h"
#include "profiler_runtime.h"

// Forward declarations for effect handler snapshot/restore
typedef struct HandlerSnapshot HandlerSnapshot;
size_t __osprey_handler_try_snapshot(HandlerSnapshot **out);
void __osprey_handler_restore(HandlerSnapshot *snap);
void __osprey_handler_stack_cleanup(void);

// Fiber runtime implementation in C for linking with LLVM-generated code

typedef struct Fiber {
  int64_t id;
  // Codegen allocates a separate environment per spawn so two in-flight
  // fibers from one site never share captures.
  int64_t (*env_function)(void *);
  void *env;
  int64_t result;
  // The completed result keeps one runtime-owned managed reference. Each
  // await retains a separate caller reference; main releases this root through
  // fiber_cleanup_results after its language-level owners have dropped.
  bool result_owned;
  // The erased i64 is a managed pointer only when codegen says so. Never probe
  // scalar/float bits: they may coincidentally equal a live heap address.
  bool result_managed;
  bool completed;
  bool running; // sequential only: on some stack below this one, not finished
  pthread_mutex_t mutex;
  pthread_cond_t cond;
  bool uses_thread;
  HandlerSnapshot *handler_snapshot; // Inherited effect handlers from parent
  _Atomic int64_t holders;           // fiber_lifetime.h [CONCURRENCY-FIBER-LIFETIME]
  struct Fiber *queue_next;          // the deterministic queue, under runtime_mutex
  struct Fiber *dead_next;           // the dead-handle list (fiber_lifetime.h)
} Fiber;

typedef struct Channel {
  int64_t id;
  int capacity;
  int64_t *buffer;
  int head;
  int tail;
  int count;
  // A sent managed value arrives carrying a +1 the CHANNEL owns until a `recv`
  // adopts it. Recorded per channel — the element type is monomorphic
  // ([CONCURRENCY-CHANNEL]) — so teardown knows whether what is still buffered
  // has references to release. [GC-ARC-PERCEUS]
  bool elem_managed;
  pthread_mutex_t mutex;
  pthread_cond_t not_empty;
  pthread_cond_t not_full;
} Channel;

// Global runtime state, all under runtime_mutex. A fiber is reached only through
// the holds on it (fiber_lifetime.h), so it needs no table. A channel id is a
// slot in `channels`, which grows to fit; it holds pointers, so what a lookup
// returns outlives the lock even if the table then moves. [CONCURRENCY-HANDLES]
static Channel **channels;
static int64_t handle_capacity;
static int64_t next_id = 1;       // the next channel slot
static int64_t next_fiber_id = 1; // spawn order, which the deterministic queue keeps
static pthread_mutex_t runtime_mutex = PTHREAD_MUTEX_INITIALIZER;

// Sequential execution mode [CONCURRENCY-DETERMINISTIC]. Queued fibers run in
// spawn order: the queue is a FIFO list, and each holds its runner's hold.
static bool deterministic_mode = false;
static Fiber *queue_head;
static Fiber *queue_tail;

// Enable/disable deterministic fiber execution
int64_t fiber_set_deterministic_mode(bool enabled) {
  pthread_mutex_lock(&runtime_mutex);
  deterministic_mode = enabled;
  pthread_mutex_unlock(&runtime_mutex);
  return 0;
}

#include "fiber_lifetime.h"

// Makes room for channel slot next_id: 0, or the bytes the allocator refused.
static size_t reserve_handle(void) {
  if (next_id < handle_capacity) {
    return 0;
  }
  int64_t capacity = handle_capacity > 0 ? handle_capacity * 2 : 1024;
  size_t bytes = (size_t)capacity * sizeof(Channel *);
  Channel **grown = realloc(channels, bytes);
  if (!grown) {
    return bytes;
  }
  size_t kept = (size_t)handle_capacity * sizeof(Channel *);
  memset((char *)grown + kept, 0, bytes - kept);
  channels = grown;
  handle_capacity = capacity;
  return 0;
}

static Channel *channel_of(int64_t channel_id) {
  pthread_mutex_lock(&runtime_mutex);
  Channel *channel = channel_id >= 1 && channel_id < next_id ? channels[channel_id] : NULL;
  pthread_mutex_unlock(&runtime_mutex);
  return channel;
}

// Restores the parent's effect handlers so perform calls work inside the
// fiber, then runs its body.
static void run_with_parent_handlers(Fiber *fiber) {
  if (fiber->handler_snapshot != NULL) {
    __osprey_handler_restore(fiber->handler_snapshot);
    fiber->handler_snapshot = NULL;
  }
  int64_t result = fiber->env_function(fiber->env);
  // The spawn transferred its capture cell to the runtime until the thunk
  // returned. [GC-ARC-PERCEUS] [MEM-FIBER-ISOLATION]
  osp_release(fiber->env);
  fiber->env = NULL;
  fiber->result = result;
}

// Run a fiber on this thread's stack, holding no lock: its body can use the
// runtime. The runner's hold ends with it.
static void execute_fiber_directly(Fiber *fiber) {
  run_with_parent_handlers(fiber);
  fiber->result_owned = true;
  fiber->completed = true;
  fiber_release(fiber);
}

// Thread function for executing fibers
static void *fiber_thread_func(void *arg) {
  Fiber *fiber = (Fiber *)arg;

  // The collector can only run once it knows every thread that can hold a root.
  osp_mem_baton_thread_enter();

  // Fibers are 1:1 pthreads, so registering here gives the CPU profiler exact
  // per-fiber sample attribution [PROF-COLLECT-REGISTRY]. No-op when inactive.
  osp_prof_thread_register(fiber->id, "fiber");

  run_with_parent_handlers(fiber);
  // The handler stack is this thread's own; left behind, every fiber kept about
  // 64 KiB for the life of the process. [CONCURRENCY-HANDLES]
  __osprey_handler_stack_cleanup();

  pthread_mutex_lock(&fiber->mutex);
  fiber->result_owned = true;
  fiber->completed = true;
  pthread_cond_broadcast(&fiber->cond);
  pthread_mutex_unlock(&fiber->mutex);
  // The runner's hold. Still a heap thread: the last hold frees through the heap.
  fiber_release(fiber);
  osp_prof_thread_unregister();
  osp_mem_baton_thread_exit();
  return NULL;
}

// Starts the fiber's own thread, which is detached: completion is what `await` waits
// for, and a fiber nobody awaits must not keep its stack for the life of the process.
static bool start_fiber_thread(Fiber *fiber) {
  pthread_t thread;
  // A real fiber thread is about to touch the shared value heap: trip the
  // memory backend off its single-threaded fast path BEFORE the thread that
  // needs the lock can exist (pthread_create is the happens-before barrier).
  osp_mem_notify_multithreaded();
  osp_mem_baton_prepare();
  return osp_thread_start(&thread, fiber_thread_func, fiber, true) == 0;
}

// Appends a fiber to the deterministic queue, under runtime_mutex.
static void enqueue(Fiber *fiber) {
  fiber->queue_next = NULL;
  if (queue_tail) {
    queue_tail->queue_next = fiber;
  } else {
    queue_head = fiber;
  }
  queue_tail = fiber;
}

const size_t fiber_record_bytes = sizeof(Fiber);

// A fresh fiber holding the caller's and the runner's holds, or NULL with the
// refused allocation's size in `*refused`.
static Fiber *fiber_new(int64_t (*env_fn)(void *), void *env, bool result_managed,
                        size_t *refused) {
  // Scanned: the capture cell and the result are managed words no stack names.
  Fiber *fiber = osp_mem_scanned_alloc(sizeof(Fiber));
  if (!fiber) {
    *refused = fiber_record_bytes;
    return NULL;
  }
  // No memory for the parent's handlers: the spawn is refused, never run without them.
  *refused = __osprey_handler_try_snapshot(&fiber->handler_snapshot);
  if (*refused != 0) {
    osp_mem_scanned_free(fiber);
    return NULL;
  }
  fiber->env_function = env_fn;
  fiber->env = env;
  fiber->result = 0;
  fiber->result_owned = false;
  fiber->result_managed = result_managed;
  fiber->completed = false;
  fiber->running = false;
  atomic_init(&fiber->holders, 2);
  fiber->queue_next = NULL;
  fiber->dead_next = NULL;
  pthread_mutex_init(&fiber->mutex, NULL);
  pthread_cond_init(&fiber->cond, NULL);
  return fiber;
}

// Create and schedule a fiber, returning the caller's hold on it; NULL for a
// NULL body, or when memory is refused, with the refused allocation's size in
// `*refused`. Handles that died since the last spawn are reclaimed first, so a
// program spawning in a loop keeps only the fibers it can still await.
// [CONCURRENCY-SPAWN-AWAIT] [CONCURRENCY-FIBER-LIFETIME]
static Fiber *fiber_start(int64_t (*env_fn)(void *), void *env, bool result_managed,
                          size_t *refused) {
  *refused = 0;
  fiber_reclaim();
  if (!env_fn) {
    return NULL;
  }
  Fiber *fiber = fiber_new(env_fn, env, result_managed, refused);
  if (!fiber) {
    return NULL;
  }
  pthread_mutex_lock(&runtime_mutex);
  fiber->id = next_fiber_id++;
  fiber->uses_thread = !deterministic_mode;
  if (!fiber->uses_thread) {
    enqueue(fiber);
    pthread_mutex_unlock(&runtime_mutex);
    return fiber;
  }
  if (!start_fiber_thread(fiber)) {
    // No thread left to give it, so it runs here, and spawn still means what it says.
    fiber->uses_thread = false;
    fiber->running = true;
    pthread_mutex_unlock(&runtime_mutex);
    execute_fiber_directly(fiber);
    return fiber;
  }
  pthread_mutex_unlock(&runtime_mutex);
  return fiber;
}

// The C runtime's own spawn: the caller's hold, which it ends with
// fiber_release, or NULL when refused.
Fiber *fiber_spawn_env(int64_t (*fn)(void *), void *env) {
  size_t refused = 0;
  return fiber_start(fn, env, false, &refused);
}

// Spawn a compiler-generated closure thunk and preserve whether its erased
// result word is a managed pointer. The program's hold is a managed handle the
// memory backend reclaims ([CONCURRENCY-FIBER-LIFETIME]), made before the fiber
// starts so a refusal never leaves one running. Compiled `spawn` has no failure
// branch, so a fiber the runtime cannot allocate stops the run by name instead
// of handing back a handle the program would carry on with (#359)
// [MEM-REFUSED-STOP]. NULL for a NULL body, which compiled code never passes.
void *fiber_spawn_env_owned(int64_t (*fn)(void *), void *env, int64_t result_managed) {
  FiberHandle *handle = osp_alloc_tagged_noinit(sizeof(FiberHandle), OSP_MEM_FIBER_HANDLE);
  if (!handle) {
    osp_alloc_refused("spawn", sizeof(FiberHandle));
  }
  *handle = (FiberHandle){.length = 0, .data = NULL, .fiber = NULL};
  size_t refused = 0;
  handle->fiber = fiber_start(fn, env, result_managed != 0, &refused);
  if (handle->fiber) {
    return handle;
  }
  osp_release(handle);
  if (refused != 0) {
    osp_alloc_refused("spawn", refused);
  }
  return NULL;
}

// Claims the first queued fiber, if it was spawned no later than `last`: the
// queue is in spawn order, and everything on it is yet to start.
static Fiber *claim_queued(int64_t last) {
  pthread_mutex_lock(&runtime_mutex);
  Fiber *next = queue_head && queue_head->id <= last ? queue_head : NULL;
  if (next) {
    queue_head = next->queue_next;
    queue_tail = queue_head ? queue_tail : NULL;
    next->running = true;
  }
  pthread_mutex_unlock(&runtime_mutex);
  return next;
}

// Sequential mode has no other thread to wait for: a queued fiber runs instead, in
// spawn order. False once none is left to run. [CONCURRENCY-DETERMINISTIC]
static bool run_queued(int64_t last) {
  Fiber *next = claim_queued(last);
  if (next) {
    execute_fiber_directly(next);
  }
  return next != NULL;
}

// A fiber with no thread of its own finishes here, through every fiber queued before
// it. One that can't is waiting on this very stack, so no await can ever answer it.
static int64_t await_on_this_stack(Fiber *fiber) {
  while (run_queued(fiber->id)) {
  }
  if (!fiber->completed) {
    return -1;
  }
  int64_t result = fiber->result;
  // The runtime keeps the thunk's transferred +1 until final cleanup. Every
  // await receives an independent +1 so repeated awaits cannot observe a
  // result reclaimed by an earlier caller [MEM-FIBER-ISOLATION]. Scalars and
  // foreign words are safe no-ops in every memory backend.
  if (fiber->result_managed) {
    osp_retain((void *)(uintptr_t)result);
  }
  return result;
}

// Waits for a fiber the caller holds to complete [CONCURRENCY-SPAWN-AWAIT].
// -1 for NULL, or for a sequential fiber waiting on this very stack.
int64_t fiber_join(Fiber *fiber) {
  if (!fiber) {
    return -1;
  }
  if (!fiber->uses_thread) {
    return await_on_this_stack(fiber);
  }
  pthread_mutex_lock(&fiber->mutex);
  while (!fiber->completed) {
    (void)osp_parked_wait(&fiber->cond, &fiber->mutex, NULL);
  }
  int64_t result = fiber->result;
  if (fiber->result_managed) {
    osp_retain((void *)(uintptr_t)result);
  }
  pthread_mutex_unlock(&fiber->mutex);
  return result;
}

// `await` on a program's handle, holding the fiber while it waits.
int64_t fiber_await(void *handle) {
  Fiber *fiber = fiber_of_handle(handle);
  int64_t result = fiber_join(fiber);
  fiber_release(fiber);
  return result;
}

// Drops the holds of every handle the program let go of. Codegen calls this
// once at the end of main, after its language-level owners have dropped, so
// each finished fiber they held is freed with its result. A fiber still running
// keeps its runner's hold and is ended by process exit.
void fiber_cleanup_results(void) { fiber_reclaim(); }

// Cooperative hand-off [CONCURRENCY-YIELD]. In concurrent (threaded) mode,
// donate the rest of this
// fiber's time slice to the scheduler so a peer fiber can run, then resume and
// forward `value`. In deterministic mode fibers run sequentially to completion,
// so there is no peer to switch to and yield forwards `value` unchanged,
// preserving the differential harness's reproducible ordering. (`deterministic_mode`
// is set once at startup before any fiber runs, so this lock-free read is safe.)
// True cross-fiber interleaving under deterministic mode would need stackful
// context switching (a separate, larger change); see docs spec 0011 §yield.
int64_t fiber_yield(int64_t value) {
  if (!deterministic_mode) {
    sched_yield();
  }
  return value;
}

#include "fiber_channels.h"

#include "fiber_waits.h"
