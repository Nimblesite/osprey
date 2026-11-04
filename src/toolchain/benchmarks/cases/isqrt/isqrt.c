/* Sum of integer square roots over 1..N — Newton's method, integer division heavy. */
#include <stdio.h>
#include <stdint.h>

static int64_t isqrt(int64_t n) {
    if (n < 2) return n;
    int64_t x = n;
    for (;;) {
        int64_t y = (x + n / x) / 2;
        if (y < x) x = y; else return x;
    }
}

int main(void) {
    int64_t acc = 0;
    for (int64_t i = 1; i < 1000001; i++) acc += isqrt(i);
    printf("%lld\n", (long long)acc);
    return 0;
}
