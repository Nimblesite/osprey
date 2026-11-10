/* Text statistics over a tiny vocab — exercises strlen/strchr per word. */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "../bench_seed.h"


int main(void) {
    const char *vocab[8] = {"the", "quick", "brown", "fox",
                            "jumps", "over", "lazy", "dog"};
    int64_t seed = read_seed();

    int64_t acc = 0;
    for (int64_t i = 0; i < 200000; i++) {
        const char *w = vocab[hash_at(seed, i) % 8];
        acc += (int64_t)strlen(w)
             + (strchr(w, 'o') != NULL ? 7 : 0)
             + (w[0] == 't' ? 3 : 0);
    }

    printf("%lld\n", (long long)acc);
    return 0;
}
