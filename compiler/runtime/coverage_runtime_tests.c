// Assertion-driven tests for the coverage collector [TESTING-COVERAGE-RUNTIME]
// (docs/specs/0027-TestingFramework.md). A failed assert aborts the binary.
// POSIX-only test harness (fork/waitpid); the unit under test is portable C11.
#include <assert.h>
#include <dlfcn.h>
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#include "test_alloc.h"

void osp_cov_register_line(int64_t line, const int64_t *counter);

// Must match coverage_runtime.c: the dump is BUILT here and renamed onto the
// destination, so this is the path that must be gone once a flush is over —
// whether it published or not.
#define COV_PARTIAL_SUFFIX ".partial"
// Must match coverage_runtime.c and the reader in
// crates/osprey-cli/src/test_cmd.rs. A writer and a reader that disagree here
// turn every dump into "no coverage data" — or, worse, into a partial one.
#define COV_DUMP_HEADER "# osprey-coverage v2"
#define COV_DUMP_FOOTER_PREFIX "# rows "

enum {
    COV_TEST_LINES = 3,
    COV_PATH_MAX = 256,
    COV_DIAG_MAX = 512
};

static const int64_t test_lines[COV_TEST_LINES] = {3, 7, 12};
static int64_t test_counters[COV_TEST_LINES] = {0, 0, 0};

static void register_all(void) {
    for (int i = 0; i < COV_TEST_LINES; i += 1) {
        osp_cov_register_line(test_lines[i], &test_counters[i]);
    }
}

// The child registers with OSPREY_COVERAGE set, bumps counters the way
// instrumented IR does (plain in-place adds), and exits normally so the
// atexit-armed dump runs [TESTING-COVERAGE-ENV].
static void run_child(const char *path) {
    assert(setenv("OSPREY_COVERAGE", path, 1) == 0);
    register_all();
    test_counters[0] = 2; // "execute" lines 3 and 12, leave 7 uncovered
    test_counters[2] = 1;
    exit(0);
}

// The dump lists every registered line — zero-hit rows included, so a reader
// needs no separate line universe — and ENDS with the row-count footer, which
// is what makes a lost row detectable instead of invisible
// [TESTING-COVERAGE-DUMP].
static void verify_dump(const char *path) {
    const int64_t expected_hits[COV_TEST_LINES] = {2, 0, 1};
    FILE *in = fopen(path, "r");
    assert(in != NULL && "a completed dump is published at the dump path");
    char header[COV_PATH_MAX];
    assert(fgets(header, sizeof(header), in) != NULL);
    assert(strcmp(header, COV_DUMP_HEADER "\n") == 0 &&
           "the header names the protocol version the reader parses");
    // BYTE-exact, row by row. `fscanf("%lld %lld")` was the obvious spelling and
    // pinned nothing about FRAMING: it skips any run of whitespace, so a tab
    // separator, a doubled space, or a row with no newline all parse here --
    // while the reader in crates/osprey-cli/src/test_cmd.rs, which splits on one
    // ' ' and requires the terminating newline, rejects the whole file. A writer
    // and a reader that disagree about framing turn every dump into "no coverage
    // data" with the tests on both sides green.
    for (int64_t i = 0; i < COV_TEST_LINES; i += 1) {
        char row[COV_PATH_MAX];
        assert(fgets(row, sizeof(row), in) != NULL &&
               "every registered line has a row");
        char expected[COV_PATH_MAX];
        int n = snprintf(expected, sizeof(expected), "%" PRId64 " %" PRId64 "\n",
                         test_lines[i], expected_hits[i]);
        assert(n > 0 && (size_t)n < sizeof(expected));
        assert(strcmp(row, expected) == 0 &&
               "one space, newline-terminated, in registration order");
    }
    char footer[COV_PATH_MAX];
    assert(fgets(footer, sizeof(footer), in) != NULL &&
           "the dump ends with its completion footer");
    char expected_footer[COV_PATH_MAX];
    int written = snprintf(expected_footer, sizeof(expected_footer),
                           "%s%d\n", COV_DUMP_FOOTER_PREFIX, COV_TEST_LINES);
    assert(written > 0 && (size_t)written < sizeof(expected_footer));
    assert(strcmp(footer, expected_footer) == 0 &&
           "the footer declares exactly how many rows were written");
    assert(fgets(footer, sizeof(footer), in) == NULL &&
           "nothing follows the footer");
    assert(fclose(in) == 0);
    assert(remove(path) == 0);
    char staging[COV_PATH_MAX];
    written = snprintf(staging, sizeof(staging), "%s%s", path,
                       COV_PARTIAL_SUFFIX);
    assert(written > 0 && (size_t)written < sizeof(staging));
    assert(access(staging, F_OK) != 0 &&
           "publishing by rename leaves no staging file behind");
}

