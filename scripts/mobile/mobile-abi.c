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
//
// The ownership blocks execute an ARC archive; hosted targets reject GC.
// Native GC string-copy hooks have a separate runtime oracle in host_import_tests.c.
// This suite does not establish GC archive or device-platform acceptance.
#include "abi.h"
#include <assert.h>
#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Published by the C runtime once OSPREY_ARC_DEBUG arms the counters.
extern size_t osp_arc_live_bytes(void);
extern size_t osp_arc_live_objects(void);

// A UTF-8 string with one-, two-, three-byte code points. `length` is byte
// length for strings by specification [BUILTIN-STRING-LENGTH], so both
// inspections must agree on 15 and neither may stop at the first non-ASCII byte.
static const char UTF8[] = "h\xc3\xa9llo\xe2\x86\x92\xe4\xb8\x96\xe7\x95\x8c";
#define UTF8_BYTES 15

// The first imports as Osprey's global nested record joined them.
static const char SNAPSHOT_JOINED[] =
    "retained \xe2\x86\x92 seed||first \xc3\xa9|second";

static int records;
static int measures;
static char imported_buffer[8193];

// Fill exactly 8192 UTF-8 bytes with a version prefix, 3-byte arrows and ASCII
// padding. Distinct versions differ in the prefix; every call is deterministic.
static size_t version_prefix(int64_t version, char *out) {
    char digits[32];
    size_t count = 0;
    uint64_t value = (uint64_t)version;
    do { digits[count++] = (char)('0' + value % 10); value /= 10; } while (value);
    out[0] = 'v';
    for (size_t i = 0; i < count; i++) out[i + 1] = digits[count - i - 1];
    out[count + 1] = ':';
    return count + 2;
}

static void fill_long_text(int64_t version, char *out) {
    size_t at = version_prefix(version, out);
    while (at + 3 <= 8192) {
        memcpy(out + at, "\xe2\x86\x92", 3);
        at += 3;
    }
    while (at < 8192) out[at++] = 'x';
    out[8192] = '\0';
}

// One bounded host buffer: each later callback is allowed to overwrite it
// after Osprey has copied the previous result. [IOS-HOST-IMPORTS]
const char *hostBorrowedText(int64_t version) {
    if (version >= 1000) {
        fill_long_text(version, imported_buffer);
        return imported_buffer;
    }
    const char *text = version == 0 ? "retained \xe2\x86\x92 seed" :
                       version == 1 ? "first \xc3\xa9" :
                       version == 2 ? "second" :
                       version == -1 ? "" : NULL;
    if (text == NULL) {
        snprintf(imported_buffer, sizeof imported_buffer, "v%lld", (long long)version);
        return imported_buffer;
    }
    memcpy(imported_buffer, text, strlen(text) + 1);
    return imported_buffer;
}

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

// A handle the host owns. Osprey stores it, returns it and passes it back to a
// host import without looking inside. [FFI-PTR]
static int handle_target = 7;

