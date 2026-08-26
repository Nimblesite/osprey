// Shared between the two translation units of the builtins suite: the
// behaviour half (builtins_runtime_tests.c) and the entropy-source half
// (random_entropy_tests.c).
//
// They are one BINARY -- gcov measures random_runtime.c across the whole suite
// -- but two objects, because the `getrandom`/`fopen` interposers the entropy
// half installs replace those symbols for every caller in the image, and the
// behaviour half's stdin/TAP scenarios must not run under them. The assertion
// counter is declared here rather than duplicated: main prints one total, and
// two copies would print the wrong one.
#ifndef OSPREY_BUILTINS_TESTS_SHARED_H
#define OSPREY_BUILTINS_TESTS_SHARED_H

#include <assert.h>
#include <stddef.h>
#include <stdint.h>

int64_t osp_random(void);
int64_t osp_random_below(int64_t n);
void osp_random_bytes(void *buf, size_t len);

extern long g_checks;
#define CHECK(c)                                                               \
  do {                                                                         \
    g_checks++;                                                                \
    assert(c);                                                                 \
  } while (0)

// The entropy half: what `osp_random_bytes` writes, and what it does when the
// OS source cannot supply it [BUILTIN-RANDOM].
void t_entropy_source(void);

#endif // OSPREY_BUILTINS_TESTS_SHARED_H
