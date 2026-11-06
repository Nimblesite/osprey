// Shared fixtures of the profiler C suite [PROF-TEST]
// (docs/specs/tooling/0028-Profiler.md): profiler_runtime_tests.c owns main
// and the helpers below, profiler_walk_tests.c and profiler_spec_tests.c add
// their runners.
#ifndef OSPREY_PROFILER_TESTS_H
#define OSPREY_PROFILER_TESTS_H

#include "profiler_runtime.h"

#include <assert.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

bool osp_prof_start(const char *out_path, uint32_t rate_hz);
void osp_prof_stop_and_dump(void);

enum { TEST_RATE_HZ = 2000, FAKE_STACK_WORDS = 64 };

// Keeps busy_work's result alive so the optimizer cannot delete the loop.
extern volatile double g_sink;
// Burns CPU in a frame of its own; noinline so it shows up in a stack.
double busy_work(long n);
// The whole file at `path`, NUL-terminated; the caller frees it.
char *slurp(const char *path);

void run_walk_tests(void);
void run_spec_tests(void);

#endif // OSPREY_PROFILER_TESTS_H
