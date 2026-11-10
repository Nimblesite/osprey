// Buffered channels and `select` [CONCURRENCY-CHANNEL] [CONCURRENCY-SELECT],
// included once by fiber_runtime.c after the fiber scheduler, whose handle
// tables, runtime lock and deterministic queue it shares: fibers and channels
// draw their ids from one space, and a deterministic `select` runs queued
// fibers until an arm is ready.
#ifndef OSPREY_FIBER_CHANNELS_H
#define OSPREY_FIBER_CHANNELS_H

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

#endif
