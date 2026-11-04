// Stateless libc doubles beyond what KLEE's own libc and intrinsics provide
// [SYMEX-MOCK-STATELESS]: string search and parsing, qsort, the environment,
// entropy, the clock and single-thread pthread mutexes. Nothing here keeps
// state between calls except the counters the unit tests read.
#include "hunt_mock.h"

#include <errno.h>
#include <klee/klee.h>
#include <pthread.h>
#include <stdlib.h>
#include <string.h>
#include <sys/random.h>
#include <sys/time.h>
#include <time.h>
#include <unistd.h>

void osp_hunt_name(char *out, const char *base, const char *suffix) {
  size_t at = 0;
  for (; base[at] != '\0' && at + 1 < OSP_HUNT_NAME_MAX; at++) {
    out[at] = base[at];
  }
  for (size_t i = 0; suffix[i] != '\0' && at + 1 < OSP_HUNT_NAME_MAX; i++, at++) {
    out[at] = suffix[i];
  }
  out[at] = '\0';
}

void osp_hunt_name_indexed(char *out, const char *base, unsigned index) {
  char digits[12];
  int length = 0;
  do {
    digits[10 - length] = (char)('0' + index % 10);
    index /= 10;
    length++;
  } while (index != 0);
  digits[11] = '\0';
  char suffix[13] = ".";
  memcpy(suffix + 1, digits + 11 - length, (size_t)length + 1);
  osp_hunt_name(out, base, suffix);
}

// libc's search functions return a mutable pointer into a const argument; the
// cast through uintptr_t is that contract, not a defect in the caller.
static char *unconst(const char *text) { return (char *)(uintptr_t)text; }

char *strstr(const char *haystack, const char *needle) {
  size_t length = strlen(needle);
  for (; *haystack != '\0'; haystack++) {
    if (strncmp(haystack, needle, length) == 0) {
      return unconst(haystack);
    }
  }
  return length == 0 ? unconst(haystack) : NULL;
}

size_t strcspn(const char *text, const char *reject) {
  size_t count = 0;
  for (; text[count] != '\0' && strchr(reject, text[count]) == NULL; count++) {
  }
  return count;
}

size_t strspn(const char *text, const char *accept) {
  size_t count = 0;
  for (; text[count] != '\0' && strchr(accept, text[count]) != NULL; count++) {
  }
  return count;
}

char *strdup(const char *text) {
  size_t size = strlen(text) + 1;
  char *copy = malloc(size);
  if (copy != NULL) {
    memcpy(copy, text, size);
  }
  return copy;
}

int strerror_r(int err, char *out, size_t out_size) {
  char name[OSP_HUNT_NAME_MAX];
  osp_hunt_name_indexed(name, "errno", (unsigned)err);
  strncpy(out, name, out_size);
  if (out_size > 0) {
    out[out_size - 1] = '\0';
  }
  return 0;
}

char *strerror(int err) {
  static char text[OSP_HUNT_NAME_MAX];
  (void)strerror_r(err, text, sizeof(text));
  return text;
}

long long strtoll(const char *text, char **end, int base) { return strtol(text, end, base); }

unsigned long long strtoull(const char *text, char **end, int base) { return strtoul(text, end, base); }

static const char *skip_space(const char *text) {
  while (*text == ' ' || *text == '\t' || *text == '\n' || *text == '\r') {
    text++;
  }
  return text;
}

static const char *parse_digits(const char *text, double *value, int *digits) {
  for (; *text >= '0' && *text <= '9'; text++, (*digits)++) {
    *value = *value * 10.0 + (*text - '0');
  }
  return text;
}

static double scale_by_ten(double value, long exponent) {
  for (long i = 0; i < (exponent < 0 ? -exponent : exponent); i++) {
    value = exponent < 0 ? value / 10.0 : value * 10.0;
  }
  return value;
}

double strtod(const char *text, char **end) {
  const char *cursor = skip_space(text);
  int negative = *cursor == '-';
  cursor += (*cursor == '-' || *cursor == '+');
  double value = 0.0;
  int digits = 0;
  int fraction_digits = 0;
  cursor = parse_digits(cursor, &value, &digits);
  if (*cursor == '.') {
    cursor = parse_digits(cursor + 1, &value, &fraction_digits);
  }
  if (digits + fraction_digits == 0) {
    if (end != NULL) {
      *end = unconst(text);
    }
    return 0.0;
  }
  long exponent = -fraction_digits;
  if (*cursor == 'e' || *cursor == 'E') {
    char *after;
    long shift = strtol(cursor + 1, &after, 10);
    if (after != cursor + 1) {
      exponent += shift;
      cursor = after;
    }
  }
  if (end != NULL) {
    *end = unconst(cursor);
  }
  value = scale_by_ten(value, exponent);
  return negative ? -value : value;
}

