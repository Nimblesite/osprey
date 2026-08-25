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

// Drain the OS entropy source into `buf` — the runtime-wide source of
// unpredictable bytes, also used for the WebSocket handshake nonce in
// http_shared.c so there is exactly one CSPRNG entry point.
// Drain the OS entropy source into `buf`. Best-effort on the /dev/urandom
// fallback path: a short read leaves the tail zeroed rather than aborting,
// which never happens on the supported platforms.
void osp_random_bytes(void *buf, size_t len) {
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
  osp_random_bytes(&v, sizeof(v));
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
    osp_random_bytes(&r, sizeof(r));
  } while (r < threshold);
  return (int64_t)(r % bound);
}

#define OSP_INPUT_INIT_CAP ((size_t)128)

#if !defined(_WIN32) && !defined(__wasm__)
#include <sys/select.h>
#include <unistd.h>

// How long a NON-INTERACTIVE stdin may stay silent before `input()` concludes
// nothing is coming. [BUILTIN-INPUT] requires the empty string rather than
// blocking when stdin is "empty or not connected", and an fd that is open but
// SILENT -- the stdin pipe an editor's `execFile` opens and never writes, or an
// idle redirect -- is neither EOF nor data, so an ungated read parks on it
// forever. That is not hypothetical: it hung "Compile and Run" with no output
// at all, because stdout is block-buffered and nothing is flushed before the
// read blocks.
//
// A terminal is exempt. It is connected and WILL deliver once the user types,
// which is the entire point of reading a line interactively, so there the wait
// stays unbounded. So is every byte after the first -- see `osp_input_byte`.
//
// KNOWN RESIDUAL: a pipe whose writer is merely SLOW is indistinguishable from
// one whose writer will never speak, so a producer that stays silent past this
// grace before its first byte reads as empty. That is the spec's own trade --
// "empty or not connected" yields "" -- and it is bounded to whole lines: a
// line already begun is never cut short.
#define OSP_INPUT_SILENT_MS 1000

// Whether stdin has a byte to give. Waits forever when `bounded` is 0 or stdin
// is a terminal, otherwise at most [`OSP_INPUT_SILENT_MS`].
static int osp_input_ready(int bounded) {
  if (!bounded || isatty(STDIN_FILENO)) {
    return 1;
  }
  fd_set set;
  FD_ZERO(&set);
  FD_SET(STDIN_FILENO, &set);
  struct timeval tv;
  tv.tv_sec = OSP_INPUT_SILENT_MS / 1000;
  tv.tv_usec = (OSP_INPUT_SILENT_MS % 1000) * 1000;
  return select(STDIN_FILENO + 1, &set, NULL, NULL, &tv) > 0;
}

// One byte of stdin, or EOF. `bounded` gates ONLY the first byte of a line: a
// producer that writes "he", pauses past the grace, then writes "llo\n" must
// still deliver `hello`. Truncating a line mid-way hands the program a value it
// then computes with, which is worse than the unbounded wait the grace exists
// to avoid -- so once a line has started, this blocks until its newline or a
// real EOF.
//
// Reads the descriptor directly rather than through stdio: `select` reports
// what the DESCRIPTOR holds, so a buffering layer between the two would report
// "nothing to read" with a line already sitting in its buffer.
// `term_runtime.c` reads keystrokes the same way.
static int osp_input_byte(int bounded) {
  if (!osp_input_ready(bounded)) {
    return EOF;
  }
  unsigned char ch;
  return read(STDIN_FILENO, &ch, 1) == 1 ? (int)ch : EOF;
}
#else
// Windows and wasm keep the stdio reader: neither can `select` a stdin handle,
// and the wasm runtime excludes `input` altogether ([WASM-TARGET]).
static int osp_input_byte(int bounded) {
  (void)bounded;
  return getchar();
}
#endif

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
  // Anything already staged for stdout must reach the user BEFORE this
  // parks: stdout is block-buffered off a terminal, so an unflushed prompt
  // is invisible for exactly as long as the read waits.
  fflush(stdout);
  int c;
  while ((c = osp_input_byte(len == 0)) != EOF && c != '\n') {
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
