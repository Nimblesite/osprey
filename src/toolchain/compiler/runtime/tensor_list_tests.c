// Assertion-driven tests for tensor_list.c, `loadTensor` [BUILTIN-FILE-TENSOR]:
// the host loader must satisfy the same safetensors contract as
// `gpuLoadTensor` (tensor_file_tests.h) while building a List<float> and
// linking no GPU unit at all — this suite links tensor_list.c, tensor_file.c,
// list_runtime.c, file_runtime.c (the io_error channel) and memory_runtime.c,
// and nothing named gpu_*.
#include <assert.h>
#include <signal.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#include "collection_runtime.h"
#include "float_spelling.h"
#include "memory_hooks.h"
#include "test_death.h"

OspreyList *osprey_load_tensor(const char *path, const char *name);
OspreyList *osprey_read_file_bytes(const char *path, int64_t offset, int64_t count);
int64_t osprey_file_size(const char *path);
OspreyList *osprey_list_dir(const char *path);
int64_t osprey_make_dir(const char *path);
char *osprey_kind_of(const char *path);
#include "osprey_path.h"
void *osprey_tensor_panic(const char *message);
const char *osp_io_error(void);
void osp_io_error_clear(void);

static long g_checks = 0;
#define CHECK(c)                                                               \
  do {                                                                         \
    g_checks++;                                                                \
    assert(c);                                                                 \
  } while (0)

#include "tensor_file_tests.h"

// The host list widens every dtype to float [BUILTIN-FILE-TENSOR], so the
// asked-for element width is not part of its contract and is ignored here.
static void *load(const char *path, const char *name, int64_t elem) {
  (void)elem;
  return osprey_load_tensor(path, name);
}

static int64_t length(void *list) {
  return osprey_list_length(list);
}

// The double a List<float> element word holds.
static double at(void *list, int64_t i) {
  CHECK(osprey_list_in_bounds(list, i));
  int64_t word = osprey_list_get(list, i);
  double d;
  memcpy(&d, &word, sizeof d);
  return d;
}

// [GPU-BUFFER-TENSOR] Exercise the shared reader through real safetensors IO.
#define HALF_WORDS 65536
static const char *half_fixture(void) {
  const char header[] = "{\"half\":{\"dtype\":\"F16\",\"shape\":[65536],\"data_offsets\":[0,131072]}}";
  size_t prefix = 8 + strlen(header), count = prefix + 2 * HALF_WORDS;
  unsigned char *image = malloc(count);
  CHECK(image != NULL);
  put_u64(image, (uint64_t)strlen(header));
  memcpy(image + 8, header, strlen(header));
  for (uint32_t bits = 0; bits < HALF_WORDS; bits++) put_u16(image + prefix + 2 * bits, (uint16_t)bits);
  const char *path = temp_file(image, count);
  free(image);
  return path;
}

static void expect_same_float(double actual, double expected) {
  if (isnan(expected)) CHECK(isnan(actual) && signbit(actual) == signbit(expected));
  else CHECK(memcmp(&actual, &expected, sizeof actual) == 0);
}

static void t_half_tensor_matches_numeric_widening(void) {
  const char *path = half_fixture();
  OspreyList *list = osprey_load_tensor(path, "half");
  CHECK(list != NULL && osprey_list_length(list) == HALF_WORDS);
  for (uint32_t bits = 0; bits < HALF_WORDS; bits++) {
    expect_same_float(at(list, bits), (double)osp_f16_to_f32((uint16_t)bits));
  }
  osp_release(list);
  CHECK(unlink(path) == 0);
}

static void panic_named(void) {
  (void)osprey_tensor_panic("loadTensor: w: no tensor of that name in the header");
}

static void panic_unnamed(void) {
  (void)osprey_tensor_panic(NULL);
}

// Broken-code quarantine: the CPU handler's failed load aborts rather than
// hand the model a fabricated tensor, with or without a recorded reason.
static void t_panic_aborts(void) {
  CHECK(osp_death_signal(panic_named) == SIGABRT);
  CHECK(osp_death_signal(panic_unnamed) == SIGABRT);
}

