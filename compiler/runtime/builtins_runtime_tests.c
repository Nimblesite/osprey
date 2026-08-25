// Assertion-driven tests for four small builtin runtimes in one binary:
//   ffi_runtime.c    — pointer cells for C out-parameters (Osprey `Ptr`)
//   random_runtime.c — [BUILTIN-RANDOM], [BUILTIN-RANDOM-BELOW], [BUILTIN-INPUT]
//   term_runtime.c   — [BUILTIN-TERM] raw mode, key decoding, ANSI writes
//   test_runtime.c   — [TESTING-RUNTIME], [TESTING-TAP], [TESTING-EXIT],
//                      [TESTING-FILTER] TAP state machine
// Linked by the Makefile's _test_c_runtime. POSIX harness: scenarios that own
// process-global state (TAP counters, stdin, the terminal) each run in a fork
// with stdin fed from a buffer and stdout captured, then compared BYTE-EXACT.
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

void *osprey_ffi_cell(void);
void *osprey_ffi_deref(void *cell);
int64_t osprey_ffi_free(void *cell);
void *osprey_ffi_null(void);
void *osprey_ffi_transient(void);
int64_t osprey_ffi_is_null(void *ptr);
int64_t osp_random(void);
int64_t osp_random_below(int64_t n);
char *osp_input(void);
int64_t term_raw_mode(int64_t on);
int64_t term_cols(void);
int64_t term_rows(void);
char *term_read_key(void);
int64_t term_clear(void);
int64_t term_hide_cursor(void);
int64_t term_show_cursor(void);
int64_t term_move_cursor(int64_t row, int64_t col);
int32_t osp_test_begin(const char *name);
void osp_test_pass(void);
void osp_test_fail(const char *reason);
void osp_test_skip(const char *reason);
void osp_test_assert(const char *label, int32_t ok, const char *expected,
                     const char *actual);
void osp_test_end(const char *name);
int32_t osp_test_finalize(void);

static long g_checks = 0;
#define CHECK(c)                                                               \
  do {                                                                         \
    g_checks++;                                                                \
    assert(c);                                                                 \
  } while (0)

#define DRAWS 10000
#define BELOW_N 7
#define OUT_CAP ((size_t)8192)
#define LONG_LINE ((size_t)300) // > the 128-byte initial input buffer

// --- fork harness ------------------------------------------------------------

// Run `fn` in a child with stdin fed `input` and stdout captured; assert the
// exit code and (when `want_out` is non-NULL) the exact output bytes.
static void run_child_expect(int (*fn)(void), const char *input,
                             const char *want_out, int want_exit) {
  int in_pipe[2];
  int out_pipe[2];
  CHECK(pipe(in_pipe) == 0 && pipe(out_pipe) == 0);
  pid_t pid = fork();
  CHECK(pid >= 0);
  if (pid == 0) {
    dup2(in_pipe[0], STDIN_FILENO);
    dup2(out_pipe[1], STDOUT_FILENO);
    close(in_pipe[0]);
    close(in_pipe[1]);
    close(out_pipe[0]);
    close(out_pipe[1]);
    int code = fn();
    fflush(stdout);
    _exit(code);
  }
  close(in_pipe[0]);
  close(out_pipe[1]);
  if (input != NULL) {
    size_t len = strlen(input);
    CHECK(write(in_pipe[1], input, len) == (ssize_t)len);
  }
  close(in_pipe[1]);
  static char out[OUT_CAP];
  size_t got = 0;
  ssize_t n;
  while ((n = read(out_pipe[0], out + got, OUT_CAP - 1 - got)) > 0) {
    got += (size_t)n;
  }
  out[got] = '\0';
  close(out_pipe[0]);
  int status = 0;
  CHECK(waitpid(pid, &status, 0) == pid);
  CHECK(WIFEXITED(status) && WEXITSTATUS(status) == want_exit);
  if (want_out != NULL) {
    CHECK(strcmp(out, want_out) == 0);
  }
}

// --- ffi_runtime -------------------------------------------------------------

static void t_ffi_cells(void) {
  void *cell = osprey_ffi_cell();
  CHECK(cell != NULL);
  CHECK(osprey_ffi_deref(cell) == NULL); // the cell starts zeroed
  int target = 7;
  void *addr = &target;
  memcpy(cell, &addr, sizeof addr); // what a C out-parameter call does
  CHECK(osprey_ffi_deref(cell) == &target);
  CHECK(osprey_ffi_free(cell) == 0);
  CHECK(osprey_ffi_free(NULL) == 0);
  CHECK(osprey_ffi_deref(NULL) == NULL);
  CHECK(osprey_ffi_null() == NULL);
  CHECK(osprey_ffi_transient() == (void *)(intptr_t)-1);
  CHECK(osprey_ffi_is_null(NULL) == 1);
  CHECK(osprey_ffi_is_null(&target) == 0);
  CHECK(osprey_ffi_is_null(osprey_ffi_transient()) == 0);
}

