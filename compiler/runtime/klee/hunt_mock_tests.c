// Native unit tests for the mock C library [SYMEX-MOCK-STREAM]
// [SYMEX-MOCK-STATELESS] [SYMEX-MOCK-UNSUPPORTED]. The doubles are compiled
// against their own headers and linked here beside the host libc; this file
// is compiled against the host headers and reports through `dprintf`, a
// host function the mock does not define (its own `write` goes to the sink).
// A shim stands in for KLEE: symbolic
// bytes come from a queue the test fills, so each test is one concrete path.
//
// The runtime units under test (`osp_input`, `readFile`) are linked too, so
// the suite proves the runtime reaches the double, not only that the double
// works [SYMEX-MOCK-CLOSED].
#include "hunt_mock_tests.h"

#include <setjmp.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

// ---- host-side reporting -------------------------------------------------

int dprintf(int fd, const char *format, ...);

static int failures;
static int checks;

static void say(const char *text) { (void)dprintf(2, "%s", text); }

void hunt_check(int condition, const char *what) {
  checks++;
  if (!condition) {
    failures++;
    say("FAIL: ");
    say(what);
    say("\n");
  }
}

// ---- KLEE shim --------------------------------------------------------------

enum { RANGE_MAX = 64 };

static unsigned char queue[OSP_HUNT_STREAM_MAX];
static size_t queue_head;
static size_t queue_tail;
static int range_values[RANGE_MAX];
static size_t range_head;
static size_t range_tail;
static char range_names[RANGE_MAX][OSP_HUNT_NAME_MAX];
static size_t range_calls;
static char last_symbolic[OSP_HUNT_NAME_MAX];
static char last_error[OSP_HUNT_NAME_MAX];
static jmp_buf on_error;

void hunt_queue_bytes(const char *bytes, size_t count) {
  size_t room = sizeof(queue) - queue_tail;
  size_t taken = count < room ? count : room;
  memcpy(queue + queue_tail, bytes, taken);
  queue_tail += taken;
}

void hunt_queue_range(int value) {
  if (range_tail < RANGE_MAX) {
    range_values[range_tail++] = value;
  }
}

const char *hunt_range_name(size_t at) { return at < range_calls && at < RANGE_MAX ? range_names[at] : ""; }

size_t hunt_range_count(void) { return range_calls; }

const char *hunt_last_symbolic(void) { return last_symbolic; }

void klee_make_symbolic(void *address, size_t count, const char *name) {
  strncpy(last_symbolic, name, sizeof(last_symbolic) - 1);
  for (size_t i = 0; i < count; i++) {
    ((unsigned char *)address)[i] = queue_head < queue_tail ? queue[queue_head++] : 0;
  }
}

int klee_range(int begin, int end, const char *name) {
  if (range_calls < RANGE_MAX) {
    strncpy(range_names[range_calls], name, OSP_HUNT_NAME_MAX - 1);
  }
  range_calls++;
  int value = range_head < range_tail ? range_values[range_head++] : begin;
  return value < begin ? begin : value >= end ? end - 1 : value;
}

void klee_assume(uintptr_t condition) { (void)condition; }

void klee_report_error(const char *file, int line, const char *message, const char *suffix) {
  (void)file;
  (void)line;
  (void)suffix;
  strncpy(last_error, message, sizeof(last_error) - 1);
  longjmp(on_error, 1);
}

void __assert_fail(const char *expression, const char *file, unsigned line, const char *function) {
  (void)expression;
  (void)file;
  (void)line;
  (void)function;
  longjmp(on_error, 2);
}

void hunt_reset(void) {
  queue_head = queue_tail = 0;
  range_head = range_tail = 0;
  range_calls = 0;
  memset(range_names, 0, sizeof(range_names));
  last_symbolic[0] = '\0';
  last_error[0] = '\0';
  osp_hunt_reset();
}

// The runtime units that must reach the doubles.
char *osp_input(void);

// ---- captured output -------------------------------------------------------

static char captured[4096];
static size_t captured_length;

static void capture(const char *data, size_t count) {
  if (captured_length + count < sizeof(captured)) {
    memcpy(captured + captured_length, data, count);
    captured_length += count;
    captured[captured_length] = '\0';
  }
}

void hunt_capture_start(void) {
  captured_length = 0;
  captured[0] = '\0';
  osp_hunt_sink = capture;
}

const char *hunt_captured(void) { return captured; }

// ---- tests -----------------------------------------------------------------

