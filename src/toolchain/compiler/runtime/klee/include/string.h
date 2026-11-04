// Mock <string.h> for symbolic execution [SYMEX-MOCK-HEADERS]. memcpy, memmove,
// memset and memcmp are KLEE's freestanding runtime; strlen, strcmp, strncmp,
// strchr, strrchr, strcpy, strncpy, strcat and memchr are `--libc=klee`; the
// rest is hunt_libc.c.
#ifndef OSP_HUNT_STRING_H
#define OSP_HUNT_STRING_H
#include <stddef.h>
void *memcpy(void *dst, const void *src, size_t n);
void *memmove(void *dst, const void *src, size_t n);
void *memset(void *dst, int byte, size_t n);
int memcmp(const void *a, const void *b, size_t n);
void *memchr(const void *s, int byte, size_t n);
size_t strlen(const char *s);
int strcmp(const char *a, const char *b);
int strncmp(const char *a, const char *b, size_t n);
char *strchr(const char *s, int c);
char *strrchr(const char *s, int c);
char *strcpy(char *dst, const char *src);
char *strncpy(char *dst, const char *src, size_t n);
char *strcat(char *dst, const char *src);
char *strstr(const char *haystack, const char *needle);
size_t strcspn(const char *s, const char *reject);
size_t strspn(const char *s, const char *accept);
char *strdup(const char *s);
char *strerror(int err);
int strerror_r(int err, char *out, size_t out_size);
#endif
