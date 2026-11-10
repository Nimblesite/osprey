/* Word-frequency count over a tiny vocab — exercises hashing + counting. */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "../bench_seed.h"


int main(void) {
    const char *vocab[8] = {"the", "quick", "brown", "fox",
                            "jumps", "over", "lazy", "dog"};
    (void)vocab;
    int64_t seed = read_seed();

    int64_t counts[8] = {0};
    for (int64_t i = 0; i < 200000; i++) {
        counts[hash_at(seed, i) % 8] += 1;
    }

    int64_t result = 0;
    for (int64_t k = 0; k < 8; k++) {
        result += (k + 1) * counts[k];
    }

    printf("%lld\n", (long long)result);
    return 0;
}