// --- random_runtime ----------------------------------------------------------

static void t_random_bounds(void) {
  int64_t first = osp_random();
  int all_same = 1;
  for (int i = 0; i < 64; i++) {
    int64_t v = osp_random();
    CHECK(v >= 0); // sign bit is always cleared
    all_same &= (v == first);
  }
  CHECK(!all_same); // 65 identical CSPRNG draws means the entropy source died
}

static void t_random_below(void) {
  CHECK(osp_random_below(0) == -1);
  CHECK(osp_random_below(-5) == -1);
  CHECK(osp_random_below(INT64_MIN) == -1);
  for (int i = 0; i < 100; i++) {
    CHECK(osp_random_below(1) == 0); // one residue class only
  }
  int seen[BELOW_N] = {0};
  for (int i = 0; i < DRAWS; i++) {
    int64_t v = osp_random_below(BELOW_N);
    CHECK(v >= 0 && v < BELOW_N);
    seen[v] = 1;
  }
  for (int r = 0; r < BELOW_N; r++) {
    CHECK(seen[r] == 1); // every residue reachable (10k draws over 7 classes)
  }
  int64_t big = osp_random_below(INT64_MAX);
  CHECK(big >= 0 && big < INT64_MAX);
}

// Reads three lines: a short one, one longer than the initial buffer (forcing
// the realloc growth path), then EOF (the documented empty string).
static int child_input_lines(void) {
  for (int i = 0; i < 3; i++) {
    char *line = osp_input();
    if (line == NULL) {
      return 2;
    }
    printf("[%s]", line);
    free(line);
  }
  return 0;
}

static void t_input_lines(void) {
  static char input[LONG_LINE + 16];
  static char want[LONG_LINE + 32];
  memset(want, 0, sizeof want);
  strcpy(input, "alpha\n");
  size_t base = strlen(input);
  memset(input + base, 'z', LONG_LINE);
  input[base + LONG_LINE] = '\n';
  input[base + LONG_LINE + 1] = '\0';
  strcpy(want, "[alpha][");
  memset(want + strlen(want), 'z', LONG_LINE);
  strcat(want, "][]"); // EOF yields the empty string, not NULL
  run_child_expect(child_input_lines, input, want, 0);
}

// Long enough that the reader is certainly parked inside `read` before the
// writer speaks: that is the whole point of the delayed-writer cases.
#define INPUT_WRITER_DELAY_US ((useconds_t)400000)

// Point STDIN at `read_fd` (consumed), answering the saved original descriptor.
static int stdin_redirect(int read_fd) {
  int saved = dup(STDIN_FILENO);
  CHECK(saved >= 0);
  CHECK(dup2(read_fd, STDIN_FILENO) >= 0);
  close(read_fd);
  return saved;
}

// Put back the descriptor `stdin_redirect` displaced.
static void stdin_restore(int saved) {
  CHECK(dup2(saved, STDIN_FILENO) >= 0);
  close(saved);
}

// Read one line through `osp_input` with STDIN redirected IN THIS PROCESS, and
// answer it. Unlike `run_child_expect` this keeps the call in the parent, whose
// gcov counters are actually flushed at exit -- a forked child `_exit`s, so the
// coverage of everything it ran is discarded ([BUILTIN-INPUT] was measured at
// zero for exactly that reason). The write end is closed before the read, so
// the descriptor is at end-of-file the moment `feed` runs out.
static char *input_through_pipe(const char *feed) {
  int pipe_fds[2];
  CHECK(pipe(pipe_fds) == 0);
  if (feed != NULL) {
    size_t len = strlen(feed);
    CHECK(write(pipe_fds[1], feed, len) == (ssize_t)len);
  }
  close(pipe_fds[1]);
  int saved = stdin_redirect(pipe_fds[0]);
  char *line = osp_input();
  stdin_restore(saved);
  return line;
}

// Read one line while a FORKED writer stays SILENT for a while and only then
// speaks. Silence is not end-of-file: an open pipe whose producer has not
// spoken yet is connected, and whatever it eventually sends must arrive whole.
// `before` is written immediately (NULL for nothing at all) and `after` once
// the delay has passed (NULL to close in silence instead) [BUILTIN-INPUT].
static char *input_from_delayed_writer(const char *before, const char *after) {
  int pipe_fds[2];
  CHECK(pipe(pipe_fds) == 0);
  pid_t writer = fork();
  CHECK(writer >= 0);
  if (writer == 0) {
    close(pipe_fds[0]);
    int ok = 1;
    if (before != NULL) {
      ok = write(pipe_fds[1], before, strlen(before)) > 0;
    }
    usleep(INPUT_WRITER_DELAY_US);
    if (ok && after != NULL) {
      ok = write(pipe_fds[1], after, strlen(after)) > 0;
    }
    close(pipe_fds[1]);
    _exit(ok ? 0 : 1);
  }
  close(pipe_fds[1]);
  int saved = stdin_redirect(pipe_fds[0]);
  char *line = osp_input();
  stdin_restore(saved);
  int status = 0;
  CHECK(waitpid(writer, &status, 0) == writer);
  CHECK(WIFEXITED(status) && WEXITSTATUS(status) == 0);
  return line;
}

