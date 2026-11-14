#include "audio_ring.h"
#include <stdatomic.h>
#include <string.h>
#define CAPACITY 8192u
static float storage[CAPACITY];
static _Atomic uint64_t read_position;
static _Atomic uint64_t write_position;
static _Atomic uint64_t underruns;
static _Atomic uint64_t max_read;

int orchestra_ring_lock_free(void) {
    return atomic_is_lock_free(&read_position) && atomic_is_lock_free(&write_position)
        && atomic_is_lock_free(&underruns) && atomic_is_lock_free(&max_read);
}
size_t orchestra_ring_queued(void) {
    uint64_t written = atomic_load_explicit(&write_position, memory_order_acquire);
    uint64_t read = atomic_load_explicit(&read_position, memory_order_acquire);
    return (size_t)(written - read);
}
size_t orchestra_ring_write(const float *samples, size_t count) {
    uint64_t written = atomic_load_explicit(&write_position, memory_order_relaxed);
    uint64_t read = atomic_load_explicit(&read_position, memory_order_acquire);
    size_t free = CAPACITY - (size_t)(written - read);
    if (count > free) count = free;
    for (size_t i = 0; i < count; ++i) storage[(written + i) % CAPACITY] = samples[i];
    atomic_store_explicit(&write_position, written + count, memory_order_release);
    return count;
}
void orchestra_ring_read(float *samples, size_t count) {
    // The sole consumer owns this maximum. The producer only samples it for diagnostics.
    if (count > atomic_load_explicit(&max_read, memory_order_relaxed))
        atomic_store_explicit(&max_read, count, memory_order_relaxed);
    uint64_t read = atomic_load_explicit(&read_position, memory_order_relaxed);
    uint64_t written = atomic_load_explicit(&write_position, memory_order_acquire);
    size_t available = (size_t)(written - read);
    size_t taken = count < available ? count : available;
    for (size_t i = 0; i < taken; ++i) samples[i] = storage[(read + i) % CAPACITY];
    if (taken < count) {
        memset(samples + taken, 0, (count - taken) * sizeof(float));
        atomic_fetch_add_explicit(&underruns, 1, memory_order_relaxed);
    }
    atomic_store_explicit(&read_position, read + taken, memory_order_release);
}
uint64_t orchestra_ring_underruns(void) { return atomic_load_explicit(&underruns, memory_order_relaxed); }
uint64_t orchestra_ring_max_read(void) { return atomic_load_explicit(&max_read, memory_order_relaxed); }