// [BUILTIN-FILE-BYTES] Bytes come back unsigned and whole through every NUL; a
// count past the end stops at the end; the caller's bad numbers and a missing
// file are errors naming the operation and the subject.
static void t_read_file_bytes(void) {
  unsigned char image[1024];
  for (size_t i = 0; i < sizeof image; i++) image[i] = (unsigned char)(i * 7);
  image[0] = 0;
  image[1] = 255;
  const char *path = temp_file(image, sizeof image);
  static const struct { int64_t offset, count, expected; } spans[] = {
      {0, 1024, 1024}, {0, 1, 1}, {1, 1, 1}, {1000, 24, 24}, {1000, 100, 24}, {1023, 5, 1}, {1024, 5, 0}, {5000, 5, 0}, {7, 0, 0},
  };
  for (size_t s = 0; s < sizeof spans / sizeof spans[0]; s++) {
    osp_io_error_clear();
    OspreyList *list = osprey_read_file_bytes(path, spans[s].offset, spans[s].count);
    CHECK(list != NULL);
    CHECK(osp_io_error() == NULL);
    CHECK(osprey_list_length(list) == spans[s].expected);
    for (int64_t i = 0; i < spans[s].expected; i++) {
      CHECK(osprey_list_get(list, i) == image[spans[s].offset + i]);
    }
    osp_release(list);
  }
  static const struct { int64_t offset, count; const char *reason; } refused[] = {
      {-1, 1, "a negative offset"}, {INT64_MIN, 1, "a negative offset"}, {0, -1, "a negative byte count"}, {0, INT64_MIN, "a negative byte count"},
  };
  for (size_t r = 0; r < sizeof refused / sizeof refused[0]; r++) {
    osp_io_error_clear();
    CHECK(osprey_read_file_bytes(path, refused[r].offset, refused[r].count) == NULL);
    CHECK(strstr(osp_io_error(), "readFileBytes") != NULL);
    CHECK(strstr(osp_io_error(), refused[r].reason) != NULL);
  }
  osp_io_error_clear();
  CHECK(osprey_read_file_bytes("/nonexistent/osprey/weights.bin", 0, 1) == NULL);
  CHECK(strstr(osp_io_error(), "readFileBytes: /nonexistent/osprey/weights.bin: ") != NULL);
  osp_io_error_clear();
  CHECK(osprey_read_file_bytes(NULL, 0, 1) == NULL);
  CHECK(strstr(osp_io_error(), "readFileBytes") != NULL);
  osp_io_error_clear();
  CHECK(osprey_read_file_bytes("/tmp", 0, 1) == NULL);
  CHECK(strstr(osp_io_error(), "readFileBytes: /tmp: ") != NULL);
  // [BUILTIN-FILE-SIZE] The length `stat` reports, and -1 with the reason for
  // a missing path, no path, and a directory.
  osp_io_error_clear();
  CHECK(osprey_file_size(path) == (int64_t)sizeof image);
  CHECK(osp_io_error() == NULL);
  osp_io_error_clear();
  CHECK(osprey_file_size("/nonexistent/osprey/weights.bin") == -1);
  CHECK(strstr(osp_io_error(), "fileSize: /nonexistent/osprey/weights.bin: ") != NULL);
  osp_io_error_clear();
  CHECK(osprey_file_size(NULL) == -1);
  CHECK(strstr(osp_io_error(), "fileSize") != NULL);
  osp_io_error_clear();
  CHECK(osprey_file_size("/tmp") == -1);
  CHECK(strstr(osp_io_error(), "not a regular file") != NULL);
  (void)remove(path);
}

// [BUILTIN-FILE-LISTDIR] One fixture directory, two entry kinds: the listing
// is full child paths, marks directories with a trailing separator, never
// lists "." or "..", and refuses a missing path, a plain file and no path
// with the reason recorded.
static char dir_fixture[128];

