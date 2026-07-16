// Osprey line-coverage collection. Implements [TESTING-COVERAGE-RUNTIME]
// (docs/specs/0027-TestingFramework.md). Called only from compiler-emitted IR
// (crates/osprey-codegen/src/coverage.rs); dependency-free C11 so the unit
// compiles unchanged into the native, GC, and wasm runtime archives.
//
// Codegen emits one i64 hit counter global per coverable source line, bumps it
// inline where control flow reaches (no call per hit), and registers each
// counter once at program start. This unit only records the table and dumps it
// at exit. Inert unless OSPREY_COVERAGE=<path> names the dump file
// [TESTING-COVERAGE-ENV]. State is plain globals: registration happens once on
// the main fiber before any user code runs.

#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct OspCovEntry {
    int64_t line;           /* flattened 1-based source line */
    const int64_t *counter; /* the counter global instrumented code bumps */
} OspCovEntry;

static OspCovEntry *cov_entries = NULL;
static int64_t cov_count = 0;
static int64_t cov_capacity = 0;
static const char *cov_path = NULL; /* dump destination from the environment */

/* Write the dump: header, then one `<line> <hits>` row per registered line
   (including zero-hit rows, so the reader needs no other line universe)
   [TESTING-COVERAGE-DUMP]. Registration order is codegen's line order. */
static void osp_cov_flush(void) {
    if (cov_path == NULL || cov_entries == NULL) {
        return;
    }
    FILE *out = fopen(cov_path, "w");
    if (out == NULL) {
        fprintf(stderr, "osprey coverage: cannot write %s\n", cov_path);
        return;
    }
    fprintf(out, "# osprey-coverage v1\n");
    for (int64_t i = 0; i < cov_count; i += 1) {
        fprintf(out, "%" PRId64 " %" PRId64 "\n", cov_entries[i].line,
                *cov_entries[i].counter);
    }
    if (fclose(out) != 0) {
        fprintf(stderr, "osprey coverage: cannot finish %s\n", cov_path);
    }
}

/* Grow the entry table; false (with a diagnostic) when memory runs out —
   coverage then under-reports rather than aborting the program. */
static int cov_reserve(void) {
    if (cov_count < cov_capacity) {
        return 1;
    }
    enum { COV_INITIAL_CAPACITY = 256 };
    int64_t next = cov_capacity == 0 ? COV_INITIAL_CAPACITY : cov_capacity * 2;
    OspCovEntry *grown =
        realloc(cov_entries, (size_t)next * sizeof(OspCovEntry));
    if (grown == NULL) {
        fprintf(stderr, "osprey coverage: out of memory\n");
        return 0;
    }
    cov_entries = grown;
    cov_capacity = next;
    return 1;
}

/* Register one coverable line and its counter global; arms the exit-time dump
   on the first call when OSPREY_COVERAGE is set. Emitted by codegen at the top
   of main, once per line, before any user code runs. */
void osp_cov_register_line(int64_t line, const int64_t *counter) {
    if (cov_path == NULL) {
        const char *path = getenv("OSPREY_COVERAGE");
        if (path == NULL || path[0] == '\0') {
            return;
        }
        if (atexit(osp_cov_flush) != 0) {
            fprintf(stderr, "osprey coverage: cannot arm exit dump\n");
            return;
        }
        cov_path = path;
    }
    if (cov_reserve() != 0) {
        cov_entries[cov_count] = (OspCovEntry){line, counter};
        cov_count += 1;
    }
}
