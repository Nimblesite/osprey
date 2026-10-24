// Permutation tests for the mock's one stateful model and its neighbours
// [SYMEX-MOCK-STREAM] [SYMEX-MOCK-STATELESS]: every stream cap against every
// symbolic length, every read shape (byte, chunk, line, block), seeks from
// every origin, fresh streams per open, the open-failure branch, the slot
// limit, modes, the standard descriptors, and the entropy and clock names.
#include "hunt_mock_tests.h"

#include <string.h>

enum { EBADF_VALUE = 9, ENOENT_VALUE = 2, EINVAL_VALUE = 22, ENFILE_VALUE = 23 };
enum { SEEK_SET_VALUE = 0, SEEK_CUR_VALUE = 1, SEEK_END_VALUE = 2 };

static char message[256];

static const char *about(const char *what, long a, long b) {
  (void)snprintf(message, sizeof(message), "%s (%ld, %ld)", what, a, b);
  return message;
}

static long clamp_cap(long cap) { return cap < 0 ? 0 : cap > OSP_HUNT_STREAM_MAX ? OSP_HUNT_STREAM_MAX : cap; }

static int pattern_at(long i) { return (int)(unsigned char)(i * 7 + 3); }

// The bytes the next stream makes symbolic: a pattern that differs at every
// position modulo 256.
static void queue_pattern(long count) {
  static char bytes[OSP_HUNT_STREAM_MAX];
  for (long i = 0; i < count && i < OSP_HUNT_STREAM_MAX; i++) {
    bytes[i] = (char)(i * 7 + 3);
  }
  hunt_queue_bytes(bytes, (size_t)clamp_cap(count));
}

static int matches_pattern(const unsigned char *bytes, long count, long from) {
  for (long i = 0; i < count; i++) {
    if (bytes[i] != pattern_at(from + i)) {
      return 0;
    }
  }
  return 1;
}

// ---- stdin: every cap against every length ------------------------------------

static long drain_stdin(long cap, long length) {
  hunt_reset();
  osp_hunt_input_bytes = cap;
  queue_pattern(clamp_cap(cap));
  hunt_queue_range((int)length);
  long count = 0;
  int ordered = 1;
  for (int byte = getchar(); byte != -1; byte = getchar()) {
    ordered = ordered && byte == pattern_at(count);
    count++;
  }
  hunt_check(ordered, about("stdin yields the symbolic bytes in order", cap, length));
  return count;
}

static void stdin_reads_exactly_its_length_at_every_cap(void) {
  static const long caps[] = {-5, 0, 1, 7, 64, OSP_HUNT_STREAM_MAX, OSP_HUNT_STREAM_MAX + 100};
  for (size_t c = 0; c < sizeof(caps) / sizeof(caps[0]); c++) {
    long cap = clamp_cap(caps[c]);
    long lengths[] = {0, 1, cap / 2, cap, cap + 5};
    for (size_t l = 0; l < sizeof(lengths) / sizeof(lengths[0]); l++) {
      long expected = lengths[l] > cap ? cap : lengths[l];
      hunt_check(drain_stdin(caps[c], lengths[l]) == expected, about("stdin length is min(length, cap)", caps[c], lengths[l]));
      hunt_check(getchar() == -1 && feof(osp_hunt_stdin), about("EOF stays EOF", caps[c], lengths[l]));
      int named = cap == 0 ? hunt_range_count() == 0 : hunt_range_count() == 1 && strcmp(hunt_range_name(0), "stdin.len") == 0 && strcmp(hunt_last_symbolic(), "stdin") == 0;
      hunt_check(named, about("stdin is the object stdin with length stdin.len", caps[c], lengths[l]));
    }
  }
}

// ---- stdin through read(0): every chunk size ------------------------------------

static void read_returns_the_stream_in_any_chunk_size(void) {
  static const size_t chunks[] = {1, 2, 3, 5, 37, 64, 4096};
  for (size_t c = 0; c < sizeof(chunks) / sizeof(chunks[0]); c++) {
    hunt_reset();
    osp_hunt_input_bytes = 64;
    queue_pattern(64);
    hunt_queue_range(37);
    static unsigned char got[OSP_HUNT_STREAM_MAX];
    long total = 0;
    for (long n = read(0, got + total, chunks[c]); n > 0; n = read(0, got + total, chunks[c])) {
      total += n;
    }
    hunt_check(total == 37 && matches_pattern(got, total, 0), about("read(0) returns every byte once", (long)chunks[c], total));
    hunt_check(read(0, got, 8) == 0 && read(0, got, 0) == 0, "read at the end returns 0");
  }
  hunt_reset();
  unsigned char byte;
  hunt_check(read(1, &byte, 1) == -1 && osp_hunt_errno == EBADF_VALUE, "read from a descriptor other than 0 is EBADF");
  hunt_check(read(7, &byte, 1) == -1 && osp_hunt_errno == EBADF_VALUE, "read from an unknown descriptor is EBADF");
}

