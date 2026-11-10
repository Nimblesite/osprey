/* Quicksort — naive first-element-pivot quicksort: split the tail into two
   fresh runs around the pivot, sort both, join. Checksum is rank-weighted. */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "../bench_seed.h"

#define BIG_MOD 1000000007
#define N 2000
#define SPREAD 100000

static void quicksort(int64_t *xs, int64_t n) {
    if (n < 2) {
        return;
    }
    int64_t pivot = xs[0];
    int64_t *below = alloc_run(n);
    int64_t *at_least = alloc_run(n);
    int64_t nb = 0, na = 0;
    for (int64_t i = 1; i < n; i++) {
        if (xs[i] < pivot) {
            below[nb++] = xs[i];
        } else {
            at_least[na++] = xs[i];
        }
    }
    quicksort(below, nb);
    quicksort(at_least, na);
    memcpy(xs, below, (size_t)nb * sizeof *xs);
    xs[nb] = pivot;
    memcpy(xs + nb + 1, at_least, (size_t)na * sizeof *xs);
    free(below);
    free(at_least);
}

int main(void) {
    int64_t seed = read_seed();

    int64_t acc = 0;
    for (int64_t t = 0; t < 8; t++) {
        int64_t s = seed + t * 131;
        int64_t xs[N];
        for (int64_t i = 0; i < N; i++) {
            xs[i] = hash_at(s, i) % SPREAD;
        }
        quicksort(xs, N);
        int64_t weight = 0;
        for (int64_t i = 0; i < N; i++) {
            weight = (weight + xs[i] * (i + 1)) % BIG_MOD;
        }
        acc = (acc + weight) % BIG_MOD;
    }

    printf("%lld\n", (long long)acc);
    return 0;
}
