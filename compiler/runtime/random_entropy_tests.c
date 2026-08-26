// What `osp_random_bytes` writes, and what it does when the OS entropy source
// cannot supply it [BUILTIN-RANDOM]. Second translation unit of the builtins
// suite; see builtins_tests_shared.h for why it is separate.
//
// The spec says a draw is "cryptographically-secure ... drawn fresh from the
// operating system's CSPRNG (arc4random_buf on macOS/BSD, getrandom(2) on
// Linux, falling back to /dev/urandom)". Two of those three words are only
// testable by taking the source away: on a healthy machine getrandom(2) never
// short-reads and never fails, so the fallback and the exhaustion guard below
// it are unreachable -- and an untested fallback is a fallback that does not
// work. Both are reached here by replacing `getrandom` and `fopen` for this
// image, the same static-link interposition test_spawn_seam.h uses for fork.
#include <stdio.h>
#include <string.h>

#include "builtins_tests_shared.h"

// The widest draw any caller makes is 8 bytes (`osp_random`, and the WebSocket
// handshake nonce in http_shared.c). PAD brackets it on both sides so an
// off-by-one in either direction lands in a guarded byte.
enum { PROBE_LEN = 8, PAD = 8, GUARD_BYTE = 0x5A };

// [BUILTIN-RANDOM]: a draw fills exactly the bytes it was asked for. Nothing
// else in the suite calls `osp_random_bytes` directly, so without this the
// length arithmetic in every branch is exercised at one single length -- and an
// overrun there writes past a caller's `uint64_t` on the stack.
static void a_draw_writes_exactly_what_was_asked_for(void) {
  for (size_t len = 1; len <= PROBE_LEN; len++) {
    unsigned char buf[PROBE_LEN + 2 * PAD];
    memset(buf, GUARD_BYTE, sizeof(buf));
    osp_random_bytes(buf + PAD, len);
    for (size_t i = 0; i < PAD; i++) {
      CHECK(buf[i] == GUARD_BYTE); // nothing written before the buffer
    }
    for (size_t i = PAD + len; i < sizeof(buf); i++) {
      CHECK(buf[i] == GUARD_BYTE); // nothing written after it
    }
  }
}

#ifdef __linux__
#include <dlfcn.h>
#include <errno.h>
#include <signal.h>
#include <stdlib.h>
#include <sys/random.h>
#include <sys/types.h>
#include <unistd.h>

#include "test_death.h"

// How the interposed `getrandom` answers. REAL forwards to libc; the rest are
// the states a healthy kernel will not produce on demand.
enum GetrandomMode {
  GR_REAL = 0,
  GR_DENY,               // the syscall is gone: every call fails
  GR_ONE_BYTE,           // the shortest possible short read, every time
  GR_PARTIAL_THEN_DENY   // some bytes, then nothing -- the splice case
};

enum { PARTIAL_LEN = 4, PARTIAL_BYTE = 0xAA };

static enum GetrandomMode gr_mode;
static unsigned gr_calls;
static size_t gr_asked[PROBE_LEN];
static ssize_t (*gr_real)(void *, size_t, unsigned int);

// Both interposers replace their symbol for the WHOLE image the moment this
// object is linked in, so they are live from before `main` -- libc and libgcov
// reach them too. Each therefore resolves its real implementation on first use
// rather than waiting to be armed. `resolve_real_symbols` then forces both
// BEFORE the first fork, so `dlsym` can never be reached from the SIGABRT
// handler: it takes the loader's lock, and a handler that takes a lock the
// interrupted thread may already hold deadlocks the child instead of killing
// it (test_death.h).
static void resolve_real_symbols(void);

ssize_t getrandom(void *buffer, size_t length, unsigned int flags) {
  if (gr_calls < PROBE_LEN) {
    gr_asked[gr_calls] = length;
  }
  gr_calls++;
  switch (gr_mode) {
  case GR_DENY:
    errno = ENOSYS;
    return -1;
  case GR_ONE_BYTE:
    if (length == 0) {
      return 0;
    }
    // A counter, not entropy: the test asserts the exact bytes that land, so
    // it can tell "byte 3 went to offset 2" from "byte 3 went anywhere".
    ((unsigned char *)buffer)[0] = (unsigned char)gr_calls;
    return 1;
  case GR_PARTIAL_THEN_DENY:
    if (gr_calls > 1) {
      errno = ENOSYS;
      return -1;
    }
    memset(buffer, PARTIAL_BYTE, PARTIAL_LEN);
    return (ssize_t)PARTIAL_LEN;
  case GR_REAL:
  default:
    break;
  }
  if (gr_real == NULL) {
    *(void **)&gr_real = dlsym(RTLD_NEXT, "getrandom");
  }
  if (gr_real == NULL) {
    errno = ENOSYS;
    return -1;
  }
  return gr_real(buffer, length, flags);
}