// [BUILTIN-INPUT] end to end, in-process. Seven descriptor states: a short
// line, a line longer than the initial buffer (the realloc growth path), a
// closed-and-empty descriptor, a line whose only terminator is EOF, and three
// delayed-writer cases -- a late FIRST byte, a mid-line pause, and a writer
// that closes in silence.
//
// The delayed cases are the contract that a wall-clock reader breaks. Treating
// elapsed silence as end-of-file answers "" for the late first byte and "he"
// for the mid-line pause: a value the program then computes with. Only a real
// end-of-file ends a line.
static void t_input_descriptor_states(void) {
  char *line = input_through_pipe("alpha\n");
  CHECK(line != NULL && strcmp(line, "alpha") == 0);
  free(line);

  static char big[LONG_LINE + 2];
  memset(big, 'z', LONG_LINE);
  big[LONG_LINE] = '\n';
  big[LONG_LINE + 1] = '\0';
  line = input_through_pipe(big);
  CHECK(line != NULL && strlen(line) == LONG_LINE);
  free(line);

  // EOF: the documented empty string, never NULL.
  line = input_through_pipe(NULL);
  CHECK(line != NULL && line[0] == '\0');
  free(line);

  // End-of-file terminates a line just as a newline would.
  line = input_through_pipe("no-newline");
  CHECK(line != NULL && strcmp(line, "no-newline") == 0);
  free(line);

  // Only the second line's worth is consumed per call: the reader stops at the
  // newline and leaves the rest of the descriptor alone.
  int pipe_fds[2];
  CHECK(pipe(pipe_fds) == 0);
  CHECK(write(pipe_fds[1], "one\ntwo\n", 8) == 8);
  close(pipe_fds[1]);
  int saved = stdin_redirect(pipe_fds[0]);
  char *first = osp_input();
  char *second = osp_input();
  char *third = osp_input();
  stdin_restore(saved);
  CHECK(first != NULL && strcmp(first, "one") == 0);
  CHECK(second != NULL && strcmp(second, "two") == 0);
  CHECK(third != NULL && third[0] == '\0');
  free(first);
  free(second);
  free(third);

  // A producer silent well past any plausible grace, then speaking, is NOT
  // end-of-file: its whole line arrives.
  line = input_from_delayed_writer(NULL, "late\n");
  CHECK(line != NULL && strcmp(line, "late") == 0);
  free(line);

  // The same producer pausing MID-LINE must not truncate it either.
  line = input_from_delayed_writer("he", "llo\n");
  CHECK(line != NULL && strcmp(line, "hello") == 0);
  free(line);

  // A writer that closes in silence IS end-of-file, and answers "".
  line = input_from_delayed_writer(NULL, NULL);
  CHECK(line != NULL && line[0] == '\0');
  free(line);
}

// --- term_runtime ------------------------------------------------------------

// With stdout a pipe (not a tty), size queries and raw mode must fail SOFTLY
// with their documented codes and print nothing.
static int child_term_not_a_tty(void) {
  int ok = term_cols() == -1 && term_rows() == -1 && term_raw_mode(1) == -1 &&
           term_raw_mode(0) == 0;
  return ok ? 0 : 1;
}

// The ANSI writers emit EXACTLY their sequences (clear+home, hide, show,
// move, and the move clamp to 1-based coordinates) and all report success.
static int child_term_ansi(void) {
  int ok = term_clear() == 0 && term_hide_cursor() == 0 &&
           term_show_cursor() == 0 && term_move_cursor(5, 7) == 0 &&
           term_move_cursor(0, -3) == 0;
  return ok ? 0 : 1;
}

// Decodes the whole keymap from fed bytes: control keys, a literal char,
// arrow/nav CSI sequences, tilde sequences, SS3 arrows, a bare Esc at EOF,
// then NULL on exhausted input.
static int child_term_keys(void) {
  for (;;) {
    char *key = term_read_key();
    if (key == NULL) {
      printf("NULL");
      return 0;
    }
    printf("%s;", key);
    free(key);
  }
}

