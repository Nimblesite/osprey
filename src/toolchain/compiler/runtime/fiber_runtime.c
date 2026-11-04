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
#include "profiler_runtime.h"

// Forward declarations for effect handler snapshot/restore
typedef struct HandlerSnapshot HandlerSnapshot;
HandlerSnapshot *__osprey_handler_snapshot(void);
void __osprey_handler_restore(HandlerSnapshot *snap);

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

// `select` waits on every channel at once, so it cannot sleep on one channel's
// condition. A channel operation that changes a channel's state advances this
// generation; a waiter records it BEFORE trying its arms, so a change made while
// it tried wakes it at once. [CONCURRENCY-SELECT]
static pthread_mutex_t select_mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t select_changed = PTHREAD_COND_INITIALIZER;
static uint64_t select_generation = 0;
static atomic_int select_waiters = 0;

static void select_notify(void) {
  if (atomic_load(&select_waiters) == 0) {
    return;
  }
  pthread_mutex_lock(&select_mutex);
  select_generation++;
  pthread_cond_broadcast(&select_changed);
  pthread_mutex_unlock(&select_mutex);
}

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

// Run a fiber on this thread's stack, holding no lock: its body can use the runtime.
static void execute_fiber_directly(Fiber *fiber) {
  if (fiber->handler_snapshot != NULL) {
    __osprey_handler_restore(fiber->handler_snapshot);
    fiber->handler_snapshot = NULL;
  }
  fiber->result = run_fiber_fn(fiber);
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

  // Restore parent's effect handlers so perform calls work inside the fiber
  if (fiber->handler_snapshot != NULL) {
    __osprey_handler_restore(fiber->handler_snapshot);
    fiber->handler_snapshot = NULL;
  }

  // Execute the fiber function
  fiber->result = run_fiber_fn(fiber);
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
  pthread_attr_t attributes;
  if (pthread_attr_init(&attributes) != 0) {
    return false;
  }
  pthread_attr_setdetachstate(&attributes, PTHREAD_CREATE_DETACHED);
  // A real fiber thread is about to touch the shared value heap: trip the
  // memory backend off its single-threaded fast path BEFORE the thread that
  // needs the lock can exist (pthread_create is the happens-before barrier).
  osp_mem_notify_multithreaded();
  osp_mem_baton_prepare();
  bool started = pthread_create(&thread, &attributes, fiber_thread_func, fiber) == 0;
  pthread_attr_destroy(&attributes);
  return started;
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
      if (fiber->completed && fiber->result_owned) {
        result = fiber->result;
        fiber->result_owned = false;
        release_result = true;
      }
      pthread_mutex_unlock(&fiber->mutex);
    } else if (fiber->completed && fiber->result_owned) {
      result = fiber->result;
      fiber->result_owned = false;
      release_result = true;
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

// Create a positive-capacity buffered channel. Capacity zero is not a
// rendezvous channel in this runtime; accepting it would leave both send and
// recv waiting forever on an empty buffer. [CONCURRENCY-CHANNEL]
int64_t channel_create(int64_t capacity) {
  if (capacity <= 0 || capacity > INT32_MAX ||
      (uint64_t)capacity > SIZE_MAX / sizeof(int64_t)) {
    return -1;
  }

  Channel *channel = malloc(sizeof(Channel));
  if (!channel) {
    return -2;
  }
  // Scanned: a buffered value may be named by nothing else.
  channel->buffer = osp_mem_scanned_alloc((size_t)capacity * sizeof(int64_t));
  if (!channel->buffer) {
    free(channel);
    return -2;
  }

  pthread_mutex_lock(&runtime_mutex);
  if (!reserve_handle()) {
    pthread_mutex_unlock(&runtime_mutex);
    osp_mem_scanned_free(channel->buffer);
    free(channel);
    return -2;
  }

  int64_t id = next_id++;
  channel->id = id;
  channel->capacity = (int)capacity;
  channel->head = 0;
  channel->tail = 0;
  channel->count = 0;
  channel->elem_managed = false;
  pthread_mutex_init(&channel->mutex, NULL);
  pthread_cond_init(&channel->not_empty, NULL);
  pthread_cond_init(&channel->not_full, NULL);

  channels[id] = channel;

  pthread_mutex_unlock(&runtime_mutex);

  return id;
}

// `Channel(capacity)` answers `Channel<T>` in the language, not
// `Result<Channel<T>, E>`, so a creation that cannot be honoured has NOWHERE to
// report. Handing the negative code back as if it were a handle is what turned
// `Channel(0)` into a poison value: `send` on it enqueued nothing and returned
// as though it had, and `recv` on it answered `-1` AS THE VALUE, which the
// caller then read as a pointer -- a segfault with no diagnostic, arbitrarily
// far from the `Channel(0)` that caused it.
//
// Every route reaches this, not just a literal: `fn channelOf(n) = Channel(n)`
// called with `0` poisons exactly the same handle, so the check belongs where
// the capacity is, not where it was written. [CONCURRENCY-CHANNEL]
int64_t channel_create_checked(int64_t capacity) {
  int64_t id = channel_create(capacity);
  if (id >= 1) {
    return id;
  }
  fprintf(stderr,
          "FATAL: Channel(%lld) has no channel to give: capacity must be "
          "positive and memory must be free (code %lld)\n",
          (long long)capacity, (long long)id);
  abort();
}

// A send that does not enqueue must not keep the +1 the caller transferred:
// ownership moved at the call, so a rejected value has no receiver and would
// leak for the life of the process. [GC-ARC-PERCEUS]
static int64_t channel_send_rejected(int64_t value, int64_t managed) {
  if (managed) {
    osp_release((void *)(uintptr_t)value);
  }
  return 0;
}

// Waits for a blocked channel to change: for another thread, or in sequential mode
// by running what is queued, as nothing else will. The channel's lock is held.
static void channel_wait(Channel *channel, pthread_cond_t *changed) {
  if (deterministic_mode) {
    pthread_mutex_unlock(&channel->mutex);
    bool ran = run_queued(INT64_MAX);
    pthread_mutex_lock(&channel->mutex);
    if (ran) {
      return;
    }
  }
  (void)osp_parked_wait(changed, &channel->mutex, NULL);
}

// Send value to channel. `managed` says the value is a heap object whose
// reference the channel now owns until a `recv` adopts it, or until
// `channel_cleanup` releases what was never received [CONCURRENCY-CHANNEL].
int64_t channel_send(int64_t channel_id, int64_t value, int64_t managed) {
  Channel *channel = channel_of(channel_id);
  if (!channel)
    return channel_send_rejected(value, managed);

  pthread_mutex_lock(&channel->mutex);
  if (managed) {
    channel->elem_managed = true;
  }

  // Wait while channel is full
  while (channel->count == channel->capacity) {
    channel_wait(channel, &channel->not_full);
  }

  // Add value to buffer
  channel->buffer[channel->tail] = value;
  channel->tail = (channel->tail + 1) % channel->capacity;
  channel->count++;

  // Signal that channel is not empty
  pthread_cond_signal(&channel->not_empty);

  pthread_mutex_unlock(&channel->mutex);
  select_notify();

  return 1; // Success
}

// Receive from channel
int64_t channel_recv(int64_t channel_id) {
  Channel *channel = channel_of(channel_id);
  if (!channel)
    return -1;

  pthread_mutex_lock(&channel->mutex);

  // Wait while channel is empty
  while (channel->count == 0) {
    channel_wait(channel, &channel->not_empty);
  }

  // Get value from buffer
  int64_t value = channel->buffer[channel->head];
  channel->head = (channel->head + 1) % channel->capacity;
  channel->count--;

  // Signal that channel is not full
  pthread_cond_signal(&channel->not_full);

  pthread_mutex_unlock(&channel->mutex);
  select_notify();

  return value;
}

#define SELECT_RECV 0
#define SELECT_SEND 1
#define SELECT_SEND_MANAGED 2

// Perform one arm's operation if its channel is ready right now.
static bool select_try(int64_t kind, int64_t channel_id, int64_t word, int64_t *received) {
  Channel *channel = channel_of(channel_id);
  if (!channel) {
    return false;
  }
  pthread_mutex_lock(&channel->mutex);
  bool ready = kind == SELECT_RECV ? channel->count > 0 : channel->count < channel->capacity;
  if (ready && kind == SELECT_RECV) {
    *received = channel->buffer[channel->head];
    channel->head = (channel->head + 1) % channel->capacity;
    channel->count--;
    pthread_cond_signal(&channel->not_full);
  } else if (ready) {
    channel->elem_managed = channel->elem_managed || kind == SELECT_SEND_MANAGED;
    channel->buffer[channel->tail] = word;
    channel->tail = (channel->tail + 1) % channel->capacity;
    channel->count++;
    pthread_cond_signal(&channel->not_empty);
  }
  pthread_mutex_unlock(&channel->mutex);
  if (ready) {
    select_notify();
  }
  return ready;
}

// The first ready arm in source order, or `count` when none is.
static int64_t select_scan(int64_t count, const int64_t *kinds, const int64_t *ids,
                           const int64_t *words, int64_t *received) {
  for (int64_t arm = 0; arm < count; arm++) {
    if (select_try(kinds[arm], ids[arm], words[arm], received)) {
      return arm;
    }
  }
  return count;
}

// Sleep until a channel changes or the deadline passes; false once it has passed.
static bool select_wait(uint64_t seen, bool bounded, const struct timespec *deadline) {
  bool in_time = true;
  pthread_mutex_lock(&select_mutex);
  while (select_generation == seen && in_time) {
    if (bounded) {
      in_time = osp_parked_wait(&select_changed, &select_mutex, deadline) != ETIMEDOUT;
    } else {
      (void)osp_parked_wait(&select_changed, &select_mutex, NULL);
    }
  }
  pthread_mutex_unlock(&select_mutex);
  return in_time;
}

static int64_t select_deterministic(int64_t count, const int64_t *kinds, const int64_t *ids,
                                    const int64_t *words, int64_t *received,
                                    int64_t timeout_ms) {
  int64_t chosen = select_scan(count, kinds, ids, words, received);
  while (chosen == count && timeout_ms != 0 && run_queued(INT64_MAX)) {
    chosen = select_scan(count, kinds, ids, words, received);
  }
  return chosen;
}

static int64_t select_until_ready(int64_t count, const int64_t *kinds, const int64_t *ids,
                                  const int64_t *words, int64_t *received,
                                  int64_t timeout_ms) {
  struct timespec deadline;
  clock_gettime(CLOCK_REALTIME, &deadline);
  if (timeout_ms > 0) {
    deadline.tv_sec += timeout_ms / 1000;
    deadline.tv_nsec += (timeout_ms % 1000) * 1000000;
    deadline.tv_sec += deadline.tv_nsec / 1000000000;
    deadline.tv_nsec %= 1000000000;
  }
  atomic_fetch_add(&select_waiters, 1);
  int64_t chosen = count;
  for (bool waiting = true; waiting;) {
    pthread_mutex_lock(&select_mutex);
    uint64_t seen = select_generation;
    pthread_mutex_unlock(&select_mutex);
    chosen = select_scan(count, kinds, ids, words, received);
    waiting = chosen == count && timeout_ms != 0 &&
              select_wait(seen, timeout_ms > 0, &deadline);
  }
  atomic_fetch_sub(&select_waiters, 1);
  return chosen;
}

// Run the channel arms of a `select`: `kinds[i]` is SELECT_RECV, SELECT_SEND or
// SELECT_SEND_MANAGED on channel `ids[i]`, sending `words[i]`. Answers the index of
// the first ready arm in source order, which has been performed, or `count` when
// none became ready within `timeout_ms` (0 never waits, negative waits for ever). A
// send that was not chosen releases the reference it would have handed over.
// [CONCURRENCY-SELECT]
int64_t channel_select(int64_t count, const int64_t *kinds, const int64_t *ids,
                       const int64_t *words, int64_t *received, int64_t timeout_ms) {
  int64_t chosen = deterministic_mode
                       ? select_deterministic(count, kinds, ids, words, received, timeout_ms)
                       : count;
  if (!deterministic_mode || (chosen == count && timeout_ms < 0)) {
    chosen = select_until_ready(count, kinds, ids, words, received, timeout_ms);
  }
  for (int64_t arm = 0; arm < count; arm++) {
    if (arm != chosen && kinds[arm] == SELECT_SEND_MANAGED) {
      osp_release((void *)(uintptr_t)words[arm]);
    }
  }
  return chosen;
}

// Release every managed value still buffered when the program ends. A value
// that was sent and never received holds the +1 `send` transferred to the
// channel, and nothing else can ever hand it back: `fiber_cleanup_results`
// walks fibers, not channels, so without this every unreceived managed send is
// a leak the ARC report counts at exit. Codegen calls this once at the end of
// main, after language owners have dropped. [GC-ARC-PERCEUS]
void channel_cleanup(void) {
  pthread_mutex_lock(&runtime_mutex);
  for (int64_t id = 1; id < next_id; id++) {
    Channel *channel = channels[id];
    if (!channel || !channel->elem_managed) {
      continue;
    }
    pthread_mutex_lock(&channel->mutex);
    while (channel->count > 0) {
      int64_t value = channel->buffer[channel->head];
      channel->head = (channel->head + 1) % channel->capacity;
      channel->count--;
      osp_release((void *)(uintptr_t)value);
    }
    pthread_mutex_unlock(&channel->mutex);
  }
  pthread_mutex_unlock(&runtime_mutex);
}

// Sleep for specified milliseconds. On Linux the profiler's directed SIGPROF
// would cut a plain usleep short (sleeping calls are exempt from SA_RESTART),
// so sleep to an absolute deadline and re-arm on EINTR — the wait stays exact
// whether or not sampling is active [PROF-COLLECT-SAMPLER].
static void sleep_for(int64_t milliseconds) {
#if defined(__linux__)
  struct timespec until;
  if (clock_gettime(CLOCK_MONOTONIC, &until) != 0) {
    usleep((unsigned int)(milliseconds * 1000));
    return;
  }
  until.tv_sec += (time_t)(milliseconds / 1000);
  until.tv_nsec += (long)(milliseconds % 1000) * 1000000L;
  if (until.tv_nsec >= 1000000000L) {
    until.tv_sec += 1;
    until.tv_nsec -= 1000000000L;
  }
  while (clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &until, NULL) ==
         EINTR) {
  }
#else
  usleep((unsigned int)(milliseconds * 1000)); // ms -> µs
#endif
}