static void stdin_stream_feeds_the_runtime_line_reader(void) {
  hunt_reset();
  osp_hunt_input_bytes = 16;
  hunt_queue_bytes("hello\nrest", 10);
  hunt_queue_range(10);
  char *line = osp_input();
  hunt_check(line != NULL && strcmp(line, "hello") == 0, "osp_input returns the first line");
  hunt_check(osp_hunt_calls[OSP_HUNT_CALL_READ] == 6, "osp_input read one byte per call through the double");
  hunt_check(strcmp(hunt_last_symbolic(), "stdin") == 0, "the stdin bytes are the symbolic object named stdin");
  char *second = osp_input();
  hunt_check(second != NULL && strcmp(second, "rest") == 0, "the cursor continues after the newline");
  hunt_check(getchar() == -1, "end of the symbolic length is EOF");
}

static void stdin_stream_honours_a_short_symbolic_length(void) {
  hunt_reset();
  osp_hunt_input_bytes = 8;
  hunt_queue_bytes("abcdefgh", 8);
  hunt_queue_range(3);
  char *line = osp_input();
  hunt_check(line != NULL && strcmp(line, "abc") == 0, "stdin.len cuts the stream short");
}

static void read_file_opens_a_symbolic_stream_named_by_path(void) {
  hunt_reset();
  osp_hunt_input_bytes = 8;
  hunt_queue_range(1);
  hunt_queue_bytes("payload!", 8);
  hunt_queue_range(7);
  char path[] = "data.txt";
  char *text = read_file(path);
  hunt_check(text != NULL && strcmp(text, "payload") == 0, "readFile returns the symbolic bytes up to the length");
  hunt_check(strcmp(hunt_last_symbolic(), "data.txt") == 0, "the file bytes are named by their path");
  hunt_check(osp_hunt_calls[OSP_HUNT_CALL_FOPEN] == 1 && osp_hunt_calls[OSP_HUNT_CALL_FREAD] >= 1, "readFile went through fopen and fread");
  hunt_check(osp_hunt_calls[OSP_HUNT_CALL_FCLOSE] == 1, "readFile closed the stream");
}

static void fopen_explores_the_failure_branch(void) {
  hunt_reset();
  hunt_queue_range(0);
  FILE_ *stream = fopen("missing.txt", "r");
  hunt_check(stream == NULL, "a zero on <path>.ok is an open failure");
  hunt_check(osp_hunt_errno == 2, "the failure sets ENOENT");
}

static void fseek_and_fgets_walk_the_stream(void) {
  hunt_reset();
  osp_hunt_input_bytes = 12;
  hunt_queue_range(1);
  hunt_queue_bytes("one\ntwo\n3333", 12);
  hunt_queue_range(12);
  FILE_ *stream = fopen("lines", "r");
  char line[8];
  hunt_check(fgets(line, sizeof(line), stream) != NULL && strcmp(line, "one\n") == 0, "fgets stops at the newline");
  hunt_check(fseeko(stream, 0, 2) == 0 && ftello(stream) == 12, "SEEK_END lands on the symbolic length");
  hunt_check(fseeko(stream, 4, 0) == 0 && fgets(line, sizeof(line), stream) != NULL && strcmp(line, "two\n") == 0, "SEEK_SET then fgets reads the second line");
  hunt_check(fseeko(stream, 99, 0) == -1, "seeking past the length fails");
  fclose(stream);
}

static void written_bytes_reach_the_sink(void) {
  hunt_reset();
  hunt_capture_start();
  printf("n=%d s=%s c=%c", 42, "ok", 'x');
  puts("!");
  fprintf(osp_hunt_stdout, "%5.2f|%-4d|%03d|%x|%lld", 3.14159, 7, 5, 255, -9000000000LL);
  hunt_check(strcmp(captured, "n=42 s=ok c=x!\n 3.14|7   |005|ff|-9000000000") == 0, captured);
  hunt_check(osp_hunt_calls[OSP_HUNT_CALL_PRINTF] == 3, "each print went through the double");
}

static void snprintf_formats_like_the_host(void) {
  char out[64];
  hunt_check(snprintf(out, sizeof(out), "%.16e", 1234.5) == 22 && strcmp(out, "1.2345000000000000e+03") == 0, out);
  hunt_check(snprintf(out, sizeof(out), "%.17g", 0.1) == 19 && strcmp(out, "0.10000000000000001") == 0, out);
  hunt_check(snprintf(out, sizeof(out), "%g", 100000.0) == 6 && strcmp(out, "100000") == 0, out);
  hunt_check(snprintf(out, sizeof(out), "%g", 1000000.0) == 5 && strcmp(out, "1e+06") == 0, out);
  hunt_check(snprintf(out, sizeof(out), "%g", 0.0001) == 6 && strcmp(out, "0.0001") == 0, out);
  hunt_check(snprintf(out, sizeof(out), "%.3f", -2.0005) == 6 && strcmp(out, "-2.001") == 0, out);
  hunt_check(snprintf(out, sizeof(out), "%s", "truncated") == 9 && snprintf(out, 4, "%s", "truncated") == 9 && strcmp(out, "tru") == 0, "snprintf reports the full length and clips");
  hunt_check(snprintf(out, sizeof(out), "%zu %ld %u %%", (size_t)5, -3L, 9u) == 8 && strcmp(out, "5 -3 9 %") == 0, out);
  hunt_check(snprintf(out, sizeof(out), "%f", 1.0 / 0.0) == 3 && strcmp(out, "inf") == 0, out);
}

