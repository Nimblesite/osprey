// [ORCHESTRA-ENGINE] Isolation is required to race the producer against the device callback under sanitizers.
#include "../../../hosts/swift/orchestra/audio_ring.h"
#include <assert.h>
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdio.h>
#define SAMPLES 2000000u
static _Atomic int complete;
static void *produce(void *unused) {
    (void)unused;
    size_t written = 0;
    float samples[173];
    while (written < SAMPLES) {
        size_t count = SAMPLES - written < 173 ? SAMPLES - written : 173;
        for (size_t i = 0; i < count; ++i) samples[i] = (float)((written + i) % 1024 + 1);
        written += orchestra_ring_write(samples, count);
        assert(orchestra_ring_queued() <= 8192);
        sched_yield();
    }
    atomic_store_explicit(&complete, 1, memory_order_release);
    return NULL;
}
int main(void) {
    assert(orchestra_ring_lock_free());
    assert(orchestra_ring_max_read() == 0);
    float samples[257];
    orchestra_ring_read(samples, 257);
    assert(orchestra_ring_max_read() == 257);
    for (size_t i = 0; i < 257; ++i) assert(samples[i] == 0);
    assert(orchestra_ring_underruns() == 1);
    pthread_t producer;
    assert(pthread_create(&producer, NULL, produce, NULL) == 0);
    size_t consumed = 0;
    while (consumed < SAMPLES || !atomic_load_explicit(&complete, memory_order_acquire)) {
        orchestra_ring_read(samples, 257);
        for (size_t i = 0; i < 257; ++i) if (samples[i] != 0) {
            assert(samples[i] == (float)(consumed % 1024 + 1));
            ++consumed;
        }
        sched_yield();
    }
    assert(pthread_join(producer, NULL) == 0);
    assert(consumed == SAMPLES && orchestra_ring_queued() == 0);
    orchestra_ring_read(samples, 17);
    assert(orchestra_ring_max_read() == 257);
    puts("PASS: two million ordered samples across ring wraparound and underruns");
    return 0;
}