// Without OSPREY_COVERAGE, registration stays inert: no dump file appears.
static void test_inert_without_env(const char *path) {
    assert(unsetenv("OSPREY_COVERAGE") == 0);
    pid_t pid = fork();
    assert(pid >= 0);
    if (pid == 0) {
        register_all();
        exit(0);
    }
    int status = 0;
    assert(waitpid(pid, &status, 0) == pid);
    assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
    FILE *in = fopen(path, "r");
    assert(in == NULL);
}

static void test_dump_written_on_exit(const char *path) {
    pid_t pid = fork();
    assert(pid >= 0);
    if (pid == 0) {
        run_child(path);
    }
    int status = 0;
    assert(waitpid(pid, &status, 0) == pid);
    assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
    verify_dump(path);
}

// Drains `fd` into `out` as a NUL-terminated string.
static void read_all(int fd, char *out, size_t out_size) {
    size_t filled = 0;
    ssize_t got = 0;
    while (filled + 1 < out_size &&
           (got = read(fd, out + filled, out_size - filled - 1)) > 0) {
        filled += (size_t)got;
    }
    out[filled] = '\0';
}

// Every failure mode of this unit is a message on stderr plus a NORMAL exit:
// coverage under-reports, it never aborts the program it is measuring
// [TESTING-COVERAGE-RUNTIME].
static void assert_child_exited_cleanly(pid_t pid) {
    int status = 0;
    assert(waitpid(pid, &status, 0) == pid);
    assert(WIFEXITED(status) &&
           "a coverage failure must not abort or signal the program");
    assert(WEXITSTATUS(status) == 0 &&
           "a coverage failure must not change the program's exit status");
}

static void child_registers_to(const char *path) {
    assert(setenv("OSPREY_COVERAGE", path, 1) == 0);
    register_all();
}

// Runs `body` in a child with stderr captured and returns its diagnostic.
static void diagnostic_from_child(void (*body)(const char *), const char *arg,
                                  char *out, size_t out_size) {
    int err_fd[2];
    assert(pipe(err_fd) == 0);
    pid_t pid = fork();
    assert(pid >= 0);
    if (pid == 0) {
        assert(close(err_fd[0]) == 0);
        assert(dup2(err_fd[1], STDERR_FILENO) == STDERR_FILENO);
        body(arg);
        exit(0);
    }
    assert(close(err_fd[1]) == 0);
    read_all(err_fd[0], out, out_size);
    assert(close(err_fd[0]) == 0);
    assert_child_exited_cleanly(pid);
}

// A dump destination the process cannot open is REPORTED. Coverage that could
// not be written must never be indistinguishable from coverage that found
// nothing [TESTING-COVERAGE-DUMP].
static void test_unwritable_dump_is_reported(void) {
    const char *unwritable = "/osprey-coverage-no-such-directory/dump.txt";
    char diag[COV_DIAG_MAX];
    diagnostic_from_child(child_registers_to, unwritable, diag, sizeof(diag));
    assert(strstr(diag, "osprey coverage: cannot write") != NULL &&
           "an unopenable dump path must be diagnosed");
    assert(strstr(diag, unwritable) != NULL &&
           "the diagnostic must name the path that failed");
    assert(strstr(diag, "cannot finish") == NULL &&
           "a dump that never opened cannot also report a close failure");
    FILE *in = fopen(unwritable, "r");
    assert(in == NULL && "no dump file may appear at an unwritable path");
}

