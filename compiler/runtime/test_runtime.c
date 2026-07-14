// Osprey testing framework run state + TAP emission. Implements
// [TESTING-RUNTIME], [TESTING-TAP], [TESTING-EXIT], [TESTING-FILTER]
// (docs/specs/0027-TestingFramework.md). Called only from compiler-emitted IR
// (crates/osprey-codegen/src/testing.rs); dependency-free C11 so the unit
// compiles unchanged into the native, GC, and wasm runtime archives.
//
// State is plain (non-atomic) globals: test execution is single-fiber by
// contract [TESTING-RISK-FIBERS].

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int64_t tests_run = 0;
static int64_t tests_failed = 0;
static int64_t stray_failures = 0; /* failing asserts outside any case */
static int64_t case_failures = -1; /* -1 = not inside a test case */

/* Begin the named case: 0 = skip, 1 = run. Skips on a filter mismatch
   [TESTING-FILTER]; a nested test() inside a running case does not run — it
   fails the enclosing case loudly instead [TESTING-BUILTIN-TEST]. */
int32_t osp_test_begin(const char *name) {
    if (case_failures >= 0) {
        printf("# nested test '%s' skipped: test() inside a test body is not "
               "supported\n",
               name == NULL ? "" : name);
        case_failures += 1;
        return 0;
    }
    const char *filter = getenv("OSPREY_TEST_FILTER");
    if (filter != NULL && filter[0] != '\0' &&
        (name == NULL || strcmp(filter, name) != 0)) {
        return 0;
    }
    case_failures = 0;
    return 1;
}

/* Record one assertion; on mismatch print the `#` diagnostic [TESTING-TAP].
   `label` is NULL for expect, the check label otherwise. */
void osp_test_assert(const char *label, int32_t ok, const char *expected,
                     const char *actual) {
    if (ok != 0) {
        return;
    }
    if (label != NULL) {
        printf("# check '%s' failed: expected %s, got %s\n", label, expected,
               actual);
    } else {
        printf("# expect failed: expected %s, got %s\n", expected, actual);
    }
    if (case_failures >= 0) {
        case_failures += 1;
    } else {
        stray_failures += 1;
    }
}

/* Close the current case and print its TAP result line [TESTING-TAP]. */
void osp_test_end(const char *name) {
    tests_run += 1;
    if (case_failures > 0) {
        tests_failed += 1;
        printf("not ok %lld - %s\n", (long long)tests_run, name);
    } else {
        printf("ok %lld - %s\n", (long long)tests_run, name);
    }
    case_failures = -1;
}

/* Print the plan + summary — always, even for a zero-case run (`1..0`), so a
   filter that matched nothing is visible [TESTING-TAP]. The process exit code
   [TESTING-EXIT]. */
int32_t osp_test_finalize(void) {
    printf("1..%lld\n", (long long)tests_run);
    printf("# tests=%lld passed=%lld failed=%lld\n", (long long)tests_run,
           (long long)(tests_run - tests_failed), (long long)tests_failed);
    return (tests_failed > 0 || stray_failures > 0) ? 1 : 0;
}
