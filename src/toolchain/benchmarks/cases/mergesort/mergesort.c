/* Mergesort — naive top-down merge sort: deal the run into two fresh halves
   by alternating elements, sort both, merge. Checksum is rank-weighted. */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "../bench_seed.h"


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
    printf("%lld\n", (long long)sorted_checksum(read_seed(), merge_sort));
    return 0;
}