static void make_dir_fixture(void) {
  snprintf(dir_fixture, sizeof dir_fixture, "/tmp/osprey_dir_tests_%ld", (long)getpid());
  CHECK(mkdir(dir_fixture, 0755) == 0 || errno == EEXIST);
  char child[160];
  snprintf(child, sizeof child, "%s/leaf.bin", dir_fixture);
  FILE *file = fopen(child, "wb");
  CHECK(file != NULL);
  fputs("x", file);
  (void)fclose(file);
  snprintf(child, sizeof child, "%s/nested", dir_fixture);
  CHECK(mkdir(child, 0755) == 0 || errno == EEXIST);
}

static void t_list_dir(void) {
  make_dir_fixture();
  osp_io_error_clear();
  OspreyList *list = osprey_list_dir(dir_fixture);
  CHECK(list != NULL && osp_io_error() == NULL);
  CHECK(osprey_list_length(list) == 2);
  int saw_file = 0;
  int saw_dir = 0;
  for (int64_t i = 0; i < osprey_list_length(list); i++) {
    const char *entry = (const char *)(intptr_t)osprey_list_get(list, i);
    if (strstr(entry, "/leaf.bin") != NULL) saw_file = 1;
    if (strstr(entry, "/nested/") != NULL) saw_dir = 1;
  }
  CHECK(saw_file && saw_dir);
  osp_release(list);
  osp_io_error_clear();
  char leaf[160];
  snprintf(leaf, sizeof leaf, "%s/leaf.bin", dir_fixture);
  CHECK(osprey_list_dir(leaf) == NULL);
  CHECK(strstr(osp_io_error(), "listDir") != NULL);
  osp_io_error_clear();
  CHECK(osprey_list_dir("/nonexistent/osprey/dir") == NULL);
  CHECK(strstr(osp_io_error(), "listDir: /nonexistent/osprey/dir: ") != NULL);
  osp_io_error_clear();
  CHECK(osprey_list_dir(NULL) == NULL);
  CHECK(strstr(osp_io_error(), "listDir") != NULL);
  (void)unlink(leaf);
  char nested[160];
  snprintf(nested, sizeof nested, "%s/nested", dir_fixture);
  (void)rmdir(nested);
  (void)rmdir(dir_fixture);
}

// [BUILTIN-FILE-KIND] "dir", "file", NULL-with-reason for a missing path and
// NULL-with-reason for no path.
static void t_kind_of(void) {
  make_dir_fixture();
  char leaf[160];
  snprintf(leaf, sizeof leaf, "%s/leaf.bin", dir_fixture);
  FILE *file = fopen(leaf, "wb");
  CHECK(file != NULL);
  fputs("x", file);
  (void)fclose(file);
  char nested[160];
  snprintf(nested, sizeof nested, "%s/nested", dir_fixture);
  CHECK(mkdir(nested, 0755) == 0 || errno == EEXIST);
  osp_io_error_clear();
  char *kind = osprey_kind_of(leaf);
  CHECK(kind != NULL && strcmp(kind, "file") == 0 && osp_io_error() == NULL);
  osp_release(kind);
  osp_io_error_clear();
  kind = osprey_kind_of(nested);
  CHECK(kind != NULL && strcmp(kind, "dir") == 0);
  osp_release(kind);
  osp_io_error_clear();
  CHECK(osprey_kind_of("/nonexistent/osprey/path") == NULL);
  CHECK(strstr(osp_io_error(), "kindOf: /nonexistent/osprey/path: ") != NULL);
  osp_io_error_clear();
  CHECK(osprey_kind_of(NULL) == NULL);
  CHECK(strstr(osp_io_error(), "kindOf") != NULL);
  (void)unlink(leaf);
  (void)rmdir(nested);
  (void)rmdir(dir_fixture);
}

