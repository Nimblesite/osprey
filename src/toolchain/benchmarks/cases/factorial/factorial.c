/* Factorial-style product 1*2*...*N taken mod 1000000007 (matches factorial.osp). */
#include <stdio.h>
#include <stdint.h>

#define MOD 1000000007LL

int main(void) {
    int64_t acc = 1;
    for (int64_t i = 1; i <= 10000000; i++) {
        acc = (acc * i) % MOD;
    }
    printf("%lld\n", (long long)acc);
    return 0;
}
