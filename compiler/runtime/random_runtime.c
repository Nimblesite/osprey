// Cryptographically-secure random + stdin line reader runtime.
// Implements [BUILTIN-RANDOM], [BUILTIN-RANDOM-BELOW], [BUILTIN-INPUT].
//
// Entropy comes straight from the OS CSPRNG — arc4random_buf on macOS/BSD,
// getrandom(2) on Linux (falling back to /dev/urandom) — so the stream is
// unpredictable and carries no userspace seed/state. That makes it suitable
// both for security use and for the benchmark suite's "randomized" input mode,
// where a run draws a fresh seed each time. The matching "constant" mode never
// calls these and stays byte-for-byte deterministic.

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(__wasm__)
// wasm32-wasip1 gets its entropy from the WASI `random_get` host call, which
// wasi-libc exposes as getentropy(2) — the same OS-CSPRNG contract as the
// branches below, and the only one available here: there is no /dev to fall
// back to. Declared locally because wasi-libc hides both arc4random_buf and
// getentropy behind _BSD_SOURCE/_GNU_SOURCE, and the runtime builds -std=c11;
// defining a feature macro to reach one symbol would change every other
// translation unit in the archive too. Without this the whole TU stayed out of
// the wasm archive and every program touching `random` link-failed.
// [WASM-TARGET] [BUILTIN-RANDOM]
#define OSP_HAVE_GETENTROPY 1
int getentropy(void *, size_t);
#elif defined(__APPLE__) || defined(__FreeBSD__) || defined(__OpenBSD__) ||    \
    defined(__NetBSD__)
#define OSP_HAVE_ARC4RANDOM 1
#elif defined(__linux__)
#include <sys/random.h>
#include <sys/types.h> // ssize_t for the getrandom(2) return
#define OSP_HAVE_GETRANDOM 1
#endif

// Drain the OS entropy source into `buf`. Best-effort on the /dev/urandom
// fallback path: a short read leaves the tail zeroed rather than aborting,
// which never happens on the supported platforms.
static void osp_entropy(void *buf, size_t len) {
#ifdef OSP_HAVE_GETENTROPY
  // getentropy caps a single call at 256 bytes; every caller here asks for 8.
  if (getentropy(buf, len) != 0) {
    memset(buf, 0, len);
  }
#elif defined(OSP_HAVE_ARC4RANDOM)
  arc4random_buf(buf, len);
#else
#ifdef OSP_HAVE_GETRANDOM
  size_t off = 0;
  while (off < len) {
    ssize_t n = getrandom((unsigned char *)buf + off, len - off, 0);
    if (n <= 0) {
      break;
    }
    off += (size_t)n;
  }
  if (off >= len) {
    return;
  }
#endif
  FILE *f = fopen("/dev/urandom", "rb");
  if (f != NULL) {
    size_t got = fread(buf, 1, len, f);
    (void)got;
    fclose(f);
  }
#endif
}

// Clears the sign bit so a drawn word is a non-negative int63.
#define OSP_SIGN_MASK 0x7FFFFFFFFFFFFFFFLL

// Implements [BUILTIN-RANDOM]: a uniform non-negative random int (0 .. 2^63-1).
int64_t osp_random(void) {
  uint64_t v;
  osp_entropy(&v, sizeof(v));
  return (int64_t)(v & (uint64_t)OSP_SIGN_MASK);
}

// Implements [BUILTIN-RANDOM-BELOW]: a uniform random int in [0, n), unbiased
// by rejection sampling (every residue class is equally likely). Returns -1
// when n <= 0, which the codegen wraps as Error per the Result<int> discipline;
// on success the value is always non-negative, so the sentinel is unambiguous.
int64_t osp_random_below(int64_t n) {
  if (n <= 0) {
    return -1;
  }
  uint64_t bound = (uint64_t)n;
  // 2^64 mod bound: the size of the unusable top partial bucket. Draws below
  // this threshold are rejected so the kept range is an exact multiple of bound.
  uint64_t threshold = (UINT64_MAX - bound + 1) % bound;
  uint64_t r;
  do {
    osp_entropy(&r, sizeof(r));
  } while (r < threshold);
  return (int64_t)(r % bound);
}

#define OSP_INPUT_INIT_CAP ((size_t)128)

// Implements [BUILTIN-INPUT]: read one line from stdin without its trailing
// newline, returning a heap string ("" on EOF/empty). The caller owns the
// result, matching the string-runtime builtins which also malloc their returns.
char *osp_input(void) {
  size_t cap = OSP_INPUT_INIT_CAP;
  size_t len = 0;
  char *buf = (char *)malloc(cap);
  if (buf == NULL) {
    return NULL;
  }
  int c;
  while ((c = getchar()) != EOF && c != '\n') {
    if (len + 1 >= cap) {
      cap *= 2;
      char *grown = (char *)realloc(buf, cap);
      if (grown == NULL) {
        free(buf);
        return NULL;
      }
      buf = grown;
    }
    buf[len] = (char)c;
    len++;
  }
  buf[len] = '\0';
  return buf;
}
