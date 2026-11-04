// Mock <assert.h> for symbolic execution [SYMEX-MOCK-HEADERS]: a failed
// assertion is KLEE's `__assert_fail` intrinsic, reported as `assert.err`.
#ifndef OSP_HUNT_ASSERT_H
#define OSP_HUNT_ASSERT_H
void __assert_fail(const char *expression, const char *file, unsigned line,
                   const char *function) __attribute__((noreturn));
#ifdef NDEBUG
#define assert(expression) ((void)0)
#else
#define assert(expression) \
  ((expression) ? (void)0 : __assert_fail(#expression, __FILE__, __LINE__, __func__))
#endif
#endif