static void swap_bytes(unsigned char *a, unsigned char *b, size_t size) {
  for (size_t i = 0; i < size; i++) {
    unsigned char byte = a[i];
    a[i] = b[i];
    b[i] = byte;
  }
}

// Insertion sort: the fewest comparisons to explore symbolically, and the
// sorted order is what matters, not the speed.
void qsort(void *base, size_t count, size_t size, int (*compare)(const void *, const void *)) {
  osp_hunt_note(OSP_HUNT_CALL_QSORT);
  unsigned char *items = base;
  for (size_t i = 1; i < count; i++) {
    for (size_t j = i; j > 0 && compare(items + (j - 1) * size, items + j * size) > 0; j--) {
      swap_bytes(items + (j - 1) * size, items + j * size, size);
    }
  }
}

// The explored program has no environment: every lookup misses.
char *getenv(const char *name) {
  (void)name;
  osp_hunt_note(OSP_HUNT_CALL_GETENV);
  return NULL;
}

static unsigned entropy_draws;

// Each draw is a fresh symbolic object `entropy.<n>`, so a bug that needs a
// particular random value is one path, not one in 2^64.
static void symbolic_bytes(void *out, size_t count) {
  osp_hunt_note(OSP_HUNT_CALL_ENTROPY);
  if (count == 0) {
    return;
  }
  char name[OSP_HUNT_NAME_MAX];
  osp_hunt_name_indexed(name, "entropy", entropy_draws++);
  klee_make_symbolic(out, count, name);
}

ssize_t getrandom(void *out, size_t count, unsigned int flags) {
  (void)flags;
  symbolic_bytes(out, count);
  return (ssize_t)count;
}

void arc4random_buf(void *out, size_t count) { symbolic_bytes(out, count); }

int getentropy(void *out, size_t count) {
  symbolic_bytes(out, count);
  return 0;
}

static unsigned clock_reads;
static long long clock_last;

// A symbolic, non-decreasing clock named `clock.<n>` in nanoseconds.
static long long symbolic_nanoseconds(void) {
  osp_hunt_note(OSP_HUNT_CALL_CLOCK);
  char name[OSP_HUNT_NAME_MAX];
  osp_hunt_name_indexed(name, "clock", clock_reads++);
  long long now;
  klee_make_symbolic(&now, sizeof(now), name);
  klee_assume(now >= clock_last);
  klee_assume(now < (1LL << 62));
  clock_last = now;
  return now;
}

int clock_gettime(clockid_t clock, struct timespec *out) {
  (void)clock;
  long long now = symbolic_nanoseconds();
  out->tv_sec = now / 1000000000LL;
  out->tv_nsec = now % 1000000000LL;
  return 0;
}

time_t time(time_t *out) {
  time_t now = symbolic_nanoseconds() / 1000000000LL;
  if (out != NULL) {
    *out = now;
  }
  return now;
}

int gettimeofday(struct timeval *out, void *zone) {
  (void)zone;
  long long now = symbolic_nanoseconds();
  out->tv_sec = now / 1000000000LL;
  out->tv_usec = now % 1000000000LL / 1000;
  return 0;
}

unsigned int sleep(unsigned int seconds) {
  (void)seconds;
  return 0;
}

int usleep(unsigned int microseconds) {
  (void)microseconds;
  return 0;
}

int fcntl(int fd, int command, ...) {
  (void)fd;
  (void)command;
  return 0;
}

// One thread explores the program, so a mutex only has to be balanced.
int pthread_mutex_init(pthread_mutex_t *mutex, const pthread_mutexattr_t *attributes) {
  (void)attributes;
  mutex->locked = 0;
  return 0;
}

int pthread_mutex_destroy(pthread_mutex_t *mutex) {
  (void)mutex;
  return 0;
}

int pthread_mutex_lock(pthread_mutex_t *mutex) {
  mutex->locked++;
  return 0;
}

int pthread_mutex_unlock(pthread_mutex_t *mutex) {
  mutex->locked--;
  return 0;
}

pthread_t pthread_self(void) { return 1; }
int pthread_equal(pthread_t first, pthread_t second) { return first == second; }