// ---- stdin through fgets: every buffer size --------------------------------------

typedef struct {
  int cap;
  const char *first;
  const char *second;
} fgets_case;

static void fgets_reads_lines_at_every_buffer_size(void) {
  static const fgets_case cases[] = {
      {2, "a", "b\n"}, {3, "ab", "\n"}, {4, "ab\n", "cd"}, {5, "ab\n", "cd"}, {8, "ab\n", "cd"},
  };
  for (size_t i = 0; i < sizeof(cases) / sizeof(cases[0]); i++) {
    hunt_reset();
    osp_hunt_input_bytes = 5;
    hunt_queue_bytes("ab\ncd", 5);
    hunt_queue_range(5);
    char line[16];
    int first = fgets(line, cases[i].cap, osp_hunt_stdin) != NULL && strcmp(line, cases[i].first) == 0;
    int second = fgets(line, 8, osp_hunt_stdin) != NULL && strcmp(line, cases[i].second) == 0;
    hunt_check(first && second, about("fgets honours its buffer size, then continues", cases[i].cap, 0));
  }
  hunt_reset();
  osp_hunt_input_bytes = 4;
  hunt_queue_bytes("tail", 4);
  hunt_queue_range(4);
  char line[16];
  hunt_check(fgets(line, 16, osp_hunt_stdin) != NULL && strcmp(line, "tail") == 0, "a last line without a newline is returned");
  hunt_check(fgets(line, 16, osp_hunt_stdin) == NULL, "fgets at EOF returns NULL");
}

// ---- files: every cap against every length, every seek ------------------------------

static FILE_ *open_symbolic(const char *path, long cap, long length) {
  osp_hunt_input_bytes = cap;
  hunt_queue_range(1);
  queue_pattern(cap);
  hunt_queue_range((int)length);
  return fopen(path, "r");
}

static void seeks_stay_inside_the_length(FILE_ *file, long cap, long length) {
  hunt_check(fseeko(file, 0, SEEK_END_VALUE) == 0 && ftello(file) == length, about("SEEK_END is the symbolic length", cap, length));
  hunt_check(fseeko(file, 1, SEEK_END_VALUE) == -1 && osp_hunt_errno == EINVAL_VALUE, about("past the end is EINVAL", cap, length));
  hunt_check(fseeko(file, -1, SEEK_SET_VALUE) == -1, about("before the start fails", cap, length));
  hunt_check(fseeko(file, length, SEEK_SET_VALUE) == 0 && fgetc(file) == -1 && feof(file), about("at the end reads EOF", cap, length));
  int back = length == 0 || (fseeko(file, -1, SEEK_END_VALUE) == 0 && fgetc(file) == pattern_at(length - 1));
  hunt_check(back, about("SEEK_END - 1 is the last byte", cap, length));
  int forward = fseeko(file, 0, SEEK_SET_VALUE) == 0 && fseeko(file, length, SEEK_CUR_VALUE) == 0 && ftello(file) == length;
  hunt_check(forward && fseeko(file, 1, SEEK_CUR_VALUE) == -1, about("SEEK_CUR moves from the cursor", cap, length));
}

static void read_one_file(long cap, long length) {
  hunt_reset();
  FILE_ *file = open_symbolic("in.dat", cap, length);
  unsigned char buffer[64];
  size_t got = file == NULL ? 0 : fread(buffer, 1, sizeof(buffer), file);
  hunt_check(file != NULL && (long)got == length && matches_pattern(buffer, length, 0) && feof(file), about("fread returns the symbolic bytes", cap, length));
  if (file == NULL) {
    return;
  }
  seeks_stay_inside_the_length(file, cap, length);
  rewind(file);
  hunt_check(ftello(file) == 0 && !feof(file) && fread(buffer, 1, sizeof(buffer), file) == got, about("rewind rereads the same stream", cap, length));
  int named = strcmp(hunt_range_name(0), "in.dat.ok") == 0 && (cap == 0 || (strcmp(hunt_range_name(1), "in.dat.len") == 0 && strcmp(hunt_last_symbolic(), "in.dat") == 0));
  hunt_check(named, about("a file is <path> with <path>.ok and <path>.len", cap, length));
  (void)fclose(file);
}

