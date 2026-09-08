// Host side of the shared mobile C boundary. Linked against the archive the
// compiler emits and executed on the iOS simulator, an iPhone-linked binary and
// an Android device. Implements [IOS-HOST-ABI] [IOS-TARGET-ENTRY]
// [ANDROID-HOST-ABI] [MOBILE-VERIFICATION].
//
// These assertions are the only place the ABI is checked by EXECUTION rather
// than by inspection. A generated header that compiles and IR that assembles
// still say nothing about whether a bool arrives canonical, an int keeps its
// sign at 64 bits, a double survives the register class, or a UTF-8 string
// crosses without truncation. Each block below fails loudly on exactly one of
// those properties.
#include "abi.h"
#include <assert.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// A UTF-8 string with one-, two-, three-byte code points. `length` is byte
// length for strings by specification [BUILTIN-STRING-LENGTH], so both
// inspections must agree on 15 and neither may stop at the first non-ASCII byte.
static const char UTF8[] = "h\xc3\xa9llo\xe2\x86\x92\xe4\xb8\x96\xe7\x95\x8c";
#define UTF8_BYTES 15

static int records;
static int measures;

void hostRecord(int64_t value, bool ready, const char *label) {
    assert(value == 41 && ready && strcmp(label, "boot") == 0);
    records++;
}

bool hostNegate(bool value) { return !value; }

// Receives a string Osprey allocated, not a literal: proves an Osprey heap
// string reaches C NUL-terminated and intact.
int64_t hostMeasure(const char *text) {
    measures++;
    assert(text != NULL);
    return (int64_t)strlen(text);
}

// `volatile` defeats constant folding: a bool the compiler can see through
// proves nothing about how a bool is passed in a register.
static volatile bool yes = true;
static volatile bool no = false;

static void check_initialization(void) {
    assert(osprey_main() == 0);
    assert(records == 1);
    // Repeated initialization returns the cached status without rerunning
    // top-level work. [IOS-TARGET-ENTRY]
    assert(osprey_main() == 0);
    assert(records == 1);
    // A global computed during initialization stays live for later calls.
    assert(strcmp(osprey_stored(), "persisted 41") == 0);
}

static void check_integers(void) {
    assert(osprey_big() == INT64_MAX);
    assert(osprey_small() == INT64_MIN);
    assert(osprey_negate(1) == -1);
    assert(osprey_negate(-7) == 7);
    assert(osprey_negate(0) == 0);
    // Checked arithmetic still yields its `?:` default across the boundary
    // rather than wrapping or trapping.
    assert(osprey_overflowDefault() == -1);
}

static void check_doubles(void) {
    assert(osprey_scale(4.0) == 6.0);
    assert(osprey_scale(-2.0) == -3.0);
    assert(osprey_scale(0.0) == 0.0);
    // Halving is exact in binary floating point, so this is an equality test
    // on the value, not on a rounding mode.
    assert(osprey_half(0.1) == 0.05);
    assert(osprey_half(-3.5) == -1.75);
}

static void check_booleans(void) {
    assert(osprey_invert(no) && !osprey_invert(yes));
    assert(osprey_invertViaHost(no) && !osprey_invertViaHost(yes));
    assert(osprey_both(yes, yes));
    assert(!osprey_both(yes, no) && !osprey_both(no, yes) && !osprey_both(no, no));
}

static void check_strings(void) {
    assert(strcmp(osprey_greet("Swift"), "Hello Swift") == 0);
    assert(strcmp(osprey_greet(""), "Hello ") == 0);

    // UTF-8 crosses intact in both directions and is measured in bytes.
    assert(osprey_bytes(UTF8) == UTF8_BYTES);
    assert(osprey_spanOf(UTF8) == UTF8_BYTES);
    assert(osprey_bytes("") == 0 && osprey_spanOf("") == 0);
    const char *joined = osprey_joinWith(UTF8, "");
    assert(strlen(joined) == UTF8_BYTES + 1);
    assert(memcmp(joined, UTF8, UTF8_BYTES) == 0 && joined[UTF8_BYTES] == '|');

    // A long borrowed input: nothing here caps at a small buffer.
    char *wide = malloc(4097);
    assert(wide != NULL);
    memset(wide, 'a', 4096);
    wide[4096] = 0;
    assert(osprey_bytes(wide) == 4096);
    free(wide);

    // Osprey borrows a host string for the duration of the call and must copy
    // anything it keeps. Scribbling over the caller's buffer afterwards must
    // not disturb a string Osprey already returned. [IOS-HOST-ABI]
    char *borrowed = malloc(8);
    assert(borrowed != NULL);
    memcpy(borrowed, "abcdefg", 8);
    const char *shouted = osprey_upper(borrowed);
    assert(shouted != borrowed);
    memset(borrowed, 'Z', 7);
    free(borrowed);
    assert(strcmp(shouted, "ABCDEFG") == 0);

    // An Osprey-allocated string passed to a host import.
    assert(osprey_measured("abc") == 3 && measures == 1);
    assert(osprey_measured("") == 0 && measures == 2);
}

static void check_unit_export(void) {
    osprey_emit(41, yes, "boot");
    assert(records == 2);
    // Application state is unchanged by a call that returns nothing.
    assert(strcmp(osprey_stored(), "persisted 41") == 0);
}

int main(void) {
    check_initialization();
    check_integers();
    check_doubles();
    check_booleans();
    check_strings();
    check_unit_export();
    puts("Mobile C ABI passed");
    return 0;
}
