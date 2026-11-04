#include <stdio.h>
#include <stdint.h>

static int64_t coin(int64_t k) {
    switch (k) {
        case 1: return 1;
        case 2: return 5;
        case 3: return 10;
        case 4: return 25;
        default: return 50;
    }
}

static int64_t ways(int64_t amount, int64_t kind) {
    if (amount == 0) return 1;
    if (amount < 0) return 0;
    if (kind == 0) return 0;
    return ways(amount - coin(kind), kind) + ways(amount, kind - 1);
}

int main(void) {
    printf("%lld\n", (long long)ways(600, 5));
    return 0;
}