// A dump that is written but cannot be PUBLISHED is reported too. Renaming
// onto the destination is the last step that can fail, and it is the only one
// whose failure leaves a COMPLETE dump sitting at a path no reader looks at —
// silence there reads as "this program covered nothing"
// [TESTING-COVERAGE-DUMP].
static void test_unpublishable_dump_is_reported(const char *path) {
    char staging[COV_PATH_MAX];
    int n = snprintf(staging, sizeof(staging), "%s%s", path,
                     COV_PARTIAL_SUFFIX);
    assert(n > 0 && (size_t)n < sizeof(staging));
    (void)remove(path);
    (void)remove(staging);
    // A file can never be renamed over a directory, so every write and the
    // close succeed and the flush fails at exactly the publishing step.
    assert(mkdir(path, S_IRWXU) == 0);
    char diag[COV_DIAG_MAX];
    diagnostic_from_child(child_registers_to, path, diag, sizeof(diag));
    assert(strstr(diag, "osprey coverage: cannot finish") != NULL &&
           "a dump that could not be published must be diagnosed");
    assert(strstr(diag, path) != NULL &&
           "the diagnostic must name the dump the reader was promised");
    // The point of publishing by rename: a reader finds NOTHING, never the
    // valid prefix of a dump whose writer died.
    struct stat published;
    assert(stat(path, &published) == 0 && S_ISDIR(published.st_mode) &&
           "a failed publish must leave the destination exactly as it was");
    assert(access(staging, F_OK) != 0 &&
           "and must not leave its staging file behind either");
    assert(rmdir(path) == 0);
}

// The registration table's first growth is a realloc of COV_INITIAL_CAPACITY
// entries; filling it exactly is what puts the NEXT registration on the
// allocation the injector fails. Must match coverage_runtime.c.
#define COV_INITIAL_CAPACITY 256

static void child_loses_a_line_to_memory(const char *path) {
    assert(setenv("OSPREY_COVERAGE", path, 1) == 0);
    static const int64_t counter = 1;
    for (int64_t line = 1; line <= COV_INITIAL_CAPACITY; line += 1) {
        osp_cov_register_line(line, &counter);
    }
    // Exactly here: the table is full, so the next registration must grow it.
    osp_alloc_fail_next();
    osp_cov_register_line(COV_INITIAL_CAPACITY + 1, &counter);
    osp_alloc_fail_off();
}

// The same loss on the very FIRST line, where there is no table at all. The
// flush used to return early on an empty table and skip the refusal entirely,
// so this run explained itself with an out-of-memory line and nothing else
// [TESTING-COVERAGE-RUNTIME].
static void child_loses_its_first_line_to_memory(const char *path) {
    assert(setenv("OSPREY_COVERAGE", path, 1) == 0);
    static const int64_t counter = 1;
    osp_alloc_fail_next(); // the table's first allocation
    osp_cov_register_line(1, &counter);
    osp_alloc_fail_off();
}

// A table that LOST a line can only produce a dump that under-reports while
// looking perfectly well-formed — a lower percentage no reader can tell from
// honestly-uncovered code. It must be diagnosed and it must not be published
// at all [TESTING-COVERAGE-RUNTIME].
static void test_incomplete_table_is_never_published(const char *path) {
    char diag[COV_DIAG_MAX];
    (void)remove(path);
    diagnostic_from_child(child_loses_a_line_to_memory, path, diag,
                          sizeof(diag));
    assert(strstr(diag, "osprey coverage: out of memory") != NULL &&
           "a line the table could not hold must be reported when it is lost");
    assert(strstr(diag, "line table incomplete") != NULL &&
           "and reported again when the dump is refused");
    assert(strstr(diag, path) != NULL &&
           "the refusal names the dump that will not appear");
    assert(access(path, F_OK) != 0 &&
           "an under-reporting dump must not be published");
}

