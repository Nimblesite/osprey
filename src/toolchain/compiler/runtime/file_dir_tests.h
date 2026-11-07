// Directory tests for file_runtime.c's listDir, kindOf and makeDir and for
// osprey_path.h's osp_path_child [BUILTIN-FILE-LISTDIR] [BUILTIN-FILE-KIND]
// [BUILTIN-FILE-MAKEDIR]. Included by test_file_runtime.c, the suite that
// owns file_runtime.c's coverage, with the read and directory out-of-memory
// arms forced through test_alloc.h; POSIX-only harness.
#ifndef OSPREY_FILE_DIR_TESTS_H
#define OSPREY_FILE_DIR_TESTS_H

#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#include "collection_runtime.h"
#include "io_error.h"
#include "memory_hooks.h"
#include "osprey_path.h"
#include "test_alloc.h"

OspreyList *osprey_list_dir(const char *path);
int64_t osprey_make_dir(const char *path);
char *osprey_kind_of(const char *path);

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

// [BUILTIN-FILE-KIND] Anything neither a directory nor a regular file (a FIFO
// here) is "other"; [BUILTIN-FILE-MAKEDIR] a missing parent is -1 with its reason.
static void t_kind_other_and_missing_parent(void) {
  char fifo[128];
  snprintf(fifo, sizeof fifo, "/tmp/osprey_kind_fifo_%ld", (long)getpid());
  (void)unlink(fifo);
  CHECK(mkfifo(fifo, 0600) == 0);
  osp_io_error_clear();
  char *kind = osprey_kind_of(fifo);
  CHECK(kind != NULL && strcmp(kind, "other") == 0 && osp_io_error() == NULL);
  osp_release(kind);
  (void)unlink(fifo);
  osp_io_error_clear();
  CHECK(osprey_make_dir("/nonexistent/osprey/parent/child") == -1);
  CHECK(strstr(osp_io_error(), "makeDir: /nonexistent/osprey/parent/child: ") != NULL);
}

// Out-of-memory arms, forced through test_alloc.h's injector: every one is NULL
// (or -1) with ENOMEM's reason on the channel and leaks nothing it allocated.
extern char *read_file(char *filename);
extern int64_t write_file(char *filename, char *content);

static void t_read_file_out_of_memory(void) {
  char path[128];
  snprintf(path, sizeof path, "/tmp/osprey_oom_read_%ld", (long)getpid());
  CHECK(write_file(path, "small") == 5);
  long live = osp_alloc_live();
  osp_alloc_fail_size(65536); // the first read buffer
  CHECK(read_file(path) == NULL && strstr(osp_io_error(), "readFile") != NULL);
  osp_alloc_fail_off();
  CHECK(osp_alloc_live() == live);
  static char big[70001];
  memset(big, 'b', sizeof big - 1);
  CHECK(write_file(path, big) == (int64_t)(sizeof big - 1));
  live = osp_alloc_live();
  osp_alloc_fail_size(131072); // the doubled buffer once 64 KiB is full
  CHECK(read_file(path) == NULL && strstr(osp_io_error(), "readFile") != NULL);
  osp_alloc_fail_off();
  CHECK(osp_alloc_live() == live);
  (void)unlink(path);
}

static void t_directory_out_of_memory(void) {
  make_dir_fixture();
  char leaf[160];
  int leaf_length = snprintf(leaf, sizeof leaf, "%s/leaf.bin", dir_fixture);
  osp_alloc_fail_size((size_t)leaf_length + 1); // the leaf entry's path copy
  CHECK(osprey_list_dir(dir_fixture) == NULL && strstr(osp_io_error(), "listDir") != NULL);
  osp_alloc_fail_off();
  osp_alloc_fail_size(sizeof "file"); // the kind's copy
  CHECK(osprey_kind_of(leaf) == NULL && strstr(osp_io_error(), "kindOf") != NULL);
  osp_alloc_fail_off();
  char nested[160];
  snprintf(nested, sizeof nested, "%s/nested", dir_fixture);
  (void)unlink(leaf);
  (void)rmdir(nested);
  (void)rmdir(dir_fixture);
}

// Every directory case, in the order the runtime depends on them.
static void run_directory_tests(void) {
  t_list_dir();
  t_kind_of();
  t_make_dir();
  t_path_child();
  t_list_dir_symlink();
  t_kind_other_and_missing_parent();
  t_read_file_out_of_memory();
  t_directory_out_of_memory();
}

#endif // OSPREY_FILE_DIR_TESTS_H