// [BUILTIN-FILE-MAKEDIR] Idempotent creation: the first call makes the
// directory, the second succeeds on EEXIST, and a file in the way is -1 with
// the reason recorded.
static void t_make_dir(void) {
  char fixture[128];
  snprintf(fixture, sizeof fixture, "/tmp/osprey_make_dir_tests_%ld", (long)getpid());
  osp_io_error_clear();
  CHECK(osprey_make_dir(fixture) == 0 && osp_io_error() == NULL);
  CHECK(osprey_make_dir(fixture) == 0);
  char blocker[160];
  snprintf(blocker, sizeof blocker, "%s/file", fixture);
  FILE *file = fopen(blocker, "wb");
  CHECK(file != NULL);
  fputs("x", file);
  (void)fclose(file);
  char under[192];
  snprintf(under, sizeof under, "%s/file/under", fixture);
  osp_io_error_clear();
  CHECK(osprey_make_dir(under) == -1);
  CHECK(strstr(osp_io_error(), "makeDir") != NULL);
  osp_io_error_clear();
  CHECK(osprey_make_dir(NULL) == -1);
  CHECK(strstr(osp_io_error(), "makeDir") != NULL);
  (void)unlink(blocker);
  (void)rmdir(fixture);
}

// osp_path_child: one separator between parts, never "//x" at the root,
// trailing separators collapsed, and truncation refused with `out` emptied.
static void t_path_child(void) {
  char out[16];
  CHECK(osp_path_child(out, sizeof out, "a/b", "c") == 5 && strcmp(out, "a/b/c") == 0);
  CHECK(osp_path_child(out, sizeof out, "a/b//", "c") == 5 && strcmp(out, "a/b/c") == 0);
  CHECK(osp_path_child(out, sizeof out, "/", "x") == 2 && strcmp(out, "/x") == 0);
  CHECK(osp_path_child(out, sizeof out, "", "x") == 2 && strcmp(out, "/x") == 0);
  CHECK(osp_path_child(out, sizeof out, "dir", "fifteen-chars!") == 0 && out[0] == '\0');
  CHECK(osp_path_child(out, sizeof out, "dir", "") == 0);
  CHECK(osp_path_child(out, sizeof out, NULL, "x") == 0);
  CHECK(osp_path_child(NULL, sizeof out, "dir", "x") == 0);
  CHECK(osp_path_child(out, 0, "dir", "x") == 0);
}

// A symlink to an ancestor is listed as a plain entry, never a directory, so
// a walker following the trailing-separator mark can never loop forever.
static void t_list_dir_symlink(void) {
  make_dir_fixture();
  char link[160];
  snprintf(link, sizeof link, "%s/loop", dir_fixture);
  CHECK(symlink(dir_fixture, link) == 0);
  osp_io_error_clear();
  OspreyList *list = osprey_list_dir(dir_fixture);
  CHECK(list != NULL && osprey_list_length(list) == 3);
  for (int64_t i = 0; i < osprey_list_length(list); i++) {
    const char *entry = (const char *)(intptr_t)osprey_list_get(list, i);
    CHECK(strstr(entry, "//") == NULL);
    if (strstr(entry, "/loop") != NULL) CHECK(entry[strlen(entry) - 1] != '/');
  }
  osp_release(list);
  (void)unlink(link);
  char child[160];
  snprintf(child, sizeof child, "%s/leaf.bin", dir_fixture);
  (void)unlink(child);
  snprintf(child, sizeof child, "%s/nested", dir_fixture);
  (void)rmdir(child);
  (void)rmdir(dir_fixture);
}

int main(void) {
  t_tensor_loader(&(TensorLoader){"loadTensor", load, length, at});
  t_read_file_bytes();
  t_list_dir();
  t_kind_of();
  t_make_dir();
  t_path_child();
  t_list_dir_symlink();
  t_half_tensor_matches_numeric_widening();
  t_panic_aborts();
  printf("[ok] tensor_list: %ld assertions\n", g_checks);
  return 0;
}
