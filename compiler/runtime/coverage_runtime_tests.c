// Assertion-driven tests for the coverage collector [TESTING-COVERAGE-RUNTIME]
// (docs/specs/0027-TestingFramework.md). A failed assert aborts the binary.
// POSIX-only test harness (fork/waitpid); the unit under test is portable C11.
#include <assert.h>
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

void osp_cov_register_line(int64_t line, const int64_t *counter);

enum { COV_TEST_LINES = 3, COV_PATH_MAX = 256 };

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
// needs no separate line universe [TESTING-COVERAGE-DUMP].
static void verify_dump(const char *path) {
    const int64_t expected_hits[COV_TEST_LINES] = {2, 0, 1};
    FILE *in = fopen(path, "r");
    assert(in != NULL);
    char header[COV_PATH_MAX];
    assert(fgets(header, sizeof(header), in) != NULL);
    assert(strcmp(header, "# osprey-coverage v1\n") == 0);
    int64_t line = 0;
    int64_t hits = 0;
    int64_t seen = 0;
    while (fscanf(in, "%" SCNd64 " %" SCNd64, &line, &hits) == 2) {
        assert(seen < COV_TEST_LINES);
        assert(line == test_lines[seen]);
        assert(hits == expected_hits[seen]);
        seen += 1;
    }
    assert(seen == COV_TEST_LINES);
    assert(fclose(in) == 0);
    assert(remove(path) == 0);
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

int main(void) {
    char path[COV_PATH_MAX];
    const char *dir = getenv("TMPDIR");
    int n = snprintf(path, sizeof(path), "%s/osprey-cov-test-%ld.txt",
                     (dir == NULL || dir[0] == '\0') ? "/tmp" : dir,
                     (long)getpid());
    assert(n > 0 && (size_t)n < sizeof(path));
    test_inert_without_env(path);
    test_dump_written_on_exit(path);
    printf("coverage_runtime_tests: OK\n");
    return 0;
}
