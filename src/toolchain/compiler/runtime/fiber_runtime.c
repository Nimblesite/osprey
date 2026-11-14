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
#include <unistd.h>

#include "memory_hooks.h"
#include "memory_parked_wait.h"
#include "osprey_thread.h"
#include "profiler_runtime.h"

// Forward declarations for effect handler snapshot/restore
typedef struct HandlerSnapshot HandlerSnapshot;
HandlerSnapshot *__osprey_handler_snapshot(void);
void __osprey_handler_restore(HandlerSnapshot *snap);
void __osprey_handler_stack_cleanup(void);

// Fiber runtime implementation in C for linking with LLVM-generated code

typedef struct Fiber {
  int64_t id;
  int64_t (*function)(void);
  // Closure-cell entry: when set, takes precedence over `function`. The env
  // points at a per-spawn heap cell owned by the spawning program (codegen
  // emits it), so two in-flight spawns from one site never share state.
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
} Fiber;

// Run a fiber's entry point through whichever ABI it was spawned with.
static int64_t run_fiber_fn(Fiber *fiber) {
  if (fiber->env_function != NULL) {
    int64_t result = fiber->env_function(fiber->env);
    // The spawn transferred the capture cell (+1) to the runtime; the thunk
    // has fully consumed it once it returns. No-op outside the ARC backend
    // [GC-ARC-PERCEUS] [MEM-FIBER-ISOLATION].
    osp_release(fiber->env);
    fiber->env = NULL;
    return result;
  }
  return fiber->function();
}

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

// Global runtime state. Fibers and channels share one id space, and a handle is
// never reused: a `Fiber<T>` can be awaited any number of times for as long as the
// program holds it, so nothing says when its id could mean something else. The
// tables grow to fit. They hold pointers, so what a lookup returns outlives the
// lock even if they then move. All of it is under runtime_mutex.
// [CONCURRENCY-HANDLES]
static Fiber **fibers;
static Channel **channels;
static int64_t handle_capacity;
static int64_t next_id = 1;
static pthread_mutex_t runtime_mutex = PTHREAD_MUTEX_INITIALIZER;

// Sequential execution mode [CONCURRENCY-DETERMINISTIC]. Queued fibers run in
// spawn order, which is id order, so the tables are the queue: nothing below
// queue_start can still need running.
static bool deterministic_mode = false;
static int64_t queue_start = 1;

// Enable/disable deterministic fiber execution
int64_t fiber_set_deterministic_mode(bool enabled) {
  pthread_mutex_lock(&runtime_mutex);
  deterministic_mode = enabled;
  if (enabled) {
    queue_start = next_id; // Reset queue when enabling
  }
  pthread_mutex_unlock(&runtime_mutex);
  return 0;
}

static bool grow_table(void **table, int64_t capacity, size_t entry) {
  char *grown = realloc(*table, (size_t)capacity * entry);
  if (!grown) {
    return false;
  }
  size_t kept = (size_t)handle_capacity * entry;
  memset(grown + kept, 0, (size_t)capacity * entry - kept);
  *table = grown;
  return true;
}

// Makes room for next_id in both tables; false when out of memory.
static bool reserve_handle(void) {
  if (next_id < handle_capacity) {
    return true;
  }
  int64_t capacity = handle_capacity > 0 ? handle_capacity * 2 : 1024;
  void *grown_fibers = fibers;
  void *grown_channels = channels;
  bool grown = grow_table(&grown_fibers, capacity, sizeof(Fiber *));
  fibers = grown_fibers;
  grown = grown && grow_table(&grown_channels, capacity, sizeof(Channel *));
  channels = grown_channels;
  if (grown) {
    handle_capacity = capacity;
  }
  return grown;
}

static Fiber *fiber_of(int64_t fiber_id) {
  pthread_mutex_lock(&runtime_mutex);
  Fiber *fiber = fiber_id >= 1 && fiber_id < next_id ? fibers[fiber_id] : NULL;
  pthread_mutex_unlock(&runtime_mutex);
  return fiber;
}

static Channel *channel_of(int64_t channel_id) {
  pthread_mutex_lock(&runtime_mutex);
  Channel *channel = channel_id >= 1 && channel_id < next_id ? channels[channel_id] : NULL;
  pthread_mutex_unlock(&runtime_mutex);
  return channel;
}

// Takes a completed fiber's still-owned result for release, at most once.
static bool take_owned_result(Fiber *fiber, int64_t *result) {
  if (!fiber->completed || !fiber->result_owned) {
    return false;
  }
  *result = fiber->result;
  fiber->result_owned = false;
  return true;
}

// Restores the parent's effect handlers so perform calls work inside the
// fiber, then runs its body.
static void run_with_parent_handlers(Fiber *fiber) {
  if (fiber->handler_snapshot != NULL) {
    __osprey_handler_restore(fiber->handler_snapshot);
    fiber->handler_snapshot = NULL;
  }
  fiber->result = run_fiber_fn(fiber);
}