// A sleeping fiber holds up no collection.
int64_t fiber_sleep(int64_t milliseconds) {
  if (milliseconds <= 0) {
    return 0;
  }
  jmp_buf registers;
  OSP_SPILL_REGISTERS(registers);
  osp_mem_baton_park(&registers);
  sleep_for(milliseconds);
  osp_mem_baton_unpark();
  return 0;
}

// Non-blocking completion probe. Returns 1 if the fiber has finished, 0 if it
// is still running, -1 for an invalid id. Lets a caller animate (sleep + redraw)
// while a fiber does real work, then `fiber_await` it the instant it reports
// done - without ever blocking the animating thread on a join or condvar.
int64_t fiber_done(int64_t fiber_id) {
  Fiber *fiber = fiber_of(fiber_id);
  if (!fiber)
    return -1;

  // Deterministic fibers only run when awaited, so report "ready" immediately;
  // the subsequent fiber_await drives execution. Avoids an infinite spin.
  if (!fiber->uses_thread)
    return 1;

  pthread_mutex_lock(&fiber->mutex);
  bool done = fiber->completed;
  pthread_mutex_unlock(&fiber->mutex);

  return done ? 1 : 0;
}

// iOS supports fibers but excludes the fork/exec process runtime. Keeping these
// wrappers in its archive would make ordinary fiber programs fail to link even
// when they never spawn an external process. [IOS-TARGET-CAPABILITIES]
#if !defined(OSPREY_IOS) && !defined(OSPREY_ANDROID)
extern int64_t await_process(int64_t process_id);
extern void cleanup_process(int64_t process_id);

int64_t fiber_await_process(int64_t process_id) {
  return await_process(process_id);
}

// Await process completion with stdout callback in fiber context
int64_t fiber_await_process_with_callback(int64_t process_id,
                                          void (*stdout_callback)(char *)) {
  if (!stdout_callback) {
    return await_process(process_id);
  }

  return fiber_await_process(process_id);
}

// Clean up process resources
void fiber_cleanup_process(int64_t process_id) { cleanup_process(process_id); }
#endif
