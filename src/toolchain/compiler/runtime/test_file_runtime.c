// Assertion-driven tests for file_runtime.c — the portable read_file/write_file
// pair and the failure channel in io_error.h. Split out of
// test_system_runtime.c when file_runtime.c was split out of system_runtime.c:
// a suite covers one translation unit, and both files were over the size
// budget together. Linked with the selected GC/ARC backend by the Makefile's
// _test_c_runtime; POSIX-only harness.
#include <assert.h>
#include <errno.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#include "io_error.h"

#define CHECK(c) assert(c)
#include "file_dir_tests.h"

extern int64_t write_file(char *filename, char *content);
extern char *read_file(char *filename);

#define FILE_ROUNDTRIP_PATH "/tmp/osprey_file_runtime_test.txt"

// write_file returns the byte count written; read_file returns the exact
// content; NULLs and missing files are rejected; rewrites truncate.
void test_file_roundtrip(void) {
  assert(write_file(NULL, "x") == -1);
  assert(write_file(FILE_ROUNDTRIP_PATH, NULL) == -1);
  assert(read_file(NULL) == NULL);
  assert(read_file("/nonexistent_osprey_dir/nope.txt") == NULL);
  assert(write_file("/nonexistent_osprey_dir/nope.txt", "x") == -2);
  const char *content = "line one\nline two\n";
  assert(write_file(FILE_ROUNDTRIP_PATH, (char *)(uintptr_t)content) ==
         (int64_t)strlen(content));
  char *back = read_file(FILE_ROUNDTRIP_PATH);
  assert(back != NULL && strcmp(back, content) == 0);
  free(back);
  assert(write_file(FILE_ROUNDTRIP_PATH, (char *)(uintptr_t) "short") == 5);
  char *truncated = read_file(FILE_ROUNDTRIP_PATH);
  assert(truncated != NULL && strcmp(truncated, "short") == 0); // truncated
  free(truncated);
  assert(write_file(FILE_ROUNDTRIP_PATH, (char *)(uintptr_t) "") == 0);
  char *emptied = read_file(FILE_ROUNDTRIP_PATH);
  assert(emptied != NULL && emptied[0] == '\0');
  free(emptied);
  assert(remove(FILE_ROUNDTRIP_PATH) == 0);
}

#define FIFO_PATH "/tmp/osprey_file_runtime_fifo"
// Comfortably past any pipe buffer, so the writer below cannot finish before
// the reader disappears, and past any malloc slack, so an undersized
// destination buffer corrupts the heap instead of getting away with it.
#define FIFO_PAYLOAD_BYTES 1048576

// Fork a child running `body` against FIFO_PATH and return its pid; the caller
// reaps it. The FIFO is created here so neither side can race its existence.
static pid_t fork_fifo_peer(void (*body)(void)) {
  (void)remove(FIFO_PATH);
  assert(mkfifo(FIFO_PATH, 0600) == 0);
  pid_t peer = fork();
  assert(peer >= 0);
  if (peer == 0) {
    body();
    _exit(0);
  }
  return peer;
}

static void fifo_write_payload(void) {
  FILE *out = fopen(FIFO_PATH, "w");
  if (out != NULL) {
    for (size_t i = 0; i < FIFO_PAYLOAD_BYTES; i++) {
      (void)fputc('A', out);
    }
    (void)fclose(out);
  }
}

// read_file must survive a NON-SEEKABLE stream. fseek/ftell both fail on a
// FIFO and ftell reports -1; sizing the destination from it allocated
// malloc((size_t)-1 + 1) — ZERO bytes — and then fread((size_t)-1) into it,
// overflowing the heap by however many bytes the writer chose to send. The
// length must come from what was actually read, never from a seek that a
// stream is entitled to refuse. Implements [BUILTIN-FILE].
static void test_read_file_non_seekable(void) {
  pid_t writer = fork_fifo_peer(fifo_write_payload);
  char *content = read_file(FIFO_PATH);
  int status = 0;
  (void)waitpid(writer, &status, 0);
  assert(content != NULL);
  assert(strlen(content) == FIFO_PAYLOAD_BYTES);
  for (size_t i = 0; i < FIFO_PAYLOAD_BYTES; i++) {
    assert(content[i] == 'A');
  }
  free(content);
  assert(remove(FIFO_PATH) == 0);
}

static void fifo_reader_hangs_up(void) {
  FILE *in = fopen(FIFO_PATH, "r");
  if (in != NULL) {
    usleep(50000); // let the writer's fopen return and fill the pipe buffer
    (void)fclose(in);
  }
}

