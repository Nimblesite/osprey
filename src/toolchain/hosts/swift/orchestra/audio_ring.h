#pragma once
#include <stddef.h>
#include <stdint.h>
// [SYNTH-RENDER] One preallocated SPSC stream: the worker writes; the device callback reads.
int orchestra_ring_lock_free(void);
size_t orchestra_ring_queued(void);
size_t orchestra_ring_write(const float *samples, size_t count);
void orchestra_ring_read(float *samples, size_t count);
uint64_t orchestra_ring_underruns(void);
uint64_t orchestra_ring_max_read(void);
