// Cryptographically-secure random + stdin line reader runtime.
// Implements [BUILTIN-RANDOM], [BUILTIN-RANDOM-BELOW], [BUILTIN-INPUT].
//
// Entropy comes straight from the OS CSPRNG — arc4random_buf on macOS/BSD,
// getrandom(2) on Linux (falling back to /dev/urandom), rand_s on Windows,
// getentropy on wasm — so the stream is unpredictable and carries no userspace
// seed/state. That makes it suitable both for security use and for the
// benchmark suite's "randomized" input mode, where a run draws a fresh seed
// each time. The matching "constant" mode never calls these and stays
// byte-for-byte deterministic.
//
// A draw either carries OS entropy or the process stops: there is no degraded
// mode that answers zero or a stale stack word and calls it random.

#ifdef _WIN32
// Must precede <stdlib.h>: msvcrt only DECLARES rand_s when this is defined,
// and without the declaration the call would be an implicit int-returning
// function -- a hard error under -Werror, and the wrong ABI if it were not.
#define _CRT_RAND_S
#endif

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
#elif defined(_WIN32)
// Windows had NO entropy source at all: it fell through to the POSIX
// `/dev/urandom` fallback, which a native Windows binary cannot open, and the
// old code then returned the caller's UNINITIALISED stack word as a
// "cryptographically-secure" draw. `random() >= 0` and `randomBelow(100) < 100`
// both hold for stack garbage, so the corpus passed on it. rand_s is msvcrt's
// wrapper over RtlGenRandom -- the OS CSPRNG, not the seeded `rand()`.
// [BUILTIN-RANDOM]
#define OSP_HAVE_RAND_S 1
#endif

#ifndef OSP_HAVE_ARC4RANDOM
// No entropy source could fill the request. There is nowhere to report it:
// `random()` answers `int` in the language, not `Result<int, Error>`, and the
// WebSocket handshake nonce in http_shared.c draws from this same call.
//
// The two previous answers were both silently wrong. `getentropy` failing
// memset the buffer to ZERO and returned, and a `/dev/urandom` that would not
// open left the caller's word UNWRITTEN -- an uninitialised stack `uint64_t`
// read back as a draw -- while the comment above claimed the tail was zeroed.
// Neither is entropy. A predictable "cryptographically-secure" value is a
// security defect no caller can detect, and a nonce is exactly where it does
// the most damage. Stopping is the only truthful answer [BUILTIN-RANDOM].
static void osp_entropy_exhausted(size_t got, size_t len) {
  fprintf(stderr,
          "FATAL: the OS entropy source gave %zu of the %zu bytes random() "
          "needs; there is no unpredictable value to return\n",
          got, len);
  // abort() does not flush stdio. stderr is unbuffered by default, but an
  // embedder that redirected it -- which is how CI captures a crash -- gets a
  // fully-buffered stream, and the one line explaining the death would die
  // with it.
  (void)fflush(stderr);
  abort();
}
#endif

// Drain the OS entropy source into `buf` — the runtime-wide source of
// unpredictable bytes, also used for the WebSocket handshake nonce in
// http_shared.c so there is exactly one CSPRNG entry point. Fills `len` bytes
// or does not return [BUILTIN-RANDOM].
void osp_random_bytes(void *buf, size_t len) {
#ifdef OSP_HAVE_GETENTROPY
  // getentropy caps a single call at 256 bytes; every caller here asks for 8.
  if (getentropy(buf, len) != 0) {
    osp_entropy_exhausted(0, len);
  }
#elif defined(OSP_HAVE_ARC4RANDOM)
  arc4random_buf(buf, len);
#elif defined(OSP_HAVE_RAND_S)
  // rand_s yields one 32-bit word per call, so a wider request is assembled a
  // word at a time and the last one is truncated to what is left.
  for (size_t off = 0; off < len;) {
    unsigned int word = 0;
    if (rand_s(&word) != 0) {
      osp_entropy_exhausted(off, len);
    }
    size_t take = len - off < sizeof(word) ? len - off : sizeof(word);
    memcpy((unsigned char *)buf + off, &word, take);
    off += take;
  }
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
  // The fallback refills from offset 0 rather than patching the tail: a
  // partial getrandom draw is discarded, so the buffer is whole-cloth from one
  // source instead of a splice of two.
  size_t got = 0;
  FILE *f = fopen("/dev/urandom", "rb");
  if (f != NULL) {
    got = fread(buf, 1, len, f);
    fclose(f);
  }
  if (got < len) {
    osp_entropy_exhausted(got, len);
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
#include <unistd.h>

// One byte of stdin, or EOF.
//
// This BLOCKS until a byte arrives or the descriptor really ends. Elapsed
// silence is NOT end-of-file: a pipe whose writer has not spoken yet is
// connected, and answering "" for it truncates or discards a line the program
// then computes with. An earlier revision timed the first byte out after a
// second to stop a non-interactive launcher hanging; that hang belongs to the
// launcher, and both of ours now close the child's stdin so the read sees a
// real EOF (vscode-extension/client/src/extension.ts, and `Stdio::null` in
// crates/osprey-cli/src/test_cmd.rs) [BUILTIN-INPUT].
//
// Reads the descriptor directly rather than through stdio: `term_runtime.c`
// reads keystrokes the same way, and a buffering layer between the two would
// strand bytes in whichever one read first.
static int osp_input_byte(void) {
  unsigned char ch;
  return read(STDIN_FILENO, &ch, 1) == 1 ? (int)ch : EOF;
}
#else
// Windows and wasm keep the stdio reader: the wasm runtime excludes `input`
// altogether ([WASM-TARGET]).
static int osp_input_byte(void) { return getchar(); }
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
  while ((c = osp_input_byte()) != EOF && c != '\n') {
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