// Run a fiber on this thread's stack, holding no lock: its body can use the runtime.
static void execute_fiber_directly(Fiber *fiber) {
  run_with_parent_handlers(fiber);
  fiber->result_owned = true;
  fiber->completed = true;
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
  osp_prof_thread_unregister();
  osp_mem_baton_thread_exit();

  // Mark as completed and signal
  pthread_mutex_lock(&fiber->mutex);
  fiber->result_owned = true;
  fiber->completed = true;
  pthread_cond_signal(&fiber->cond);
  pthread_mutex_unlock(&fiber->mutex);

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

// Create and schedule a fiber (shared by both spawn ABIs).
// [CONCURRENCY-SPAWN-AWAIT]
static int64_t fiber_spawn_internal(int64_t (*fn)(void),
                                    int64_t (*env_fn)(void *), void *env,
                                    bool result_managed) {
  // Scanned: the capture cell and the result are managed words no stack names.
  Fiber *fiber = osp_mem_scanned_alloc(sizeof(Fiber));
  if (!fiber) {
    return -2; // Memory allocation failed
  }
  pthread_mutex_lock(&runtime_mutex);
  if (!reserve_handle()) {
    pthread_mutex_unlock(&runtime_mutex);
    osp_mem_scanned_free(fiber);
    return -2;
  }
  int64_t id = next_id++;
  fiber->id = id;
  fiber->function = fn;
  fiber->env_function = env_fn;
  fiber->env = env;
  fiber->result = 0;
  fiber->result_owned = false;
  fiber->result_managed = result_managed;
  fiber->completed = false;
  fiber->running = false;
  fiber->uses_thread = !deterministic_mode;
  fiber->handler_snapshot = __osprey_handler_snapshot();
  pthread_mutex_init(&fiber->mutex, NULL);
  pthread_cond_init(&fiber->cond, NULL);
  fibers[id] = fiber;
  bool queued = !fiber->uses_thread;
  if (!queued && !start_fiber_thread(fiber)) {
    // No thread left to give it, so it runs here, and spawn still means what it says.
    fiber->uses_thread = false;
    fiber->running = true;
    pthread_mutex_unlock(&runtime_mutex);
    execute_fiber_directly(fiber);
    return id;
  }
  pthread_mutex_unlock(&runtime_mutex);
  return id;
}

// Spawn with the env-free entry ABI.
int64_t fiber_spawn(int64_t (*fn)(void)) {
  if (!fn) {
    return -1; // Invalid function pointer
  }
  return fiber_spawn_internal(fn, NULL, NULL, false);
}

// Spawn with the closure-cell entry ABI: `fn(env)` runs on the fiber. The env
// cell is allocated per spawn by the compiled program, so re-entering the same
// spawn site never aliases another in-flight fiber's captures.
int64_t fiber_spawn_env(int64_t (*fn)(void *), void *env) {
  if (!fn) {
    return -1; // Invalid function pointer
  }
  return fiber_spawn_internal(NULL, fn, env, false);
}

// Spawn a compiler-generated closure thunk and preserve whether its erased
// result word is a managed pointer. The legacy two-argument entry point remains
// scalar for C-runtime callers such as the HTTP server.
int64_t fiber_spawn_env_owned(int64_t (*fn)(void *), void *env,
                              int64_t result_managed) {
  if (!fn) {
    return -1; // Invalid function pointer
  }
  return fiber_spawn_internal(NULL, fn, env, result_managed != 0);
}

// Claims the first queued fiber at or below `last` that nothing has started.
static Fiber *claim_queued(int64_t last) {
  pthread_mutex_lock(&runtime_mutex);
  Fiber *next = NULL;
  for (int64_t id = queue_start; id < next_id && id <= last && !next; id++) {
    Fiber *fiber = fibers[id];
    if (fiber && !fiber->uses_thread && !fiber->completed && !fiber->running) {
      fiber->running = true;
      next = fiber;
    }
  }
  while (queue_start < next_id && (!fibers[queue_start] || fibers[queue_start]->uses_thread ||
                                   fibers[queue_start]->completed)) {
    queue_start++;
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

// Wait for fiber completion [CONCURRENCY-SPAWN-AWAIT].
int64_t fiber_await(int64_t fiber_id) {
  Fiber *fiber = fiber_of(fiber_id);
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

// Release the runtime roots of completed fiber results. Codegen calls this
// once at the end of main, after language-level ARC owners have dropped, so the
// retained root supports any number of awaits without becoming a process leak.
void fiber_cleanup_results(void) {
  pthread_mutex_lock(&runtime_mutex);

  // Never invalidate a cached result while another fiber can still execute an
  // await against it. Unstructured, still-running fibers are terminated by
  // process exit; their roots intentionally remain process-owned here. Once
  // every thunk is quiescent no more language code can race this teardown.
  for (int64_t id = 1; id < next_id; id++) {
    Fiber *fiber = fibers[id];
    if (!fiber)
      continue;

    bool completed;
    if (fiber->uses_thread) {
      pthread_mutex_lock(&fiber->mutex);
      completed = fiber->completed;
      pthread_mutex_unlock(&fiber->mutex);
    } else {
      completed = fiber->completed;
    }
    if (!completed) {
      pthread_mutex_unlock(&runtime_mutex);
      return;
    }
  }

  for (int64_t id = 1; id < next_id; id++) {
    Fiber *fiber = fibers[id];
    if (!fiber)
      continue;

    int64_t result = 0;
    bool release_result = false;
    if (fiber->uses_thread) {
      pthread_mutex_lock(&fiber->mutex);
      release_result = take_owned_result(fiber, &result);
      pthread_mutex_unlock(&fiber->mutex);
    } else {
      release_result = take_owned_result(fiber, &result);
    }

    if (release_result && fiber->result_managed) {
      osp_release((void *)(uintptr_t)result);
    }
  }
  pthread_mutex_unlock(&runtime_mutex);
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
