/* Mergesort — naive top-down merge sort: deal the run into two fresh halves
   by alternating elements, sort both, merge. Checksum is rank-weighted. */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "../bench_seed.h"

#define BIG_MOD 1000000007
#define N 2000
#define SPREAD 100000

static void merge_sort(int64_t *xs, int64_t n) {
    if (n < 2) {
        return;
    }
    int64_t ne = (n + 1) / 2, no = n / 2;
    int64_t *evens = alloc_run(ne);
    int64_t *odds = alloc_run(no);
    for (int64_t i = 0; i < n; i++) {
        if (i % 2 == 0) {
            evens[i / 2] = xs[i];
        } else {
            odds[i / 2] = xs[i];
        }
    }
    merge_sort(evens, ne);
    merge_sort(odds, no);
    int64_t i = 0, j = 0, k = 0;
    while (i < ne && j < no) {
        if (odds[j] < evens[i]) {
            xs[k++] = odds[j++];
        } else {
            xs[k++] = evens[i++];
        }
    }
    while (i < ne) {
        xs[k++] = evens[i++];
    }
    while (j < no) {
        xs[k++] = odds[j++];
    }
    free(evens);
    free(odds);
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
        merge_sort(xs, N);
        int64_t weight = 0;
        for (int64_t i = 0; i < N; i++) {
            weight = (weight + xs[i] * (i + 1)) % BIG_MOD;
        }
        acc = (acc + weight) % BIG_MOD;
    }

    printf("%lld\n", (long long)acc);
    return 0;
}