static int urandom_denied;
static FILE *(*fopen_real)(const char *, const char *);

// Only `/dev/urandom` is refused. Everything else must still open, because
// libgcov writes this suite's own .gcda files through `fopen` -- from inside
// the SIGABRT handler of the death test below.
FILE *fopen(const char *pathname, const char *mode) {
  if (urandom_denied && pathname != NULL &&
      strcmp(pathname, "/dev/urandom") == 0) {
    errno = EACCES;
    return NULL;
  }
  if (fopen_real == NULL) {
    *(void **)&fopen_real = dlsym(RTLD_NEXT, "fopen");
  }
  if (fopen_real == NULL) {
    errno = ENOSYS;
    return NULL;
  }
  return fopen_real(pathname, mode);
}

static void resolve_real_symbols(void) {
  unsigned char probe = 0;
  CHECK(getrandom(&probe, sizeof(probe), 0) == 1); // drives the lazy resolve
  CHECK(gr_real != NULL);
  FILE *self = fopen("/dev/null", "rb");
  CHECK(self != NULL);
  CHECK(fclose(self) == 0);
  CHECK(fopen_real != NULL);
}

static void arm(enum GetrandomMode mode) {
  gr_mode = mode;
  gr_calls = 0;
}

static void disarm(void) {
  gr_mode = GR_REAL;
  urandom_denied = 0;
}

// [BUILTIN-RANDOM]: `getrandom(2)` is allowed to return fewer bytes than asked
// for, so the loop must resume at the offset it reached and ask only for what
// is left. One byte per call is the worst case, and it pins both halves of that
// arithmetic: the destination advances and the remaining length shrinks.
static void b_a_short_source_is_drained_in_order(void) {
  unsigned char got[PROBE_LEN];
  memset(got, 0, sizeof(got));
  arm(GR_ONE_BYTE);
  osp_random_bytes(got, sizeof(got));
  disarm();

  CHECK(gr_calls == PROBE_LEN); // one call per byte, and not one more
  for (size_t i = 0; i < PROBE_LEN; i++) {
    CHECK(gr_asked[i] == PROBE_LEN - i); // asked for exactly what was left
    CHECK(got[i] == (unsigned char)(i + 1)); // in order, none skipped or reused
  }
}

enum { FALLBACK_DRAWS = 32 };

// [BUILTIN-RANDOM]: "falling back to /dev/urandom". With the syscall gone the
// fallback is the only source, and it must still deliver entropy -- not zeros,
// not a stale stack word. Distinctness is the observable: 32 draws from a real
// CSPRNG collide with probability about 2.7e-17, while any of the failure
// modes this replaced repeats immediately.
static void c_the_fallback_supplies_real_entropy(void) {
  uint64_t drawn[FALLBACK_DRAWS];
  arm(GR_DENY);
  for (size_t i = 0; i < FALLBACK_DRAWS; i++) {
    drawn[i] = 0;
    osp_random_bytes(&drawn[i], sizeof(drawn[i]));
    CHECK(drawn[i] != 0); // the buffer was written, not left as it was found
  }
  for (size_t i = 0; i < FALLBACK_DRAWS; i++) {
    for (size_t j = i + 1; j < FALLBACK_DRAWS; j++) {
      CHECK(drawn[i] != drawn[j]);
    }
  }
  // The published builtins keep their contracts on this source too.
  for (size_t i = 0; i < FALLBACK_DRAWS; i++) {
    CHECK(osp_random() >= 0);
    int64_t below = osp_random_below((int64_t)PROBE_LEN);
    CHECK(below >= 0 && below < (int64_t)PROBE_LEN);
  }
  disarm();
}

enum { SPLICE_ROUNDS = 4 };

