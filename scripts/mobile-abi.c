#include "abi.h"
#include <assert.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static int calls;
void hostRecord(int64_t value, bool ready, const char *label) {
    assert(value == 41 && ready && strcmp(label, "boot") == 0);
    calls++;
}
bool hostNegate(bool value) { return !value; }
int main(void) {
    assert(osprey_main() == 0 && calls == 1);
    assert(osprey_big() == INT64_MAX && osprey_scale(4.0) == 6.0);
    assert(osprey_invert(false) && !osprey_invert(true));
    assert(osprey_invertViaHost(false) && !osprey_invertViaHost(true));
    assert(strcmp(osprey_stored(), "persisted 41") == 0);
    assert(strcmp(osprey_greet("Swift"), "Hello Swift") == 0);
    osprey_emit(41, true, "boot");
    assert(calls == 2 && strcmp(osprey_stored(), "persisted 41") == 0);
    puts("Mobile C ABI passed");
    return 0;
}
