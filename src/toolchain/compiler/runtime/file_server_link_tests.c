// The file server's folder links, end to end: a folder entry that is a link, one
// replaced by a link mid-request, or one that opens but is not a regular file is
// never served ([FFI-FILE-SERVER-FOLDER]). Split from file_server_tests.c so each
// test TU stays under the repo size budget.
#include "file_server_tests.h"

#include <assert.h>
#include <stdatomic.h>
#include <sys/stat.h>

static void assert_folder_symlink_refused(const char *folder, int64_t port) {
  char outside[] = "/tmp/osprey-net-outside.XXXXXX";
  int fd = mkstemp(outside);
  assert(fd >= 0 && write(fd, "outside", 7) == 7 && close(fd) == 0);
  char link[512];
  snprintf(link, sizeof link, "%s/escape.txt", folder);
  assert(symlink(outside, link) == 0);
  static const Exchange refused[] = {
      {"GET /escape.txt HTTP/1.1\r\n\r\n", 404, {NULL}, NULL, 0, 0},
      {"HEAD /escape.txt HTTP/1.1\r\n\r\n", 404, {NULL}, NULL, 0, 0},
  };
  expect_exchanges(port, refused, sizeof refused / sizeof refused[0]);
  assert(unlink(link) == 0 && unlink(outside) == 0);
}

// [FFI-FILE-SERVER-FOLDER] An entry that opens but is not a regular file is refused:
// a subfolder, and a FIFO, which must answer at once rather than wait for a writer.
static void assert_non_files_refused(const char *folder, int64_t port) {
  char sub[512], fifo[512];
  snprintf(sub, sizeof sub, "%s/sub", folder);
  snprintf(fifo, sizeof fifo, "%s/pipe.ts", folder);
  assert(mkdir(sub, 0700) == 0 && mkfifo(fifo, 0600) == 0);
  static const Exchange refused[] = {
      {"GET /sub HTTP/1.1\r\n\r\n", 404, {NULL}, NULL, 0, 0},
      {"GET /pipe.ts HTTP/1.1\r\n\r\n", 404, {NULL}, NULL, 0, 0},
      {"HEAD /pipe.ts HTTP/1.1\r\n\r\n", 404, {NULL}, NULL, 0, 0},
  };
  expect_exchanges(port, refused, sizeof refused / sizeof refused[0]);
  assert(rmdir(sub) == 0 && unlink(fifo) == 0);
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
    assert(symlink(swap->outside, staged) == 0);
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
  char outside[] = "/tmp/osprey-net-swapped.XXXXXX";
  int fd = mkstemp(outside);
  assert(fd >= 0 && write(fd, "outside", 7) == 7 && close(fd) == 0);
  Swapper swap = {folder, outside, false};
  swap_in(&swap, false);
  pthread_t thread;
  assert(pthread_create(&thread, NULL, swap_thread, &swap) == 0);
  int served = 0;
  for (int i = 0; i < 600; i++) {
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

void run_file_server_link_tests(const char *folder, int64_t port) {
  assert_folder_symlink_refused(folder, port);
  assert_swapped_link_never_escapes(folder, port);
  assert_non_files_refused(folder, port);
}
