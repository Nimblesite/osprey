// The TAP stream test_runtime.c writes [TESTING-TAP]: mixed, clean, filtered,
// nested and empty runs and NULL reasons, each in a forked child whose exact
// output and exit code are pinned. Part of the builtins_runtime suite; see
// builtins_tests_shared.h.
#include <stdint.h>
#include <stdlib.h>

#include "builtins_tests_shared.h"

int32_t osp_test_begin(const char *name);
void osp_test_pass(void);
void osp_test_fail(const char *reason);
void osp_test_skip(const char *reason);
void osp_test_assert(const char *label, int32_t ok, const char *expected,
                     const char *actual);
void osp_test_end(const char *name);
int32_t osp_test_finalize(void);

// --- test_runtime (TAP) ------------------------------------------------------

// Mixed run: pass, fail (with diagnostic BEFORE the result line), skip with a
// reason, and a stray assertion outside any case. Exit code must be 1.
static int child_tap_mixed(void) {
  if (osp_test_begin("one")) {
    osp_test_assert(NULL, 1, "x", "x");
    osp_test_pass();
    osp_test_end("one");
  }
  if (osp_test_begin("two")) {
    osp_test_fail("boom");
    osp_test_end("two");
  }
  if (osp_test_begin("three")) {
    osp_test_skip("later");
    osp_test_end("three");
  }
  osp_test_assert("lbl", 0, "1", "2"); // stray: outside any case
  return osp_test_finalize();
}

// All green: finalize reports 0.
static int child_tap_clean(void) {
  if (osp_test_begin("a")) {
    osp_test_assert("eq", 1, "1", "1");
    osp_test_end("a");
  }
  if (osp_test_begin("b")) {
    osp_test_end("b");
  }
  return osp_test_finalize();
}

// OSPREY_TEST_FILTER selects exactly one case; unmatched cases never run and
// never count.
static int child_tap_filter(void) {
  if (setenv("OSPREY_TEST_FILTER", "only", 1) != 0) {
    return 3;
  }
  if (osp_test_begin("other")) {
    osp_test_end("other"); // must be unreachable
  }
  if (osp_test_begin("only")) {
    osp_test_end("only");
  }
  return osp_test_finalize();
}

// A nested test() fails the ENCLOSING case loudly instead of running.
static int child_tap_nested(void) {
  if (osp_test_begin("outer")) {
    if (osp_test_begin("inner")) {
      osp_test_end("inner"); // must be unreachable
    }
    osp_test_end("outer");
  }
  return osp_test_finalize();
}

// A zero-case run still prints the plan, so a filter matching nothing shows.
static int child_tap_empty(void) { return osp_test_finalize(); }

// NULL reasons are tolerated everywhere (fail, skip).
static int child_tap_null_reasons(void) {
  if (osp_test_begin("s")) {
    osp_test_skip(NULL);
    osp_test_end("s");
  }
  if (osp_test_begin("f")) {
    osp_test_fail(NULL);
    osp_test_end("f");
  }
  return osp_test_finalize();
}

void t_tap(void) {
  run_child_expect(child_tap_mixed, "",
                   "ok 1 - one\n"
                   "# fail: boom\n"
                   "not ok 2 - two\n"
                   "ok 3 - three # SKIP later\n"
                   "# check 'lbl' failed: expected 1, got 2\n"
                   "1..3\n"
                   "# tests=3 passed=1 failed=1 skipped=1\n",
                   1);
  run_child_expect(child_tap_clean, "",
                   "ok 1 - a\nok 2 - b\n1..2\n"
                   "# tests=2 passed=2 failed=0 skipped=0\n",
                   0);
  run_child_expect(child_tap_filter, "",
                   "ok 1 - only\n1..1\n"
                   "# tests=1 passed=1 failed=0 skipped=0\n",
                   0);
  run_child_expect(child_tap_nested, "",
                   "# nested test 'inner' skipped: test() inside a test body "
                   "is not supported\n"
                   "not ok 1 - outer\n1..1\n"
                   "# tests=1 passed=0 failed=1 skipped=0\n",
                   1);
  run_child_expect(child_tap_empty, "",
                   "1..0\n# tests=0 passed=0 failed=0 skipped=0\n", 0);
  // A reasonless skip ends at the bare directive — no trailing space, so the
  // line is byte-comparable against a golden [TESTING-TAP].
  run_child_expect(child_tap_null_reasons, "",
                   "ok 1 - s # SKIP\n"
                   "# fail: \n"
                   "not ok 2 - f\n1..2\n"
                   "# tests=2 passed=0 failed=1 skipped=1\n",
                   1);
}