// A write that does not reach its destination must NOT report success. stdio
// buffers, so the bytes leave for the file at flush time — which is fclose —
// and a write_file that returns fwrite's count without checking it against the
// requested length, and drops fclose's status entirely, reports a successful
// write of data that was never stored. Silent data loss. Implements
// [BUILTIN-FILE].
static void test_write_file_reports_a_failed_flush(void) {
  void (*previous)(int) = signal(SIGPIPE, SIG_IGN);
  pid_t reader = fork_fifo_peer(fifo_reader_hangs_up);
  char *payload = malloc(FIFO_PAYLOAD_BYTES + 1);
  assert(payload != NULL);
  memset(payload, 'B', FIFO_PAYLOAD_BYTES);
  payload[FIFO_PAYLOAD_BYTES] = '\0';
  int64_t written = write_file(FIFO_PATH, payload);
  int status = 0;
  (void)waitpid(reader, &status, 0);
  free(payload);
  assert(written < 0);
  const char *reason = osp_io_error();
  assert(reason != NULL);
  assert(strstr(reason, strerror(EPIPE)) != NULL);
  (void)signal(SIGPIPE, previous);
  assert(remove(FIFO_PATH) == 0);
#ifdef __linux__
  // A single byte fits in stdio's buffer. /dev/full refuses it at fclose.
  char full[] = "/dev/full";
  char byte[] = "x";
  assert(write_file(full, byte) == -4);
  reason = osp_io_error();
  assert(reason != NULL && strstr(reason, "writeFile: /dev/full: ") == reason);
  assert(strstr(reason, strerror(ENOSPC)) != NULL);
#endif
}

// Every failure carries WHY it failed, and every success retires the previous
// reason so a later failure cannot inherit a stale one. Without this the
// Osprey-level `Error { message }` reads the placeholder word "Error" and a
// missing directory is indistinguishable from a permissions denial or a full
// disk. Implements [BUILTIN-FILE-ERRMSG].
static void test_io_failures_report_a_truthful_reason(void) {
  const char *missing = "/nonexistent_osprey_dir/nope.txt";
  assert(read_file((char *)(uintptr_t)missing) == NULL);
  const char *read_reason = osp_io_error();
  assert(read_reason != NULL);
  assert(strstr(read_reason, missing) != NULL);
  assert(strstr(read_reason, strerror(ENOENT)) != NULL);

  assert(write_file((char *)(uintptr_t)missing, (char *)(uintptr_t) "x") == -2);
  const char *write_reason = osp_io_error();
  assert(write_reason != NULL);
  assert(strstr(write_reason, missing) != NULL);
  assert(strstr(write_reason, strerror(ENOENT)) != NULL);

  // A success must leave nothing behind for the next failure to borrow.
  assert(write_file(FILE_ROUNDTRIP_PATH, (char *)(uintptr_t) "ok") == 2);
  assert(osp_io_error() == NULL);
  char *back = read_file(FILE_ROUNDTRIP_PATH);
  assert(back != NULL && strcmp(back, "ok") == 0);
  assert(osp_io_error() == NULL);
  free(back);
  assert(remove(FILE_ROUNDTRIP_PATH) == 0);

  // A rejected argument is a failure like any other and owes a reason too.
  assert(write_file(NULL, (char *)(uintptr_t) "x") == -1);
  assert(osp_io_error() != NULL);
  assert(read_file(NULL) == NULL);
  assert(osp_io_error() != NULL);
}

#define FILE_DIRECTORY_PATH "/tmp/osprey_file_runtime_dir"

// A read that fails AFTER fopen succeeded must not hand back a partial buffer
// as if it were the file. Reading a directory is exactly that shape: the open
// is legal, every fread fails, and the only witness is ferror. Dropping it
// would return an empty string for a path the process could not read — the
// same silent-success defect the write path guards against.
// Implements [BUILTIN-FILE].
static void test_read_file_reports_a_stream_error(void) {
  (void)rmdir(FILE_DIRECTORY_PATH);
  assert(mkdir(FILE_DIRECTORY_PATH, 0700) == 0);
  char *content = read_file((char *)(uintptr_t)FILE_DIRECTORY_PATH);
  assert(content == NULL && "a stream error must not yield a buffer");
  const char *reason = osp_io_error();
  assert(reason != NULL && "a failed read owes a reason");
  assert(strncmp(reason, "readFile: ", strlen("readFile: ")) == 0 &&
         "the reason names the operation that failed");
  assert(strstr(reason, FILE_DIRECTORY_PATH) != NULL &&
         "the reason names the path that failed");
  assert(rmdir(FILE_DIRECTORY_PATH) == 0);
}

