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
static int64_t tests_skipped = 0;   /* cases reported Skip [TESTING-VERDICT] */
static int64_t stray_failures = 0;  /* failing asserts outside any case */
static int64_t case_failures = -1;  /* -1 = not inside a test case */
static int64_t case_skipped = 0;    /* current case reported Skip */
/* Why the current case was skipped. COPIED, never aliased: the caller's
   string is an Osprey-owned value whose region may end before the verdict is
   printed, so holding the pointer would read freed memory under a reclaiming
   backend ([MEM-BACKENDS], plan 0011 M5b). Truncation is harmless — the reason
   is diagnostic text. */
#define SKIP_REASON_MAX 256
static char skip_reason[SKIP_REASON_MAX] = "";

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
    case_skipped = 0;
    skip_reason[0] = '\0';
    return 1;
}

/* Record a Verdict value from a pure ML-flavor case [TESTING-VERDICT].
   Pass is a no-op; Fail bumps the case/stray failure count with its reason;
   Skip marks the case skipped so osp_test_end emits a TAP SKIP directive. */
void osp_test_pass(void) {}

void osp_test_fail(const char *reason) {
    printf("# fail: %s\n", reason == NULL ? "" : reason);
    if (case_failures >= 0) {
        case_failures += 1;
    } else {
        stray_failures += 1;
    }
}

void osp_test_skip(const char *reason) {
    case_skipped = 1;
    if (reason == NULL) {
        skip_reason[0] = '\0';
        return;
    }
    (void)snprintf(skip_reason, sizeof(skip_reason), "%s", reason);
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

/* Close the current case and print its TAP result line [TESTING-TAP]. A Skip
   reported by the case wins over a pass and emits a SKIP directive; a failing
   assertion still fails the case regardless [TESTING-VERDICT]. */
void osp_test_end(const char *name) {
    tests_run += 1;
    if (case_failures > 0) {
        tests_failed += 1;
        printf("not ok %lld - %s\n", (long long)tests_run, name);
    } else if (case_skipped != 0) {
        tests_skipped += 1;
        printf("ok %lld - %s # SKIP %s\n", (long long)tests_run, name,
               skip_reason);
    } else {
        printf("ok %lld - %s\n", (long long)tests_run, name);
    }
    case_failures = -1;
    case_skipped = 0;
}

/* Print the plan + summary — always, even for a zero-case run (`1..0`), so a
   filter that matched nothing is visible [TESTING-TAP]. The process exit code
   [TESTING-EXIT]. */
int32_t osp_test_finalize(void) {
    printf("1..%lld\n", (long long)tests_run);
    printf("# tests=%lld passed=%lld failed=%lld skipped=%lld\n",
           (long long)tests_run,
           (long long)(tests_run - tests_failed - tests_skipped),
           (long long)tests_failed, (long long)tests_skipped);
    return (tests_failed > 0 || stray_failures > 0) ? 1 : 0;
}
