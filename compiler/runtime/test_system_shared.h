// Shared between the two translation units of the system-runtime suite.
//
// They are one BINARY -- coverage of system_runtime.c is measured across the
// whole suite, not per file -- but two objects, because the allocator, fork,
// thread and mutex interposers that the failure tests need may be defined in
// exactly one of them. Everything the behavioural half already had stays
// there; only what both halves touch is declared here.
#ifndef OSPREY_TEST_SYSTEM_SHARED_H
#define OSPREY_TEST_SYSTEM_SHARED_H

#include <stdint.h>

// Records every event the runtime reports about a process.
void capture_handler(int64_t process_id, int64_t event_type, char *data);

// Every half-built teardown: the record, its mutex, its command copy, its
// pipes, its fork, and its monitor thread. Runs FIRST -- the seams need a
// process with one thread and no children of its own.
void run_spawn_failure_tests(void);

// The out-of-descriptors and out-of-handles paths. Runs LAST: it burns the
// handle space for the rest of the program.
void run_descriptor_exhaustion_tests(void);

#endif // OSPREY_TEST_SYSTEM_SHARED_H
