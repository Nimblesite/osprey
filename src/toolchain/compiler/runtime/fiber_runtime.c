#if defined(__wasi__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE // clock_gettime and nanosleep under -std=c11
#endif
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

#include "handle_lifetime.h"

typedef struct Fiber {
  Held held; // first: handle_lifetime.h [CONCURRENCY-FIBER-LIFETIME]
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
  struct Fiber *queue_next;          // the deterministic queue, under runtime_mutex
} Fiber;

typedef struct Channel {
  Held held; // first: handle_lifetime.h [CONCURRENCY-CHANNEL-LIFETIME]
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

// Global runtime state, all under runtime_mutex. Fibers and channels are
// reached only through the holds on them (handle_lifetime.h), so neither needs
// a table. [CONCURRENCY-HANDLES]
static int64_t next_fiber_id = 1; // spawn order, which the deterministic queue keeps
static pthread_mutex_t runtime_mutex = PTHREAD_MUTEX_INITIALIZER;
// Runners that have published `completed` and not yet dropped their hold, under
// runtime_mutex. Exit cleanup waits for zero: a completed fiber's result is
// released by whichever holder is last, and the leak census after main must not
// run before the runner's drop [CONCURRENCY-FIBER-LIFETIME] [GC-ARC-PERCEUS].
static size_t releasing_runners;
static pthread_cond_t runners_idle = PTHREAD_COND_INITIALIZER;

// Sequential execution mode [CONCURRENCY-DETERMINISTIC]. Queued fibers run in
// spawn order: the queue is a FIFO list, and each holds its runner's hold.
// Where there is one thread (memory_parked_wait.h) it is always sequential
// [CONCURRENCY-WASM] [SYMEX-MOCK-THREADS].
static bool deterministic_mode = OSP_ONE_THREAD;
static Fiber *queue_head;
static Fiber *queue_tail;
// The fiber whose thread this is; NULL on a thread no fiber owns.
static _Thread_local Fiber *thread_fiber;

// Enable/disable deterministic fiber execution
int64_t fiber_set_deterministic_mode(bool enabled) {
  pthread_mutex_lock(&runtime_mutex);
  deterministic_mode = enabled || OSP_ONE_THREAD;
  pthread_mutex_unlock(&runtime_mutex);
  return 0;
}

// Frees everything a fiber still owns. Only the last holder gets here.
static void fiber_free(Held *held) {
  Fiber *fiber = (Fiber *)held;
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

void fiber_release(Fiber *fiber) { held_release((Held *)fiber); }

// Fibers waiting on a channel now, and how many stuck ones were reported.
static atomic_int channel_waits;
static _Atomic size_t stuck_reported;

// How many fibers are parked on a channel right now, each counted only once
// its wait is registered: what a test waits on before collecting a verdict,
// instead of a timer a loaded runner can outrun.
int fiber_channel_waits(void) {
  return atomic_load_explicit(&channel_waits, memory_order_relaxed);
}

// Names the fibers a collection found blocked for ever since the last report
// [CONCURRENCY-DEADLOCK]. The collector only counts them: it may hold the heap
// lock or have stopped the world, so the report is written here.
static void report_stuck_fibers(void) {
  size_t found = osp_mem_stuck_waits();
  size_t seen = atomic_exchange_explicit(&stuck_reported, found, memory_order_relaxed);
  if (found > seen) {
    fprintf(stderr,
            "osprey: %zu fiber(s) blocked for ever: only blocked fibers can reach "
            "the channel each waits on [CONCURRENCY-DEADLOCK]\n",
            found - seen);
  }
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

// A runner about to publish its completion: counted until it has dropped its
// hold. Taken before `completed` is visible, so whoever observed the completion
// finds the count until the drop has happened.
static void runner_releasing_enter(void) {
  pthread_mutex_lock(&runtime_mutex);
  releasing_runners++;
  pthread_mutex_unlock(&runtime_mutex);
}

// Past `fiber_release`, which may have freed the fiber: touches nothing of it.
static void runner_releasing_leave(void) {
  pthread_mutex_lock(&runtime_mutex);
  if (--releasing_runners == 0) {
    pthread_cond_broadcast(&runners_idle);
  }
  pthread_mutex_unlock(&runtime_mutex);
}

// Thread function for executing fibers
static void *fiber_thread_func(void *arg) {
  Fiber *fiber = (Fiber *)arg;

  // The collector can only run once it knows every thread that can hold a root.
  osp_mem_baton_thread_enter();

  // Fibers are 1:1 pthreads, so registering here gives the CPU profiler exact
  // per-fiber sample attribution [PROF-COLLECT-REGISTRY]. No-op when inactive.
  osp_prof_thread_register(fiber->id, "fiber");

  thread_fiber = fiber;
  run_with_parent_handlers(fiber);
  // The handler stack is this thread's own; left behind, every fiber kept about
  // 64 KiB for the life of the process. [CONCURRENCY-HANDLES]
  __osprey_handler_stack_cleanup();
  runner_releasing_enter();

  pthread_mutex_lock(&fiber->mutex);
  fiber->result_owned = true;
  fiber->completed = true;
  pthread_cond_broadcast(&fiber->cond);
  pthread_mutex_unlock(&fiber->mutex);
  // The runner's hold. Still a heap thread: the last hold frees through the heap.
  fiber_release(fiber);
  runner_releasing_leave();
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
  HandlerSnapshot *snapshot = NULL;
  *refused = __osprey_handler_try_snapshot(&snapshot);
  if (*refused != 0) {
    osp_mem_scanned_free(fiber);
    return NULL;
  }
  *fiber = (Fiber){.env_function = env_fn, .env = env, .result_managed = result_managed,
                   .handler_snapshot = snapshot};
  held_init(&fiber->held, 2, fiber_free);
  pthread_mutex_init(&fiber->mutex, NULL);
  pthread_cond_init(&fiber->cond, NULL);
  return fiber;
}

// Numbers a fresh fiber and starts it: queued in sequential mode, otherwise on
// its own thread, or here when the system has no thread left to give.
static void schedule(Fiber *fiber) {
  pthread_mutex_lock(&runtime_mutex);
  fiber->id = next_fiber_id++;
  fiber->uses_thread = !deterministic_mode;
  if (!fiber->uses_thread) {
    enqueue(fiber);
  } else if (!start_fiber_thread(fiber)) {
    // It runs here, and spawn still means what it says.
    fiber->uses_thread = false;
    fiber->running = true;
    pthread_mutex_unlock(&runtime_mutex);
    execute_fiber_directly(fiber);
    return;
  }
  pthread_mutex_unlock(&runtime_mutex);
}

// Create and schedule a fiber, returning the caller's hold on it; NULL for a
// NULL body, or when memory is refused, with the refused allocation's size in
// `*refused`. Handles that died since the last spawn are reclaimed first, so a
// program spawning in a loop keeps only the fibers it can still await.
// [CONCURRENCY-SPAWN-AWAIT] [CONCURRENCY-FIBER-LIFETIME]
static Fiber *fiber_start(int64_t (*env_fn)(void *), void *env, bool result_managed,
                          size_t *refused) {
  *refused = 0;
  held_reclaim();
  report_stuck_fibers();
  Fiber *fiber = env_fn ? fiber_new(env_fn, env, result_managed, refused) : NULL;
  if (fiber) {
    schedule(fiber);
  }
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
  HeldHandle *handle = held_handle_new(OSP_MEM_FIBER_HANDLE, "spawn");
  size_t refused = 0;
  handle->held = (Held *)fiber_start(fn, env, result_managed != 0, &refused);
  if (handle->held) {
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
  Fiber *fiber = (Fiber *)held_of_handle(handle);
  int64_t result = fiber_join(fiber);
  fiber_release(fiber);
  return result;
}

// Wait until no completed runner is still dropping its hold. Only a fiber whose
// body has finished is counted: one still running is ended by process exit, so
// this never waits on it. The wait parks like `fiber_join`, so a tracing
// collector can scan this stack while it waits [MEM-GC-SAFEPOINT].
static void wait_for_releasing_runners(void) {
  pthread_mutex_lock(&runtime_mutex);
  while (releasing_runners > 0) {
    (void)osp_parked_wait(&runners_idle, &runtime_mutex, NULL);
  }
  pthread_mutex_unlock(&runtime_mutex);
}

// Drops the holds of every handle the program let go of. Codegen calls this
// once at the end of main, after its language-level owners have dropped, so
// each finished fiber and unshared channel they held is freed. A fiber still
// running keeps its runner's hold and is ended by process exit; one still
// waiting on a channel gets a last collection's verdict [CONCURRENCY-DEADLOCK].
// A fiber that finished but whose runner has not yet let go is waited for: its
// awaited result is released by that drop, and nothing else runs before the
// exit census [CONCURRENCY-FIBER-LIFETIME].
void fiber_cleanup_results(void) {
  held_reclaim();
  wait_for_releasing_runners();
  if (atomic_load_explicit(&channel_waits, memory_order_relaxed) > 0) {
    osp_collect();
  }
  report_stuck_fibers();
}

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