// The reason channel is the only thing between an Osprey `Error { message }`
// and the placeholder word "Error", so every shape it can be handed must
// produce a truthful sentence: a real errno, an errno the C library cannot
// name, a failure with no errno at all, and the NULL op/subject the header
// documents. Implements [BUILTIN-FILE-ERRMSG].
static void test_error_channel_shapes(void) {
  const char *subject = "/some/path";
  const char *read_prefix = "readFile: /some/path: ";
  osp_io_error_clear();
  assert(osp_io_error() == NULL && "a cleared channel holds nothing");
  assert(osp_io_error_take() == NULL &&
         "take on a cleared channel yields nothing to own");

  osp_io_error_set("readFile", subject, ENOENT);
  const char *reason = osp_io_error();
  assert(reason != NULL);
  assert(strncmp(reason, read_prefix, strlen(read_prefix)) == 0 &&
         "the sentence is op, then subject, then reason");
  assert(strstr(reason, strerror(ENOENT)) != NULL &&
         "a real errno is spelled out, not printed as a number");

  // A reason must OUTLIVE the channel: `take` hands back an owned copy, so a
  // Result carrying it cannot later read whatever failed most recently.
  char *owned = osp_io_error_take();
  assert(owned != NULL && strcmp(owned, reason) == 0);
  osp_io_error_set("writeFile", "/other", EACCES);
  assert(strcmp(owned, reason) != 0 &&
         "the taken copy is independent of the channel buffer");
  assert(strstr(owned, "readFile") != NULL &&
         "the taken copy still reads the failure it was taken from");
  assert(strstr(osp_io_error(), "writeFile: /other: ") == osp_io_error());
  assert(strstr(osp_io_error(), strerror(EACCES)) != NULL);
  free(owned);

  // An errno the C library cannot name still yields text — never an empty or
  // uninitialised tail that would read as a successful, reasonless failure.
  const int unnameable = 999999;
  osp_io_error_set("readFile", subject, unnameable);
  const char *unknown = osp_io_error();
  assert(unknown != NULL);
  assert(strncmp(unknown, read_prefix, strlen(read_prefix)) == 0);
  assert(strlen(unknown) > strlen(read_prefix) &&
         "an unnameable errno still produces a reason");
  assert(strstr(unknown, strerror(ENOENT)) == NULL &&
         "and it is not the previous failure's reason left in the buffer");

  // A failure with no errno at all is named as such, not reported as errno 0
  // (which spells "Undefined error: 0" or, worse, "Success").
  osp_io_error_set("writeFile", subject, 0);
  assert(strcmp(osp_io_error(), "writeFile: /some/path: unspecified failure") ==
             0 &&
         "a cause that is not an errno is stated plainly");

  // NULL op and NULL subject are documented inputs, not crashes.
  osp_io_error_set("readFile", NULL, EINVAL);
  assert(strstr(osp_io_error(), "readFile: <null path>: ") == osp_io_error());
  osp_io_error_set(NULL, subject, EINVAL);
  assert(strstr(osp_io_error(), "io: /some/path: ") == osp_io_error());
  osp_io_error_set(NULL, NULL, 0);
  assert(strcmp(osp_io_error(), "io: <null path>: unspecified failure") == 0);

  osp_io_error_clear();
  assert(osp_io_error() == NULL && "clear retires the reason");
  assert(osp_io_error_take() == NULL);
}

// --- [PORT-STRING-NUL] embedded-NUL guard, on real files ---------------------
//
// read_stream refuses a stream whose bytes hold a NUL, because a char* cannot
// carry one. These cases drive that guard through the filesystem: a real file
// of a real size, holding a real NUL, at a position that matters. Each one
// proves three things that a hand-built in-memory fixture cannot: the fixture
// really was `len` bytes on disk, the guard really fired with the operation and
// path that failed, and the same file still reads back whole once the NUL is
// gone. The guard itself is unchanged; these are its witnesses.
#define NUL_TEMPLATE "/tmp/osprey_nul_XXXXXX"
#define NUL_OLD_OP "writeFile"
#define NUL_OLD_PATH "/tmp/osprey_nul_seeded_reason"
#define NUL_SENTENCE "text contains NUL; use a binary reader"