// A partial draw followed by failure must be DISCARDED, not patched: the
// fallback refills from offset 0, so no buffer is half one source and half
// another. The surviving-prefix check is probabilistic in the honest
// direction -- a real refill reproduces the 0xAA prefix with probability
// 2^-32 per round, so all four rounds reproducing it is 2^-128.
static void d_a_partial_draw_is_discarded_not_patched(void) {
  int prefix_survived_every_round = 1;
  for (size_t round = 0; round < SPLICE_ROUNDS; round++) {
    unsigned char got[PROBE_LEN];
    static const unsigned char partial[PARTIAL_LEN] = {
        PARTIAL_BYTE, PARTIAL_BYTE, PARTIAL_BYTE, PARTIAL_BYTE};
    memset(got, 0, sizeof(got));
    arm(GR_PARTIAL_THEN_DENY);
    osp_random_bytes(got, sizeof(got));
    disarm();
    CHECK(gr_calls == 2); // one partial draw, one refusal, then the fallback
    if (memcmp(got, partial, sizeof(partial)) != 0) {
      prefix_survived_every_round = 0;
    }
  }
  CHECK(!prefix_survived_every_round);
}

// Redirecting stderr to a file makes it FULLY BUFFERED, and abort() does not
// flush stdio -- so this also pins the guard's own fflush. Deleting that line
// leaves this file empty and the assertion below red. Written next to the
// suite binary and removed once read.
#define DIAG_PATH "osp_entropy_diag.tmp"

static void body_with_no_entropy_source(void) {
  if (freopen(DIAG_PATH, "w", stderr) == NULL) {
    // Not an assert: a failed assert raises SIGABRT, which is exactly what
    // this test is looking for, so a broken setup would read as a pass. A
    // distinct signal cannot be mistaken for the guard.
    raise(SIGUSR1);
  }
  arm(GR_DENY);
  urandom_denied = 1;
  (void)osp_random();
}

static void body_with_the_fallback_available(void) {
  arm(GR_DENY);
  (void)osp_random();
}

enum { DIAG_CAP = 256 };

// The exact text, not "it said something": the byte counts in it are the
// guard's own account of what it could not get, and a guard that reports the
// wrong shortfall is a guard nobody can act on.
static void diagnostic_names_the_shortfall(void) {
  char text[DIAG_CAP];
  memset(text, 0, sizeof(text));
  FILE *f = fopen_real(DIAG_PATH, "rb");
  CHECK(f != NULL);
  size_t n = fread(text, 1, sizeof(text) - 1, f);
  fclose(f);
  CHECK(n > 0);
  CHECK(strcmp(text, "FATAL: the OS entropy source gave 0 of the 8 bytes "
                     "random() needs; there is no unpredictable value to "
                     "return\n") == 0);
  CHECK(remove(DIAG_PATH) == 0);
}

// [BUILTIN-RANDOM]: `random()` answers `int`, not `Result<int, Error>`, so a
// draw that cannot be made has nowhere to report. Returning zeros or an
// unwritten stack word would be a predictable "cryptographically-secure"
// value -- undetectable by the caller, and worst exactly where it is used as a
// handshake nonce. The process stops instead.
static void e_no_entropy_source_is_fatal(void) {
  CHECK(osp_death_signal(body_with_no_entropy_source) == SIGABRT);
  diagnostic_names_the_shortfall();
  // The pair that stops this from passing vacuously: the same body with the
  // fallback left alone must run to completion. A guard that fires on every
  // draw would satisfy the assertion above and fail this one.
  CHECK(osp_death_signal(body_with_the_fallback_available) == 0);
}
#endif // __linux__

void t_entropy_source(void) {
  a_draw_writes_exactly_what_was_asked_for();
#ifdef __linux__
  // Forces both interposers to find their real implementation while ordinary
  // code is running. Everything after this may fork, and the death test's
  // SIGABRT handler must never be the first caller: `dlsym` takes the loader's
  // lock, and a handler that takes a lock the interrupted thread already holds
  // wedges the child instead of killing it.
  resolve_real_symbols();
  b_a_short_source_is_drained_in_order();
  c_the_fallback_supplies_real_entropy();
  d_a_partial_draw_is_discarded_not_patched();
  e_no_entropy_source_is_fatal();
#endif
}
