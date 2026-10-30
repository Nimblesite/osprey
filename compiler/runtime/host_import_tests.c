// Managed engine controls for [IOS-HOST-IMPORTS]; Apple archive ABI is tested separately.
#include "memory_hooks.h"
#include "string_runtime.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef HOST_IMPORT_ARC
size_t osp_arc_live_objects(void);
size_t osp_arc_live_bytes(void);
static size_t baseline_objects;
static size_t baseline_bytes;
#else
void osp_collect(void);
size_t osp_gc_live_objects(void);
size_t osp_gc_collections(void);
#endif

static char *saved;
enum { PAYLOAD_BYTES = 8192, SOAK_ROUNDS = 20000 };

static void payload(char *text, int revision) {
    memset(text, 'x', PAYLOAD_BYTES);
    int prefix = snprintf(text, PAYLOAD_BYTES, "%d \xc3\xa9\xe2\x86\x92", revision);
    assert(prefix > 0 && prefix < PAYLOAD_BYTES);
    text[prefix] = 'x';
    text[PAYLOAD_BYTES] = '\0';
}

static void check_copy(int revision) {
    char text[PAYLOAD_BYTES + 1];
    char expected[PAYLOAD_BYTES + 1];
    payload(text, revision);
    memcpy(expected, text, sizeof text);
    char *copy = osp_host_string_copy(text);
    assert(copy != NULL && copy != text);
    memset(text, 'Z', PAYLOAD_BYTES);
    assert(memcmp(copy, expected, sizeof text) == 0);
    assert(strcmp(saved, "retained \xc3\xa9\xe2\x86\x92") == 0);
    osp_release(copy);
}

static void check_scalar_inputs(void) {
    assert(osp_host_string_copy(NULL) == NULL);
    char empty[] = "";
    char *copy = osp_host_string_copy(empty);
    assert(copy != NULL && copy != empty && copy[0] == '\0');
    osp_release(copy);
}

static void checkpoint(int revision) {
#ifdef HOST_IMPORT_ARC
    (void)revision;
    assert(osp_arc_live_objects() == baseline_objects);
    assert(osp_arc_live_bytes() == baseline_bytes);
#else
    if (revision % 256 == 0) { osp_collect(); assert(osp_gc_live_objects() <= 16); }
#endif
}

static void check_soak(void) {
    for (int revision = 0; revision < SOAK_ROUNDS; revision++) {
        check_copy(revision);
        checkpoint(revision);
    }
#ifndef HOST_IMPORT_ARC
    assert(osp_gc_collections() > 0);
#endif
}

static void retained_seed(void) {
    saved = osp_host_string_copy("retained \xc3\xa9\xe2\x86\x92");
    assert(saved != NULL);
#ifdef HOST_IMPORT_ARC
    baseline_objects = osp_arc_live_objects();
    baseline_bytes = osp_arc_live_bytes();
    assert(baseline_objects == 1 && baseline_bytes > 0);
#endif
}

int main(void) {
    assert(setenv("OSPREY_ARC_DEBUG", "1", 1) == 0);
    osp_mem_boot();
    check_scalar_inputs();
    retained_seed();
    check_soak();
    osp_release(saved);
    saved = NULL;
#ifdef HOST_IMPORT_ARC
    assert(osp_arc_live_objects() == 0 && osp_arc_live_bytes() == 0);
#endif
    puts("[ok] managed host imports: 20000 distinct 8192-byte copies");
    return 0;
}