// mkstemp a unique path and hand back an open binary stream on it, so no two
// cases can collide on a name and no case can read another's leftovers.
static FILE *nul_open_unique(char *path, size_t cap) {
  assert(strlen(NUL_TEMPLATE) < cap);
  (void)snprintf(path, cap, "%s", NUL_TEMPLATE);
  int fd = mkstemp(path);
  assert(fd >= 0);
  FILE *out = fdopen(fd, "wb");
  assert(out != NULL);
  return out;
}

// Write `len` raw bytes — NULs included — and close. A short write is a failure.
static void nul_write_raw(FILE *out, const char *bytes, size_t len) {
  assert(fwrite(bytes, 1, len, out) == len);
  assert(fflush(out) == 0);
  assert(fclose(out) == 0);
}

// The length the FILESYSTEM reports, which is the only claim that matters here.
static size_t nul_disk_length(const char *path) {
  struct stat info;
  assert(stat(path, &info) == 0);
  assert(info.st_size >= 0);
  return (size_t)info.st_size;
}

// A fixture: `len` bytes of 'a', with the byte at `nul_at` replaced by a NUL.
// SIZE_MAX means "no NUL" — the safe rewrite of the same length.
static char *nul_payload(size_t len, size_t nul_at) {
  char *bytes = malloc(len + 1);
  assert(bytes != NULL);
  memset(bytes, 'a', len);
  bytes[len] = '\0';
  if (nul_at < len) {
    bytes[nul_at] = '\0';
  }
  return bytes;
}

// Read the file back with raw bytes and prove it holds exactly `len` of them,
// byte for byte — a NUL that did not reach the disk would make the guard look
// loaded when it was testing nothing.
static void nul_assert_bytes(const char *path, const char *want, size_t len) {
  FILE *in = fopen(path, "rb");
  assert(in != NULL);
  char *got = malloc(len + 1);
  assert(got != NULL);
  size_t n = fread(got, 1, len + 1, in);
  assert(ferror(in) == 0);
  assert(n == len && "the fixture holds exactly the bytes written");
  assert(memcmp(got, want, len) == 0 && "every byte survived the write");
  free(got);
  assert(fclose(in) == 0);
}

// The reason must name the operation AND the exact path that failed, not just
// carry the right sentence for some other file.
static int nul_reason_names(const char *path, const char *reason) {
  char prefix[256];
  (void)snprintf(prefix, sizeof prefix, "readFile: %s: ", path);
  return strncmp(reason, prefix, strlen(prefix)) == 0;
}

// A same-length ASCII rewrite must read back COMPLETE — the guard refuses a NUL
// stream, but the file it refused is not poisoned, and the buffer handed back
// is the whole text, never a prefix cut at some earlier boundary.
static void nul_assert_safe_rewrite(const char *path, size_t len) {
  char *safe = nul_payload(len, SIZE_MAX);
  assert(write_file((char *)(uintptr_t)path, safe) == (int64_t)len);
  assert(nul_disk_length(path) == len);
  nul_assert_bytes(path, safe, len);
  char *back = read_file((char *)(uintptr_t)path);
  assert(back != NULL && "a NUL-free file still reads");
  assert(strlen(back) == len && "nothing was clipped off the text");
  assert(memcmp(back, safe, len) == 0 && "the full text came back");
  free(back);
  free(safe);
}

// One fixture already on disk at `path`: refused with a truthful reason, its
// reason owned by the caller after a `take` that must NOT clear the channel,
// then rewritten safe and read whole with the channel retired.
static void nul_assert_fixture(const char *path, const char *bytes, size_t len) {
  char expected_old[256], expected_nul[256];
  (void)snprintf(expected_nul, sizeof expected_nul, "readFile: %s: %s", path, NUL_SENTENCE);
  (void)snprintf(expected_old, sizeof expected_old, "%s: %s: %s", NUL_OLD_OP,
                 NUL_OLD_PATH, strerror(EACCES));
  assert(nul_disk_length(path) == len && "the fixture is the size it claims");
  nul_assert_bytes(path, bytes, len);
  osp_io_error_set(NUL_OLD_OP, NUL_OLD_PATH, EACCES);
  char *old = osp_io_error_take();
  assert(old != NULL && strcmp(old, expected_old) == 0);
  assert(read_file((char *)(uintptr_t)path) == NULL &&
         "text containing a NUL is refused, not returned as its prefix");
  const char *reason = osp_io_error();
  assert(reason != NULL && nul_reason_names(path, reason) &&
         "the reason names the operation and the exact path");
  assert(strstr(reason, NUL_SENTENCE) != NULL &&
         "and says the text holds a NUL");
  char *owned = osp_io_error_take();
  assert(owned != NULL && strcmp(owned, reason) == 0);
  assert(strcmp(owned, expected_nul) == 0);
  assert(osp_io_error() != NULL &&
         "take hands back a copy; it does not retire the reason");
  nul_assert_safe_rewrite(path, len);
  assert(osp_io_error() == NULL && "a successful read retires the reason");
  assert(strcmp(owned, expected_nul) == 0 && "the saved NUL reason survives a successful rewrite");
  assert(strcmp(old, expected_old) == 0 &&
         "the saved reason is unchanged by later calls");
  free(owned);
  free(old);
  assert(unlink(path) == 0);
}

