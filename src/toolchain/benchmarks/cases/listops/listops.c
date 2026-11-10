/* List operations — build arrays, then sum + count below threshold. */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "../bench_seed.h"

#define BIG_MOD 1000000007
#define N 4000

int main(void) {
    int64_t seed = read_seed();

    int64_t acc = 0;
    for (int64_t t = 0; t < 8; t++) {
        int64_t s = seed + t * 131;
        int64_t xs[N];
        for (int64_t i = 0; i < N; i++) {
            xs[i] = hash_at(s, i) % 1000;
        }
        int64_t sum = 0, below = 0;
        for (int64_t i = 0; i < N; i++) {
            sum += xs[i];
            if (xs[i] < 500) {
                below++;
            }
        }
        acc = (acc + sum + below) % BIG_MOD;
    }

    printf("%lld\n", (long long)acc);
    return 0;
}
