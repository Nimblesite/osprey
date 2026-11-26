// The fiber runtime's C entry points, for the runtime's own units and their
// tests. A Fiber is reached through a hold ([CONCURRENCY-FIBER-LIFETIME]):
// fiber_spawn_env returns the caller's, fiber_join reads a fiber the caller
// holds, and fiber_release ends a hold. Compiled programs hold fibers through
// managed handles instead (fiber_spawn_env_owned, fiber_await, fiber_done).
// A channel is always reached through its managed handle
// ([CONCURRENCY-CHANNEL-LIFETIME]).
#ifndef OSPREY_FIBER_API_H
#define OSPREY_FIBER_API_H

#include <stddef.h>
#include <stdint.h>

typedef struct Fiber Fiber;

Fiber *fiber_spawn_env(int64_t (*fn)(void *), void *env);
int64_t fiber_join(Fiber *fiber);
void fiber_release(Fiber *fiber);

// What every spawn asks for first: one fiber record.
extern const size_t fiber_record_bytes;

void *fiber_spawn_env_owned(int64_t (*fn)(void *), void *env, int64_t result_managed);
int64_t fiber_await(void *handle);
int64_t fiber_done(void *handle);
int64_t fiber_id(void *handle);

void *channel_create(int64_t capacity);
int64_t channel_send(void *handle, int64_t value, int64_t managed);
int64_t channel_recv(void *handle);
int64_t channel_select(int64_t count, const int64_t *kinds, void *const *channels,
                       const int64_t *words, int64_t *received, int64_t timeout_ms);

#endif
