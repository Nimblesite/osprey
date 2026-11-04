/* Sum of modular exponentiation: sum of i^20 mod P for i in 1..N, naive repeated multiply. */
#include <stdio.h>
#include <stdint.h>

static const int64_t P = 1000000007;

static int64_t powmod(int64_t base, int64_t e, int64_t acc) {
    if (e == 0) {
        return acc;
    }
    return powmod(base, e - 1, (acc * base) % P);
}

int main(void) {
    int64_t acc = 0;
    for (int64_t i = 1; i < 1000000; i++) {
        acc = (acc + powmod(i, 20, 1)) % P;
    }
    printf("%lld\n", (long long)acc);
    return 0;
}
