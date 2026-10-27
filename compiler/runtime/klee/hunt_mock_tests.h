// The native test harness the mock C library's suites share
// [SYMEX-MOCK-STREAM] [SYMEX-MOCK-STATELESS] [SYMEX-MOCK-UNSUPPORTED]:
// reporting, the KLEE shim's queues, and the doubles under test declared by
// their libc names. Each suite is one `hunt_mock_*_tests.c` file; main lives
// in hunt_mock_tests.c.
#ifndef OSP_HUNT_MOCK_TESTS_H
#define OSP_HUNT_MOCK_TESTS_H

#include "hunt_mock.h"

#include <stddef.h>

// ---- reporting and the KLEE shim -------------------------------------------

void hunt_check(int condition, const char *what);
// Clears the shim's queues, the recorded names and every mock stream.
void hunt_reset(void);
// Bytes the next `klee_make_symbolic` calls take, in order.
void hunt_queue_bytes(const char *bytes, size_t count);
// Values the next `klee_range` calls return, in order (clamped to the range).
void hunt_queue_range(int value);
void hunt_set_object_size(size_t size);
void hunt_mock_parse_tests(void);
// The name of the `at`th `klee_range` call since the last reset.
const char *hunt_range_name(size_t at);
size_t hunt_range_count(void);
// The name of the last `klee_make_symbolic` call.
const char *hunt_last_symbolic(void);

// ---- the doubles under test, through their libc names ----------------------

char *osp_input(void);

typedef struct osp_hunt_file FILE_;
FILE_ *fopen(const char *path, const char *mode);
int fclose(FILE_ *stream);
size_t fread(void *out, size_t size, size_t count, FILE_ *stream);
size_t fwrite(const void *data, size_t size, size_t count, FILE_ *stream);
char *fgets(char *out, int cap, FILE_ *stream);
int fgetc(FILE_ *stream);
int fseeko(FILE_ *stream, long offset, int whence);
long ftello(FILE_ *stream);
void rewind(FILE_ *stream);
int feof(FILE_ *stream);
int ferror(FILE_ *stream);
void clearerr(FILE_ *stream);
long read(int fd, void *out, size_t count);
long write(int fd, const void *data, size_t count);
int fprintf(FILE_ *stream, const char *format, ...);
int snprintf(char *out, size_t cap, const char *format, ...);
int sprintf(char *out, const char *format, ...);
int printf(const char *format, ...);
int puts(const char *text);
int getchar(void);
void qsort(void *base, size_t count, size_t size, int (*compare)(const void *, const void *));
char *getenv(const char *name);
double strtod(const char *text, char **end);
char *strstr(const char *haystack, const char *needle);
size_t strcspn(const char *text, const char *reject);
void arc4random_buf(void *out, size_t count);
int fork(void);
double sqrt(double x);
double exp(double x);
double log(double x);
double tanh(double x);
double pow(double base, double exponent);
double floor(double x);
double fmod(double x, double y);
extern FILE_ *osp_hunt_stdout;
extern FILE_ *osp_hunt_stdin;
extern int osp_hunt_errno;

// Runtime integration exercised by both suites.
char *read_file(char *path);

// ---- captured output -------------------------------------------------------

// Routes the mock's sink into a buffer and empties it.
void hunt_capture_start(void);
const char *hunt_captured(void);

// ---- the suites --------------------------------------------------------------

void hunt_mock_format_tests(void);
void hunt_mock_stream_tests(void);

#endif
