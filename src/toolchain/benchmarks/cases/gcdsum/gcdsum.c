/* Sum of gcd(i, K) for i in 1..N — Euclidean-algorithm recursion (modulo heavy). */
#include <stdio.h>
#include <stdint.h>

static int64_t gcd(int64_t a, int64_t b) {
    return b == 0 ? a : gcd(b, a % b);
}

int main(void) {
    int64_t acc = 0;
    for (int64_t i = 1; i < 2000000; i++) {
        acc += gcd(i, 1234567);
    }
    printf("%lld\n", (long long)acc);
    return 0;
}