// Losing the first line means losing the table, and a refusal that names no
// dump is a refusal no reader can act on [TESTING-COVERAGE-RUNTIME].
static void test_a_table_that_never_grew_is_refused_by_name(const char *path) {
    char diag[COV_DIAG_MAX];
    (void)remove(path);
    diagnostic_from_child(child_loses_its_first_line_to_memory, path, diag,
                          sizeof(diag));
    assert(strstr(diag, "osprey coverage: out of memory") != NULL);
    assert(strstr(diag, "line table incomplete") != NULL &&
           "an empty table is still an INCOMPLETE one, and says so");
    assert(strstr(diag, path) != NULL &&
           "the refusal names the dump that will not appear");
    assert(access(path, F_OK) != 0);
}

static void child_cannot_name_its_staging_file(const char *path) {
    assert(setenv("OSPREY_COVERAGE", path, 1) == 0);
    static const int64_t counter = 1;
    osp_cov_register_line(1, &counter);
    // The exit-time flush allocates exactly once before it opens anything: the
    // staging path it publishes from.
    osp_alloc_fail_next();
}

// A dump that cannot even name its staging file is a write failure like any
// other: diagnosed, and nothing published [TESTING-COVERAGE-DUMP].
static void test_unnameable_staging_path_is_reported(const char *path) {
    char diag[COV_DIAG_MAX];
    (void)remove(path);
    diagnostic_from_child(child_cannot_name_its_staging_file, path, diag,
                          sizeof(diag));
    assert(strstr(diag, "osprey coverage: cannot write") != NULL &&
           "a staging path that cannot be formed is a write failure");
    assert(strstr(diag, path) != NULL);
    assert(access(path, F_OK) != 0 && "and nothing is published");
}

// Test-only control over remove(). The flush unlinks its staging path before
// creating it exclusively, and the security question is not what the unlink
// does — it is what the OPEN does when the thing that was unlinked is back
// before it runs. Nothing a single-threaded test can arrange from outside
// reproduces that window; a remove that reports success and changes nothing is
// exactly the state the window leaves behind, and it is the only way to prove
// the open refuses an occupied name instead of following it.
static int cov_remove_disabled;
static int (*cov_real_remove)(const char *);

int remove(const char *path) {
    if (cov_remove_disabled) {
        return 0; // "gone", and put straight back by whoever is racing
    }
    if (cov_real_remove == NULL) {
        *(void **)&cov_real_remove = dlsym(RTLD_NEXT, "remove");
    }
    return cov_real_remove == NULL ? -1 : cov_real_remove(path);
}

#define COV_SENTINEL_TEXT "a file the coverage writer must never touch\n"
#define COV_SENTINEL_SUFFIX ".sentinel"

static void child_flushes_onto_a_replanted_symlink(const char *path) {
    assert(setenv("OSPREY_COVERAGE", path, 1) == 0);
    register_all();
    cov_remove_disabled = 1;
}

