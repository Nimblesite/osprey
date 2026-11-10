// Shared by the translation units of the string_runtime suite: the main file
// (string_runtime_tests.c) and its parse, edge, cursor and float halves. One
// binary, so coverage of string_runtime.c is measured across all of them.
#ifndef OSPREY_STRING_TESTS_SHARED_H
#define OSPREY_STRING_TESTS_SHARED_H

#include <assert.h>
#include <ctype.h>
#include <errno.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "collection_runtime.h"
#include "float_spelling.h"
#include "string_runtime.h"
#include "memory_hooks.h"

// Releases runtime-produced text through its manager and fixture text through libc.
void release_test_buffer(void *p);

// string_parse_tests.c
void test_parse_int(void);
void test_parse_float(void);
void test_split(void);
void test_lines(void);
void test_words(void);
void test_join(void);
void test_join_runtime_list(void);
void test_parse_int_signs(void);
void test_parse_int_long_prefixes(void);
void test_parse_int_byte_mutations(void);

// string_edge_tests.c
void test_long_strings(void);
void test_round_trips(void);
void test_unicode_bytes(void);
void test_repeated_replace(void);
void test_split_pathological(void);
void test_pad_overflow_guards(void);

// string_cursor_tests.c
void test_cursor_byte_length(void);
void test_cursor_byte_at(void);
void test_cursor_codepoint_at(void);
void test_cursor_codepoint_width(void);
void test_cursor_from_codepoint(void);
void test_cursor_error_messages(void);
void test_cursor_roundtrip_exhaustive(void);

// string_float_tests.c
void test_float_to_string(void);
void test_float16_conversion(void);
void test_float16_spelling_roundtrip(void);
void test_float32_spelling_roundtrip(void);
void test_sized_float_strings(void);
void test_float_to_string_matches_reference(void);

#endif