static void files_read_and_seek_at_every_cap_and_length(void) {
  static const long caps[] = {0, 1, 5, 16};
  for (size_t c = 0; c < sizeof(caps) / sizeof(caps[0]); c++) {
    for (long length = 0; length <= caps[c]; length++) {
      read_one_file(caps[c], length);
    }
  }
}

// Files keep no state between opens: the same path reopened is a fresh
// symbolic stream with a fresh open choice.
static void every_open_is_a_fresh_stream(void) {
  hunt_reset();
  unsigned char buffer[8];
  FILE_ *first = open_symbolic("a.txt", 8, 6);
  hunt_check(first != NULL && fread(buffer, 1, 8, first) == 6, "the first open reads its own length");
  (void)fclose(first);
  hunt_queue_range(1);
  hunt_queue_bytes("ZZZZZZZZ", 8);
  hunt_queue_range(2);
  FILE_ *second = fopen("a.txt", "r");
  hunt_check(second != NULL && fread(buffer, 1, 8, second) == 2 && buffer[0] == 'Z', "a reopened path reads fresh symbolic bytes");
  int names = strcmp(hunt_range_name(0), "a.txt.ok") == 0 && strcmp(hunt_range_name(1), "a.txt.len") == 0 && strcmp(hunt_range_name(2), "a.txt.ok") == 0 && strcmp(hunt_range_name(3), "a.txt.len") == 0;
  hunt_check(names && hunt_range_count() == 4, "each open draws its own .ok and .len");
  (void)fclose(second);
}

// ---- opens: failure, the slot limit and modes ---------------------------------------

static void opens_fail_symbolically_and_slots_run_out(void) {
  hunt_reset();
  hunt_queue_range(0);
  hunt_check(fopen("gone.txt", "r") == NULL && osp_hunt_errno == ENOENT_VALUE, "a zero .ok fails the open with ENOENT");
  FILE_ *open_files[OSP_HUNT_FILE_SLOTS];
  for (int i = 0; i < OSP_HUNT_FILE_SLOTS; i++) {
    hunt_queue_range(1);
    open_files[i] = fopen("slot.txt", "r");
    hunt_check(open_files[i] != NULL, about("a failed open does not use a slot", i, OSP_HUNT_FILE_SLOTS));
  }
  size_t drawn = hunt_range_count();
  hunt_check(fopen("one-too-many.txt", "r") == NULL && osp_hunt_errno == ENFILE_VALUE, "a ninth open is ENFILE");
  hunt_check(hunt_range_count() == drawn, "a refused open draws no .ok");
  (void)fclose(open_files[3]);
  hunt_queue_range(1);
  hunt_check(fopen("again.txt", "r") != NULL, "closing frees its slot");
  for (int i = 0; i < OSP_HUNT_FILE_SLOTS; i++) {
    (void)fclose(open_files[i]);
  }
}

typedef struct {
  const char *mode;
  int readable;
  int writable;
} mode_case;

static void every_mode_reads_and_writes_as_it_says(void) {
  static const mode_case modes[] = {
      {"r", 1, 0}, {"rb", 1, 0}, {"r+", 1, 1}, {"rb+", 1, 1}, {"w", 0, 1}, {"wb", 0, 1}, {"a", 0, 1},
  };
  for (size_t i = 0; i < sizeof(modes) / sizeof(modes[0]); i++) {
    hunt_reset();
    hunt_capture_start();
    osp_hunt_input_bytes = 4;
    hunt_queue_range(1);
    queue_pattern(4);
    hunt_queue_range(4);
    FILE_ *file = fopen("mode.txt", modes[i].mode);
    unsigned char buffer[4];
    int reads = file != NULL && fread(buffer, 1, 4, file) == (size_t)(modes[i].readable ? 4 : 0);
    int read_error = modes[i].readable ? !ferror(file) : ferror(file) && osp_hunt_errno == EBADF_VALUE;
    clearerr(file);
    int writes = fwrite("xyz", 1, 3, file) == (size_t)(modes[i].writable ? 3 : 0);
    int write_error = modes[i].writable ? !ferror(file) : ferror(file) && osp_hunt_errno == EBADF_VALUE;
    hunt_check(reads && read_error && writes && write_error, modes[i].mode);
    (void)fclose(file);
  }
}