static void t_term(void) {
  run_child_expect(child_term_not_a_tty, "", "", 0);
  run_child_expect(child_term_ansi, "",
                   "\x1b[2J\x1b[H" "\x1b[?25l" "\x1b[?25h" "\x1b[5;7H"
                   "\x1b[1;1H",
                   0);
  run_child_expect(child_term_keys,
                   "\r\n\t\x7f\x08\x03x\x1b[A\x1b[B\x1b[C\x1b[D\x1b[H\x1b[F"
                   "\x1b[3~\x1b[5~\x1b[6~\x1bOA\x1b",
                   "Enter;Enter;Tab;Backspace;Backspace;Ctrl-C;x;Up;Down;"
                   "Right;Left;Home;End;Delete;PageUp;PageDown;Up;Esc;NULL",
                   0);
}

// --- test_runtime (TAP) ------------------------------------------------------

// Mixed run: pass, fail (with diagnostic BEFORE the result line), skip with a
// reason, and a stray assertion outside any case. Exit code must be 1.
static int child_tap_mixed(void) {
  if (osp_test_begin("one")) {
    osp_test_assert(NULL, 1, "x", "x");
    osp_test_pass();
    osp_test_end("one");
  }
  if (osp_test_begin("two")) {
    osp_test_fail("boom");
    osp_test_end("two");
  }
  if (osp_test_begin("three")) {
    osp_test_skip("later");
    osp_test_end("three");
  }
  osp_test_assert("lbl", 0, "1", "2"); // stray: outside any case
  return osp_test_finalize();
}

// All green: finalize reports 0.
static int child_tap_clean(void) {
  if (osp_test_begin("a")) {
    osp_test_assert("eq", 1, "1", "1");
    osp_test_end("a");
  }
  if (osp_test_begin("b")) {
    osp_test_end("b");
  }
  return osp_test_finalize();
}

// OSPREY_TEST_FILTER selects exactly one case; unmatched cases never run and
// never count.
static int child_tap_filter(void) {
  if (setenv("OSPREY_TEST_FILTER", "only", 1) != 0) {
    return 3;
  }
  if (osp_test_begin("other")) {
    osp_test_end("other"); // must be unreachable
  }
  if (osp_test_begin("only")) {
    osp_test_end("only");
  }
  return osp_test_finalize();
}

// A nested test() fails the ENCLOSING case loudly instead of running.
static int child_tap_nested(void) {
  if (osp_test_begin("outer")) {
    if (osp_test_begin("inner")) {
      osp_test_end("inner"); // must be unreachable
    }
    osp_test_end("outer");
  }
  return osp_test_finalize();
}

// A zero-case run still prints the plan, so a filter matching nothing shows.
static int child_tap_empty(void) { return osp_test_finalize(); }

// NULL reasons are tolerated everywhere (fail, skip).
static int child_tap_null_reasons(void) {
  if (osp_test_begin("s")) {
    osp_test_skip(NULL);
    osp_test_end("s");
  }
  if (osp_test_begin("f")) {
    osp_test_fail(NULL);
    osp_test_end("f");
  }
  return osp_test_finalize();
}

static void t_tap(void) {
  run_child_expect(child_tap_mixed, "",
                   "ok 1 - one\n"
                   "# fail: boom\n"
                   "not ok 2 - two\n"
                   "ok 3 - three # SKIP later\n"
                   "# check 'lbl' failed: expected 1, got 2\n"
                   "1..3\n"
                   "# tests=3 passed=1 failed=1 skipped=1\n",
                   1);
  run_child_expect(child_tap_clean, "",
                   "ok 1 - a\nok 2 - b\n1..2\n"
                   "# tests=2 passed=2 failed=0 skipped=0\n",
                   0);
  run_child_expect(child_tap_filter, "",
                   "ok 1 - only\n1..1\n"
                   "# tests=1 passed=1 failed=0 skipped=0\n",
                   0);
  run_child_expect(child_tap_nested, "",
                   "# nested test 'inner' skipped: test() inside a test body "
                   "is not supported\n"
                   "not ok 1 - outer\n1..1\n"
                   "# tests=1 passed=0 failed=1 skipped=0\n",
                   1);
  run_child_expect(child_tap_empty, "",
                   "1..0\n# tests=0 passed=0 failed=0 skipped=0\n", 0);
  // A reasonless skip ends at the bare directive — no trailing space, so the
  // line is byte-comparable against a golden [TESTING-TAP].
  run_child_expect(child_tap_null_reasons, "",
                   "ok 1 - s # SKIP\n"
                   "# fail: \n"
                   "not ok 2 - f\n1..2\n"
                   "# tests=2 passed=0 failed=1 skipped=1\n",
                   1);
}

int main(void) {
  t_ffi_cells();
  t_random_bounds();
  t_random_below();
  t_input_lines();
  t_input_descriptor_states();
  t_term();
  t_tap();
  printf("[ok] builtins_runtime: %ld assertions\n", g_checks);
  return 0;
}