// The staging name is derived from a path the environment chose, so anyone who
// can write that directory can predict it and plant a symlink there. Opening
// it with plain "w" would follow the link and truncate its target with this
// process's privileges — turning a coverage run into an arbitrary-file
// overwrite. Exclusive creation refuses a name that already exists, whatever
// kind of thing is sitting on it [TESTING-COVERAGE-DUMP].
static void test_a_symlink_at_the_staging_path_is_not_followed(const char *path) {
    char staging[COV_PATH_MAX];
    char sentinel[COV_PATH_MAX];
    int n = snprintf(staging, sizeof(staging), "%s%s", path,
                     COV_PARTIAL_SUFFIX);
    assert(n > 0 && (size_t)n < sizeof(staging));
    n = snprintf(sentinel, sizeof(sentinel), "%s%s", path,
                 COV_SENTINEL_SUFFIX);
    assert(n > 0 && (size_t)n < sizeof(sentinel));
    (void)remove(path);
    (void)remove(staging);
    (void)remove(sentinel);

    FILE *planted = fopen(sentinel, "w");
    assert(planted != NULL);
    assert(fputs(COV_SENTINEL_TEXT, planted) >= 0);
    assert(fclose(planted) == 0);
    assert(symlink(sentinel, staging) == 0);

    char diag[COV_DIAG_MAX];
    diagnostic_from_child(child_flushes_onto_a_replanted_symlink, path, diag,
                          sizeof(diag));
    assert(strstr(diag, "osprey coverage: cannot write") != NULL &&
           "a staging name that is already taken must be refused, not opened");
    assert(strstr(diag, path) != NULL &&
           "and the refusal names the dump the reader was promised");
    assert(access(path, F_OK) != 0 &&
           "nothing is published: not the dump, and not the link either");

    struct stat link_stat;
    assert(lstat(staging, &link_stat) == 0 && S_ISLNK(link_stat.st_mode) &&
           "the planted symlink is untouched, so it was never opened");
    char kept[COV_DIAG_MAX];
    FILE *in = fopen(sentinel, "r");
    assert(in != NULL && "the symlink's target still exists");
    size_t got = fread(kept, 1, sizeof(kept), in);
    assert(feof(in) && "the target grew nothing: no header, no rows, no footer");
    assert(fclose(in) == 0);
    assert(got == strlen(COV_SENTINEL_TEXT) &&
           memcmp(kept, COV_SENTINEL_TEXT, got) == 0 &&
           "the target is byte-identical: not truncated and not written to");
    assert(remove(staging) == 0);
    assert(remove(sentinel) == 0);
}

// A crashed or killed run leaves its staging file behind, and the dump path is
// per-suite and stable, so EVERY later run for that suite would meet it. The
// flush clears the staging path before creating it exclusively: without that,
// one Ctrl-C during `osprey test --coverage` disables coverage for that suite
// permanently [TESTING-COVERAGE-DUMP].
static void test_a_leftover_staging_file_does_not_disable_coverage(
    const char *path) {
    char staging[COV_PATH_MAX];
    int n = snprintf(staging, sizeof(staging), "%s%s", path,
                     COV_PARTIAL_SUFFIX);
    assert(n > 0 && (size_t)n < sizeof(staging));
    (void)remove(path);
    FILE *corpse = fopen(staging, "w");
    assert(corpse != NULL);
    assert(fputs("the remains of a run that was killed mid-dump\n", corpse) >= 0);
    assert(fclose(corpse) == 0);

    pid_t pid = fork();
    assert(pid >= 0);
    if (pid == 0) {
        run_child(path);
    }
    int status = 0;
    assert(waitpid(pid, &status, 0) == pid);
    assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
    // Same oracle as the ordinary case: a complete dump, and no staging file.
    verify_dump(path);
}

int main(void) {
    char path[COV_PATH_MAX];
    const char *dir = getenv("TMPDIR");
    int n = snprintf(path, sizeof(path), "%s/osprey-cov-test-%ld.txt",
                     (dir == NULL || dir[0] == '\0') ? "/tmp" : dir,
                     (long)getpid());
    assert(n > 0 && (size_t)n < sizeof(path));
    test_inert_without_env(path);
    test_dump_written_on_exit(path);
    test_a_leftover_staging_file_does_not_disable_coverage(path);
    test_unwritable_dump_is_reported();
    test_unpublishable_dump_is_reported(path);
    test_incomplete_table_is_never_published(path);
    test_a_table_that_never_grew_is_refused_by_name(path);
    test_unnameable_staging_path_is_reported(path);
    test_a_symlink_at_the_staging_path_is_not_followed(path);
    printf("coverage_runtime_tests: OK\n");
    return 0;
}