// ---- the standard descriptors ------------------------------------------------------

static void standard_descriptors_go_to_the_sink(void) {
  hunt_reset();
  hunt_capture_start();
  hunt_check(write(1, "out", 3) == 3 && write(2, "err", 3) == 3, "stdout and stderr accept writes");
  hunt_check(puts("line") == 0, "puts succeeds");
  hunt_check(strcmp(hunt_captured(), "outerrline\n") == 0, hunt_captured());
  hunt_check(write(0, "x", 1) == -1 && osp_hunt_errno == EBADF_VALUE, "stdin is not writable");
  hunt_check(write(9, "x", 1) == -1 && osp_hunt_errno == EBADF_VALUE, "an unknown descriptor is EBADF");
  unsigned char byte;
  hunt_check(fread(&byte, 1, 1, osp_hunt_stdout) == 0 && ferror(osp_hunt_stdout), "stdout is not readable");
}

// ---- entropy and the clock: one fresh named object per draw -------------------------

struct hunt_timespec {
  long tv_sec;
  long tv_nsec;
};
int clock_gettime(int id, struct hunt_timespec *out);

static unsigned draw_index(const char *name, const char *base) {
  size_t prefix = strlen(base);
  unsigned value = 0;
  for (const char *at = name + prefix; *at >= '0' && *at <= '9'; at++) {
    value = value * 10 + (unsigned)(*at - '0');
  }
  return value;
}

static void every_draw_is_a_fresh_numbered_object(void) {
  hunt_reset();
  unsigned char bytes[2];
  arc4random_buf(bytes, 2);
  unsigned first = draw_index(hunt_last_symbolic(), "entropy.");
  arc4random_buf(bytes, 2);
  hunt_check(strncmp(hunt_last_symbolic(), "entropy.", 8) == 0 && draw_index(hunt_last_symbolic(), "entropy.") == first + 1, "entropy draws are entropy.<n>, n counting up");
  char before[OSP_HUNT_NAME_MAX];
  strcpy(before, hunt_last_symbolic());
  arc4random_buf(bytes, 0);
  hunt_check(strcmp(hunt_last_symbolic(), before) == 0, "an empty draw makes nothing symbolic");
  long long nanoseconds = 3000000123LL;
  hunt_queue_bytes((const char *)&nanoseconds, sizeof(nanoseconds));
  struct hunt_timespec now;
  hunt_check(clock_gettime(0, &now) == 0 && now.tv_sec == 3 && now.tv_nsec == 123, "the clock splits its symbolic nanoseconds");
  hunt_check(strncmp(hunt_last_symbolic(), "clock.", 6) == 0, hunt_last_symbolic());
}

// ---- object names ----------------------------------------------------------------------

static void names_carry_their_suffix_and_index(void) {
  char name[OSP_HUNT_NAME_MAX];
  static const unsigned indices[] = {0, 7, 10, 99, 4294967295u};
  static const char *const expected[] = {"entropy.0", "entropy.7", "entropy.10", "entropy.99", "entropy.4294967295"};
  for (size_t i = 0; i < sizeof(indices) / sizeof(indices[0]); i++) {
    osp_hunt_name_indexed(name, "entropy", indices[i]);
    hunt_check(strcmp(name, expected[i]) == 0, name);
  }
  osp_hunt_name(name, "dir/data.csv", ".len");
  hunt_check(strcmp(name, "dir/data.csv.len") == 0, name);
  osp_hunt_name(name, "", ".ok");
  hunt_check(strcmp(name, ".ok") == 0, name);
  char long_path[300];
  memset(long_path, 'p', sizeof(long_path) - 1);
  long_path[sizeof(long_path) - 1] = '\0';
  osp_hunt_name(name, long_path, ".len");
  hunt_check(strlen(name) == OSP_HUNT_NAME_MAX - 1, "a long name is cut to the name limit and terminated");
}

void hunt_mock_stream_tests(void) {
  stdin_reads_exactly_its_length_at_every_cap();
  read_returns_the_stream_in_any_chunk_size();
  fgets_reads_lines_at_every_buffer_size();
  files_read_and_seek_at_every_cap_and_length();
  every_open_is_a_fresh_stream();
  opens_fail_symbolically_and_slots_run_out();
  every_mode_reads_and_writes_as_it_says();
  standard_descriptors_go_to_the_sink();
  every_draw_is_a_fresh_numbered_object();
  names_carry_their_suffix_and_index();
}