// An empty file holds no NUL, so it is the control: it must read back as empty
// text with NO reason, proving the guard is scanning bytes it actually read and
// not reporting an empty stream as a NUL one.
static void nul_case_empty(void) {
  char path[64];
  FILE *out = nul_open_unique(path, sizeof path);
  nul_write_raw(out, "", 0);
  printf("    [nul] len=0 nul_at=none (empty, control)\n");
  assert(nul_disk_length(path) == 0 && "the fixture is the size it claims");
  assert(write_file((char *)(uintptr_t)path, (char *)(uintptr_t) "") == 0);
  char *back = read_file((char *)(uintptr_t)path);
  assert(back != NULL && back[0] == '\0' && "empty text, not a refusal");
  assert(osp_io_error() == NULL && "an empty read owes no reason");
  free(back);
  assert(unlink(path) == 0);
}

// One case: build `len` bytes with a NUL at `nul_at`, put them on disk, verify.
static void nul_case(size_t len, size_t nul_at) {
  char path[64];
  FILE *out = nul_open_unique(path, sizeof path);
  char *bytes = nul_payload(len, nul_at);
  nul_write_raw(out, bytes, len);
  printf("    [nul] len=%zu nul_at=embedded offset=%zu\n", len, nul_at);
  nul_assert_fixture(path, bytes, len);
  free(bytes);
}

// The 9-byte variant deliberately puts the NUL BETWEEN valid UTF-8 sequences:
// 'a', U+00E9, NUL, U+00E9, U+2192. The guard must fire on the byte, not on
// some ASCII-only or byte-width assumption, and each neighbour is a whole
// code point so nothing here is invalid UTF-8.
static void nul_case_unicode9(void) {
  char path[64];
  FILE *out = nul_open_unique(path, sizeof path);
  const char unicode9[9] = {'a',           (char)0xC3, (char)0xA9, '\0',
                            (char)0xC3, (char)0xA9, (char)0xE2, (char)0x86,
                            (char)0x92};
  nul_write_raw(out, unicode9, sizeof unicode9);
  printf("    [nul] len=9 nul_at=embedded between UTF-8 neighbours\n");
  nul_assert_fixture(path, unicode9, sizeof unicode9);
}

// Sizes bracket the 64 KiB read chunk: one under it, the exact boundary, one
// over it (the first realloc), and a second-chunk file. Each non-empty size is
// tried at the start, the middle and the end, because a guard that only scans
// the first chunk, or skips the last byte of a chunk, passes one and fails the
// others.
static void run_nul_filesystem_tests(void) {
  static const size_t spans[] = {9, 65534, 65535, 65536, 65537, 131073};
  nul_case_empty();      // control: no NUL, so no refusal
  nul_case(1, 0);        // a lone NUL: start and end are the same byte
  nul_case_unicode9();   // NUL between UTF-8 code points
  for (size_t i = 0; i < sizeof spans / sizeof spans[0]; i++) {
    nul_case(spans[i], 0);
    nul_case(spans[i], spans[i] / 2);
    nul_case(spans[i], spans[i] - 1);
  }
}

int main(int argc, char **argv) {
  if (argc == 2 && strcmp(argv[1], "--nul-files") == 0) {
    run_nul_filesystem_tests();
    return 0;
  }
  assert(argc == 1);
  printf("Running File Runtime Tests...\n\n");

  test_file_roundtrip();
  test_read_file_non_seekable();
  test_write_file_reports_a_failed_flush();
  test_io_failures_report_a_truthful_reason();
  test_read_file_reports_a_stream_error();
  test_error_channel_shapes();
  run_directory_tests();
  run_nul_filesystem_tests();

  printf("=== ALL FILE RUNTIME TESTS PASSED ===\n");
  return 0;
}
