#include <stdio.h>
#include <stdint.h>

static int64_t sub(int64_t a, int64_t b) {
    return a - b;
}

static int64_t tak(int64_t x, int64_t y, int64_t z) {
    if (x > y) {
        return tak(
            tak(sub(x, 1), y, z),
            tak(sub(y, 1), z, x),
            tak(sub(z, 1), x, y));
    }
    return z;
}

int main(void) {
    printf("%lld\n", (long long)tak(32, 16, 8));
    return 0;
}
