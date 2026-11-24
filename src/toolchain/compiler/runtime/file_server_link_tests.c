// The file server's folder links, end to end: a folder entry that is a link, one
// replaced by a link mid-request, or one that opens but is not a regular file is
// never served ([FFI-FILE-SERVER-FOLDER]). Windows runs the link and subfolder
// cases; it has no FIFO, and its rename cannot replace a name atomically.
#include <assert.h>
#include <stdatomic.h>
#include <sys/stat.h>

// Last: on Windows it maps close() to closesocket(), which would rename the CRT's
// own close() in any system header after it.
#include "file_server_tests.h"

// A new file outside every served folder holding "outside": its path in `out`.
static void write_outside(char *out, size_t cap, const char *stem) {
  FILE *file = fdopen(fs_temp_file(out, cap, stem), "wb");
  assert(file != NULL && fputs("outside", file) >= 0 && fclose(file) == 0);
}

// Makes `link` a symbolic link to the file `target`. Windows hosted runners are
// administrators; elsewhere on Windows this needs Developer Mode.
static void make_link(const char *target, const char *link) {
#ifdef _WIN32
  assert(CreateSymbolicLinkA(link, target, SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE) != 0);
#else
  assert(symlink(target, link) == 0);
#endif
}

static void assert_folder_symlink_refused(const char *folder, int64_t port) {
  char outside[512], link[512];
  write_outside(outside, sizeof outside, "osprey-net-outside");
  snprintf(link, sizeof link, "%s/escape.txt", folder);
  make_link(outside, link);
  static const Exchange refused[] = {
      {"GET /escape.txt HTTP/1.1\r\n\r\n", 404, {NULL}, NULL, 0, 0},
      {"HEAD /escape.txt HTTP/1.1\r\n\r\n", 404, {NULL}, NULL, 0, 0},
  };
  expect_exchanges(port, refused, sizeof refused / sizeof refused[0]);
  assert(unlink(link) == 0 && unlink(outside) == 0);
}

// [FFI-FILE-SERVER-FOLDER] An entry that opens but is not a regular file is refused.
static void assert_subfolder_refused(const char *folder, int64_t port) {
  char sub[512];
  snprintf(sub, sizeof sub, "%s/sub", folder);
  fs_make_folder(sub);
  static const Exchange refused[] = {
      {"GET /sub HTTP/1.1\r\n\r\n", 404, {NULL}, NULL, 0, 0},
      {"HEAD /sub HTTP/1.1\r\n\r\n", 404, {NULL}, NULL, 0, 0},
  };
  expect_exchanges(port, refused, sizeof refused / sizeof refused[0]);
  assert(rmdir(sub) == 0);
}

#ifndef _WIN32
// A FIFO must answer at once rather than wait for a writer that never comes.
static void assert_fifo_refused(const char *folder, int64_t port) {
  char fifo[512];
  snprintf(fifo, sizeof fifo, "%s/pipe.ts", folder);
  assert(mkfifo(fifo, 0600) == 0);
  static const Exchange refused[] = {
      {"GET /pipe.ts HTTP/1.1\r\n\r\n", 404, {NULL}, NULL, 0, 0},
      {"HEAD /pipe.ts HTTP/1.1\r\n\r\n", 404, {NULL}, NULL, 0, 0},
  };
  expect_exchanges(port, refused, sizeof refused / sizeof refused[0]);
  assert(unlink(fifo) == 0);
}

// The entry a swapper keeps replacing: a real file inside the folder, then a link to
// a file outside it, each renamed over `race.txt` in one atomic step.
typedef struct {
  const char *folder;
  const char *outside;
  atomic_bool done;
} Swapper;

static void swap_in(const Swapper *swap, bool as_link) {
  char staged[512], race[512];
  snprintf(staged, sizeof staged, "%s/.staged", swap->folder);
  snprintf(race, sizeof race, "%s/race.txt", swap->folder);
  if (as_link) {
    make_link(swap->outside, staged);
  } else {
    write_text(swap->folder, ".staged", "inside!");
  }
  assert(rename(staged, race) == 0);
}

static void *swap_thread(void *arg) {
  Swapper *swap = arg;
  for (bool as_link = false; !atomic_load(&swap->done); as_link = !as_link) {
    swap_in(swap, as_link);
  }
  return NULL;
}

// [FFI-FILE-SERVER-FOLDER] Replacing the checked entry between the server's lookup and
// its open never serves outside bytes: every answer is the inside file or a 404.
static void assert_swapped_link_never_escapes(const char *folder, int64_t port) {
  char outside[512];
  write_outside(outside, sizeof outside, "osprey-net-swapped");
  Swapper swap = {folder, outside, false};
  swap_in(&swap, false);
  pthread_t thread;
  assert(pthread_create(&thread, NULL, swap_thread, &swap) == 0);
  int served = 0;
  // At least 600 requests, and more until one lands while the inside file is in place: a
  // slow (coverage-instrumented) server can lose every early race to the swapper, which
  // proves nothing about serving. The bound keeps a server that never serves a failure.
  for (int i = 0; i < 600 || (served == 0 && i < 60000); i++) {
    Reply reply;
    fetch((int)port, "GET /race.txt HTTP/1.1\r\n\r\n", &reply);
    assert(reply.status == 200 || reply.status == 404);
    assert(reply.body_len == 0 || (reply.body_len == 7 && memcmp(reply.body, "inside!", 7) == 0));
    served += reply.status == 200;
    free(reply.body);
  }
  atomic_store(&swap.done, true);
  assert(pthread_join(thread, NULL) == 0 && served > 0);
  char race[512];
  snprintf(race, sizeof race, "%s/race.txt", folder);
  assert(unlink(race) == 0 && unlink(outside) == 0);
}
#endif

void run_file_server_link_tests(const char *folder, int64_t port) {
  assert_folder_symlink_refused(folder, port);
  assert_subfolder_refused(folder, port);
#ifndef _WIN32
  assert_fifo_refused(folder, port);
  assert_swapped_link_never_escapes(folder, port);
#endif
}
