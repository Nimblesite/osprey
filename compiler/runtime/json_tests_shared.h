// Shared between the two translation units of the json suite: the behaviour
// half (json_runtime_tests.c) and the grammar half (json_grammar_tests.c).
//
// They are one BINARY -- coverage of json_runtime.c is measured across the
// whole suite -- but two objects, because the allocator interposer the
// behaviour half needs may be defined in exactly one of them. The assertion
// counter is declared here rather than duplicated: main prints one total, and
// two copies would print the wrong one.
#ifndef OSPREY_JSON_TESTS_SHARED_H
#define OSPREY_JSON_TESTS_SHARED_H

#include <assert.h>
#include <stdint.h>

int64_t json_parse(char *s);
char *json_get(int64_t handle, char *path);
int64_t json_length(int64_t handle, char *path);
int64_t json_free(int64_t handle);

extern long g_checks;
#define CHECK(c)                                                               \
  do {                                                                         \
    g_checks++;                                                                \
    assert(c);                                                                 \
  } while (0)

#define ERR_NULL ((int64_t)-1)
#define ERR_MALFORMED ((int64_t)-2)
#define ERR_TRAILING ((int64_t)-3)
#define ERR_TABLE_FULL ((int64_t)-4)

// json_get result must equal `want`, exactly, and is caller-freed.
void expect_get(int64_t h, const char *path, const char *want);
int64_t parse_ok(const char *src);

// The grammar half: everything the string and number productions accept, and
// everything they must refuse [BUILTIN-JSON-STRING] [BUILTIN-JSON-NUMBER].
void t_string_grammar_rejects_everything_outside_it(void);
void t_number_grammar(void);

#endif // OSPREY_JSON_TESTS_SHARED_H