int64_t hostResolve(void *window) {
    return window == &handle_target ? *(int *)window : -1;
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
    const char *stored = osprey_stored();
    assert(strcmp(stored, "persisted 41") == 0);
    osprey_release_string(stored);
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

static void check_greetings(void) {
    const char *greeting = osprey_greet("Swift");
    assert(strcmp(greeting, "Hello Swift") == 0);
    osprey_release_string(greeting);
    const char *empty = osprey_greet("");
    assert(strcmp(empty, "Hello ") == 0);
    osprey_release_string(empty);
}

static void check_utf8(void) {
    // UTF-8 crosses intact in both directions and is measured in bytes.
    assert(osprey_bytes(UTF8) == UTF8_BYTES);
    assert(osprey_spanOf(UTF8) == UTF8_BYTES);
    assert(osprey_bytes("") == 0 && osprey_spanOf("") == 0);
    const char *joined = osprey_joinWith(UTF8, "");
    assert(strlen(joined) == UTF8_BYTES + 1);
    assert(memcmp(joined, UTF8, UTF8_BYTES) == 0 && joined[UTF8_BYTES] == '|');
    osprey_release_string(joined);
}

static void check_wide_input(void) {
    // A long borrowed input: nothing here caps at a small buffer.
    char *wide = malloc(4097);
    assert(wide != NULL);
    memset(wide, 'a', 4096);
    wide[4096] = 0;
    assert(osprey_bytes(wide) == 4096);
    free(wide);
}

static void check_borrowed_copy(void) {
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
    osprey_release_string(shouted);
}

static void churn_export_strings(void) {
    for (int i = 0; i < 16; i++) {
        const char *temporary = osprey_upper("churn!");
        assert(strcmp(temporary, "CHURN!") == 0);
        osprey_release_string(temporary);
    }
}

static void mutate_host_buffers(char *ignored, char *borrowed, const char *replacement, size_t size) {
    memset(ignored, 'x', strlen(ignored));
    memcpy(borrowed, replacement, size);
}

static int retained_input_survives(int second) {
    osprey_retainHostInput("");
    size_t objects = osp_arc_live_objects(), bytes = osp_arc_live_bytes();
    char borrowed[] = "before", ignored[] = "unused";
    if (second) osprey_retainSecond(ignored, borrowed); else osprey_retainHostInput(borrowed);
    mutate_host_buffers(ignored, borrowed, "after!", sizeof borrowed);
    const char *saved = osprey_retainedInput();
    assert(osp_arc_live_objects() == objects);
    assert(osp_arc_live_bytes() == bytes + strlen("before"));
    churn_export_strings();
    int valid = saved != borrowed && strcmp(saved, "before") == 0;
    if (saved != borrowed) osprey_release_string(saved);
    osprey_retainHostInput("");
    assert(osp_arc_live_objects() == objects && osp_arc_live_bytes() == bytes);
    return valid;
}

static int identity_input_survives(int second) {
    size_t objects = osp_arc_live_objects(), bytes = osp_arc_live_bytes();
    char borrowed[] = "stable", ignored[] = "unused";
    const char *saved = second ? osprey_identitySecond(ignored, borrowed) : osprey_identityInput(borrowed);
    mutate_host_buffers(ignored, borrowed, "broken", sizeof borrowed);
    assert(osp_arc_live_objects() == objects + 1);
    assert(osp_arc_live_bytes() == bytes + sizeof borrowed);
    churn_export_strings();
    int valid = saved != borrowed && strcmp(saved, "stable") == 0;
    if (saved != borrowed) osprey_release_string(saved);
    assert(osp_arc_live_objects() == objects && osp_arc_live_bytes() == bytes);
    return valid;
}

static void check_export_input_ownership(void) {
    for (int second = 0; second < 2; second++) {
        int retained = retained_input_survives(second);
        int identity = identity_input_survives(second);
        assert(retained && "retained host input aliases caller buffer");
        assert(identity && "returned host input aliases caller buffer");
    }
}

static void check_measured(void) {
    // An Osprey-allocated string passed to a host import.
    assert(osprey_measured("abc") == 3 && measures == 1);
    assert(osprey_measured("") == 0 && measures == 2);
}

static void check_pointers(void) {
    assert(osprey_passBack(&handle_target) == &handle_target);
    assert(osprey_passBack(NULL) == NULL);
    assert(osprey_resolve(&handle_target) == 7);
}

static void check_import_ownership(void) {
    const char *saved = osprey_retainedImport();
    assert(saved != imported_buffer);
    assert(strcmp(saved, "retained \xe2\x86\x92 seed") == 0);
    osprey_release_string(saved);
    const char *two = osprey_readTwoImports();
    assert(two != imported_buffer);
    assert(strcmp(two, "first \xc3\xa9|second") == 0);
    osprey_release_string(two);
    memset(imported_buffer, 'Z', sizeof(imported_buffer) - 1);
    imported_buffer[sizeof(imported_buffer) - 1] = 0;
    const char *again = osprey_retainedImport();
    assert(again != imported_buffer);
    assert(strcmp(again, "retained \xe2\x86\x92 seed") == 0);
    osprey_release_string(again);
}

static void check_import_payloads(void) {
    const char *empty = osprey_changingImport(-1);
    assert(strcmp(empty, "") == 0);
    assert(empty != imported_buffer);
    osprey_release_string(empty);
    const char *unicode = osprey_changingImport(1);
    assert(strcmp(unicode, "first \xc3\xa9") == 0);
    assert(unicode != imported_buffer);
    osprey_release_string(unicode);
    const char *longCopy = osprey_changingImport(1000);
    char expected[8193];
    fill_long_text(1000, expected);
    assert(longCopy != imported_buffer);
    assert(memcmp(longCopy, expected, 8193) == 0);
    osprey_release_string(longCopy);
}

static void check_import_reference(void) {
    const char *text = osprey_importByReference();
    assert(text != imported_buffer);
    memset(imported_buffer, 'Z', sizeof imported_buffer - 1);
    assert(strcmp(text, "first \xc3\xa9") == 0);
    osprey_release_string(text);
}

static void check_retained_join(void) {
    const char *joined = osprey_retainedNestedJoin();
    assert(strcmp(joined, SNAPSHOT_JOINED) == 0);
    osprey_release_string(joined);
    const char *list = osprey_retainedListJoin();
    assert(strcmp(list, SNAPSHOT_JOINED) == 0);
    osprey_release_string(list);
}

static void check_retained_long(void) {
    char expected[8193];
    fill_long_text(1000, expected);
    const char *saved = osprey_retainedLongImport();
    assert(memcmp(saved, expected, sizeof expected) == 0);
    osprey_release_string(saved);
}

static void check_retained_fields(void) {
    const char *seedText = osprey_retainedSeedImport();
    assert(strcmp(seedText, "retained \xe2\x86\x92 seed") == 0);
    osprey_release_string(seedText);

    const char *blank = osprey_retainedBlankImport();
    assert(strcmp(blank, "") == 0);
    osprey_release_string(blank);

    const char *first = osprey_retainedFirstImport();
    assert(strcmp(first, "first \xc3\xa9") == 0);
    osprey_release_string(first);

    const char *second = osprey_retainedSecondImport();
    assert(strcmp(second, "second") == 0);
    osprey_release_string(second);
}

static void check_unit_export(void) {
    osprey_emit(41, yes, "boot");
    assert(records == 2);
    // Application state is unchanged by a call that returns nothing.
    const char *stored = osprey_stored();
    assert(strcmp(stored, "persisted 41") == 0);
    osprey_release_string(stored);
}

static void soak_step(int64_t version) {
    char expected[8193];
    fill_long_text(version, expected);
    const char *result = osprey_changingImport(version);
    assert(result != imported_buffer);
    assert(memcmp(result, expected, 8193) == 0);
    memset(imported_buffer, 0x5a, sizeof imported_buffer);
    assert(memcmp(result, expected, 8193) == 0);
    osprey_release_string(result);
    check_retained_join();
    check_export_input_ownership();
}

static void check_arc_steady_state(void) {
    check_retained_long();
    for (int i = 0; i < 16; i++) soak_step(1000 + i);
    size_t bytes = osp_arc_live_bytes();
    size_t objects = osp_arc_live_objects();
    assert(bytes > 0 && objects > 0);
    for (int i = 0; i < 20000; i++) soak_step(1000 + i);
    assert(osp_arc_live_bytes() == bytes);
    assert(osp_arc_live_objects() == objects);
    check_retained_long();
}

static void check_scalar_boundary(void) {
    check_initialization();
    check_integers();
    check_doubles();
    check_booleans();
    check_greetings();
    check_utf8();
    check_wide_input();
    check_borrowed_copy();
    check_export_input_ownership();
    check_measured();
    check_pointers();
}

static void check_ownership_regression(void) {
    check_import_ownership();
    check_import_payloads();
    check_import_reference();
    check_retained_join();
    check_retained_fields();
    check_unit_export();
    check_arc_steady_state();
}

int main(void) {
    int armed = setenv("OSPREY_ARC_DEBUG", "1", 1);
    assert(armed == 0);
    assert(osprey_host_abi_version() == 1);
    check_scalar_boundary();
    check_ownership_regression();
    puts("Mobile C ABI passed");
    return 0;
}
