// Mock <stdlib.h> for symbolic execution [SYMEX-MOCK-HEADERS]. malloc, calloc,
// realloc, free, abort, exit and atexit are KLEE intrinsics; strtol, strtoul
// and atoi come from `--libc=klee`; the rest is hunt_libc.c.
#ifndef OSP_HUNT_STDLIB_H
#define OSP_HUNT_STDLIB_H
#include <stddef.h>

// macOS declares the BSD entropy call here, and random_runtime.c reaches for
// it under __APPLE__ when the mock is built for the host [SYMEX-MOCK-TESTS].
void arc4random_buf(void *out, size_t count);
#define EXIT_SUCCESS 0
#define EXIT_FAILURE 1
void *malloc(size_t size);
void *calloc(size_t count, size_t size);
void *realloc(void *ptr, size_t size);
void free(void *ptr);
void abort(void) __attribute__((noreturn));
void exit(int status) __attribute__((noreturn));
void _Exit(int status) __attribute__((noreturn));
int atexit(void (*function)(void));
long strtol(const char *text, char **end, int base);
unsigned long strtoul(const char *text, char **end, int base);
long long strtoll(const char *text, char **end, int base);
unsigned long long strtoull(const char *text, char **end, int base);
double strtod(const char *text, char **end);
int atoi(const char *text);
void qsort(void *base, size_t count, size_t size,
           int (*compare)(const void *, const void *));
char *getenv(const char *name);
static inline int abs(int value) { return value < 0 ? -value : value; }
static inline long labs(long value) { return value < 0 ? -value : value; }
static inline long long llabs(long long value) { return value < 0 ? -value : value; }
#endif