static void string_and_number_doubles_match_libc(void) {
  char *end;
  hunt_check(strtod("  -12.5e2xyz", &end) == -1250.0 && strcmp(end, "xyz") == 0, "strtod parses sign, fraction and exponent");
  hunt_check(strtod("nope", &end) == 0.0 && strcmp(end, "nope") == 0, "strtod leaves end on no digits");
  hunt_check(strcmp(strstr("haystack", "st"), "stack") == 0 && strstr("hay", "z") == NULL, "strstr finds and misses");
  hunt_check(strcspn("key=value", "=") == 3, "strcspn counts to the first reject");
  hunt_check(getenv("HOME") == NULL && osp_hunt_calls[OSP_HUNT_CALL_GETENV] == 1, "the environment is empty");
}

static int by_value(const void *a, const void *b) { return *(const int *)a - *(const int *)b; }

static void qsort_sorts(void) {
  int values[] = {5, -1, 3, 3, 0, 9};
  qsort(values, 6, sizeof(int), by_value);
  int sorted = values[0] == -1 && values[1] == 0 && values[2] == 3 && values[3] == 3 && values[4] == 5 && values[5] == 9;
  hunt_check(sorted && osp_hunt_calls[OSP_HUNT_CALL_QSORT] == 1, "qsort orders the elements");
}

static void entropy_is_a_named_symbolic_object(void) {
  hunt_reset();
  hunt_queue_bytes("\x01\x02\x03\x04", 4);
  unsigned char bytes[4];
  arc4random_buf(bytes, 4);
  hunt_check(bytes[0] == 1 && bytes[3] == 4, "entropy comes from the symbolic draw");
  hunt_check(strncmp(hunt_last_symbolic(), "entropy.", 8) == 0, last_symbolic);
  hunt_check(osp_hunt_calls[OSP_HUNT_CALL_ENTROPY] == 1, "the entropy double counted the draw");
}

static int close_to(double a, double b) { return a - b < 1e-9 && b - a < 1e-9; }

static void math_doubles_are_accurate(void) {
  hunt_check(close_to(sqrt(2.0), 1.4142135623730951), "sqrt");
  hunt_check(close_to(exp(1.0), 2.718281828459045), "exp");
  hunt_check(close_to(log(10.0), 2.302585092994046), "log");
  hunt_check(close_to(tanh(0.5), 0.46211715726000974), "tanh");
  hunt_check(close_to(pow(2.0, 10.0), 1024.0) && close_to(pow(2.0, 0.5), 1.4142135623730951), "pow");
  hunt_check(floor(-1.5) == -2.0 && close_to(fmod(7.5, 2.0), 1.5), "floor and fmod");
  hunt_check(sqrt(-1.0) != sqrt(-1.0), "sqrt of a negative is nan");
}

static void unsupported_calls_stop_the_path(void) {
  hunt_reset();
  if (setjmp(on_error) == 0) {
    (void)fork();
    hunt_check(0, "fork must not return");
  }
  hunt_check(strcmp(last_error, "unmodelled: fork") == 0, last_error);
  hunt_check(osp_hunt_calls[OSP_HUNT_CALL_UNSUPPORTED] == 1, "the stub counted the call");
}

int main(void) {
  stdin_stream_feeds_the_runtime_line_reader();
  stdin_stream_honours_a_short_symbolic_length();
  read_file_opens_a_symbolic_stream_named_by_path();
  fopen_explores_the_failure_branch();
  fseek_and_fgets_walk_the_stream();
  written_bytes_reach_the_sink();
  snprintf_formats_like_the_host();
  string_and_number_doubles_match_libc();
  qsort_sorts();
  entropy_is_a_named_symbolic_object();
  math_doubles_are_accurate();
  unsupported_calls_stop_the_path();
  hunt_mock_format_tests();
  hunt_mock_stream_tests();
  char summary[64];
  int length = snprintf(summary, sizeof(summary), "hunt_mock_tests: %d checks, %d failures\n", checks, failures);
  (void)length;
  say(summary);
  return failures == 0 ? 0 : 1;
}
