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
#include <string.h>

typedef struct OspCovEntry {
    int64_t line;           /* flattened 1-based source line */
    const int64_t *counter; /* the counter global instrumented code bumps */
} OspCovEntry;

static OspCovEntry *cov_entries = NULL;
static int64_t cov_count = 0;
static int64_t cov_capacity = 0;
static const char *cov_path = NULL; /* dump destination from the environment */

/* Set when cov_reserve could not grow the table. The lines it could not record
   are absent from the dump, so the dump would UNDER-REPORT while looking
   perfectly well-formed — a lower percentage that no reader can distinguish
   from honestly-uncovered code. An incomplete table is not published at all. */
static int cov_truncated = 0;

#define COV_PARTIAL_SUFFIX ".partial"

/* The sibling path the dump is built at before it is published. Caller frees.
   NULL when the name cannot be formed, which is treated as a write failure. */
static char *cov_partial_path(void) {
    size_t len = strlen(cov_path);
    char *partial = (char *)malloc(len + sizeof(COV_PARTIAL_SUFFIX));
    if (partial == NULL) {
        return NULL;
    }
    memcpy(partial, cov_path, len);
    memcpy(partial + len, COV_PARTIAL_SUFFIX, sizeof(COV_PARTIAL_SUFFIX));
    return partial;
}

/* Write header, rows and footer into `out`. False if any write failed —
   `fprintf` returns a count nobody checks per call, so the accumulated error
   flag is what actually decides whether the bytes reached the file. */
static int cov_write_rows(FILE *out) {
    fprintf(out, "# osprey-coverage v2\n");
    for (int64_t i = 0; i < cov_count; i += 1) {
        fprintf(out, "%" PRId64 " %" PRId64 "\n", cov_entries[i].line,
                *cov_entries[i].counter);
    }
    /* The completion footer. A dump truncated after any valid prefix lacks it,
       and the reader rejects the whole file rather than believing the prefix.
       The count is what makes a LOST ROW detectable rather than invisible:
       without it, a dump missing its last thousand rows parses cleanly and
       reports a higher percentage over a smaller universe. */
    fprintf(out, "# rows %" PRId64 "\n", cov_count);
    return ferror(out) == 0;
}

/* Write the dump: header, one `<line> <hits>` row per registered line
   (including zero-hit rows, so the reader needs no other line universe), then
   the `# rows <n>` completion footer [TESTING-COVERAGE-DUMP]. Registration
   order is codegen's line order.

   Published by RENAME from a sibling temporary, so a reader opening the dump
   path sees either nothing or a complete file — never the valid prefix of one
   whose writer died or ran out of disk. */
static void osp_cov_flush(void) {
    if (cov_path == NULL) {
        return;
    }
    /* Before the empty-table check, not after it. When the table lost its VERY
       FIRST line there is no table at all, and an early return on
       `cov_entries == NULL` skipped the refusal — leaving an out-of-memory
       line on stderr, no statement that the dump was deliberately withheld, and
       a reader downstream reporting a dump that simply never arrived. */
    if (cov_truncated) {
        fprintf(stderr,
                "osprey coverage: line table incomplete; refusing to write %s\n",
                cov_path);
        return;
    }
    if (cov_entries == NULL) {
        return;
    }
    char *partial = cov_partial_path();
    /* The staging name is derived from a path the environment chose, so it is
       predictable to anyone who can write that directory. `remove` first drops
       whatever is sitting there -- including a symlink, which unlink removes
       rather than follows -- and the exclusive-create mode then refuses to open
       anything that reappears in the gap. Plain "w" would happily follow a
       planted symlink and truncate whatever it pointed at, with this process's
       privileges. */
    if (partial != NULL) {
        remove(partial);
    }
    FILE *out = partial == NULL ? NULL : fopen(partial, "wx");
    if (out == NULL) {
        fprintf(stderr, "osprey coverage: cannot write %s\n", cov_path);
        free(partial);
        return;
    }
    int wrote = cov_write_rows(out);
    if (fclose(out) != 0 || !wrote || rename(partial, cov_path) != 0) {
        fprintf(stderr, "osprey coverage: cannot finish %s\n", cov_path);
        remove(partial);
    }
    free(partial);
}

/* Grow the entry table; false (with a diagnostic) when memory runs out. The
   failure is REMEMBERED: a table that lost lines can only produce a dump that
   silently under-reports, so the flush refuses to publish one. */
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
        cov_truncated = 1;
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
