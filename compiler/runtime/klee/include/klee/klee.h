// The KLEE intrinsics the mock C library calls [SYMEX-MOCK-HEADERS]. These
// mirror KLEE's own `klee/klee.h` (credit: the KLEE project, klee.github.io)
// so the mock compiles and its native unit tests run on a machine without
// KLEE installed; under KLEE the engine itself implements them.
#ifndef OSP_HUNT_KLEE_H
#define OSP_HUNT_KLEE_H

#include <stddef.h>
#include <stdint.h>

void klee_make_symbolic(void *address, size_t count, const char *name);
int klee_range(int begin, int end, const char *name);
void klee_assume(uintptr_t condition);
__attribute__((noreturn)) void klee_report_error(const char *file, int line,
                                                 const char *message,
                                                 const char *suffix);

#endif
