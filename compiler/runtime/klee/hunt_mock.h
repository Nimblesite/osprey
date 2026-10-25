// Internal contract of the mock C library that stands in for the system libc
// when a program runs under KLEE [SYMEX-MOCK-CLOSED]. Every double records
// that it was reached in `osp_hunt_calls`, so the unit tests can prove the
// runtime still makes the underlying call, and every byte written leaves
// through `osp_hunt_sink`, so the tests can read it back.
//
// This header uses only the compiler's freestanding headers: the native unit
// tests include it beside the host libc, the doubles include it beside the
// mock headers in `include/`.
#ifndef OSP_HUNT_MOCK_H
#define OSP_HUNT_MOCK_H

#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>

enum {
  // Largest symbolic stream; `osp_hunt_input_bytes` picks the size in use.
  OSP_HUNT_STREAM_MAX = 4096,
  // Files open at once; `fopen` fails with ENFILE beyond this.
  OSP_HUNT_FILE_SLOTS = 8,
  // Longest symbolic object name, including the `.len` / `.ok` suffixes.
  OSP_HUNT_NAME_MAX = 128,
};

// The libc entry points a program can reach, counted per process.
typedef enum {
  OSP_HUNT_CALL_READ,
  OSP_HUNT_CALL_GETCHAR,
  OSP_HUNT_CALL_FOPEN,
  OSP_HUNT_CALL_FREAD,
  OSP_HUNT_CALL_FGETS,
  OSP_HUNT_CALL_FWRITE,
  OSP_HUNT_CALL_FCLOSE,
  OSP_HUNT_CALL_PRINTF,
  OSP_HUNT_CALL_FORMAT,
  OSP_HUNT_CALL_ENTROPY,
  OSP_HUNT_CALL_CLOCK,
  OSP_HUNT_CALL_GETENV,
  OSP_HUNT_CALL_QSORT,
  OSP_HUNT_CALL_MATH,
  OSP_HUNT_CALL_UNSUPPORTED,
  OSP_HUNT_CALL_COUNT
} osp_hunt_call;

extern unsigned osp_hunt_calls[OSP_HUNT_CALL_COUNT];
void osp_hunt_note(osp_hunt_call call);

// One stream [SYMEX-MOCK-STREAM]: readable streams carry symbolic bytes and a
// symbolic length, materialised on first use; writable streams forward to
// the sink. `len` is -1 until materialised.
struct osp_hunt_file {
  char name[OSP_HUNT_NAME_MAX];
  unsigned char *data;
  long len;
  long pos;
  unsigned char readable;
  unsigned char writable;
  unsigned char in_use;
  unsigned char error;
  unsigned char eof;
};

// Symbolic size of every readable stream. Weak so the tool's strong
// definition, appended to the program module, wins.
extern long osp_hunt_input_bytes;
// One byte beyond the input cap forces a short read; one holds the NUL.
size_t osp_hunt_file_buffer_capacity(void);
int osp_hunt_initial_input(char **out);

// Where written bytes go. The default discards them; tests capture them.
extern void (*osp_hunt_sink)(const char *data, size_t count);

// "<base><suffix>" clipped to OSP_HUNT_NAME_MAX.
void osp_hunt_name(char *out, const char *base, const char *suffix);
// "<base>.<index>" clipped to OSP_HUNT_NAME_MAX.
void osp_hunt_name_indexed(char *out, const char *base, unsigned index);

// Report an unmodelled libc call as `unmodelled.err` and stop the path
// [SYMEX-MOCK-UNSUPPORTED].
void osp_hunt_unsupported(const char *file, int line, const char *name)
    __attribute__((noreturn));

// The printf engine: formats into `emit(target, byte)` and returns the
// length the output would have, as snprintf does.
int osp_hunt_vformat(void (*emit)(void *target, char byte), void *target,
                     const char *format, va_list arguments);

// Exact decimal digits of a finite non-negative double: value =
// 0.d[0]d[1]... * 10^point, with no leading zero digit. Zero has count 0.
// 2^53 * 5^1074 has 767 digits; the extra byte absorbs a rounding carry.
enum { OSP_HUNT_DECIMAL_MAX = 800 };
struct osp_hunt_decimal {
  char digits[OSP_HUNT_DECIMAL_MAX];
  int count;
  int point;
};
void osp_hunt_decimal(double magnitude, struct osp_hunt_decimal *out);
// Rounds to the first `keep` digits, half to even on an exact tie, as the
// host libc does; a carry can lengthen the digits and move the point.
void osp_hunt_decimal_round(struct osp_hunt_decimal *d, int keep);

// Forgets every stream, counter and errno: the unit tests' seam between
// cases [SYMEX-MOCK-TESTS]. Under KLEE nothing calls it.
void osp_hunt_reset(void);

#endif
