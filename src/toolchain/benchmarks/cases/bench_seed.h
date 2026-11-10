/* The seed and hash every C benchmark shares with its other-language twins:
   a MINSTD step, a doubly hashed index, and a seed read from stdin (0 asks for
   a random one, as in every language's version of the case). */
#ifndef BENCH_SEED_H
#define BENCH_SEED_H

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#define MINSTD 16807
#define MODULUS 2147483647

static inline int64_t r1(int64_t x) { return (x * MINSTD) % MODULUS; }

static inline int64_t hash_at(int64_t s, int64_t i) { return r1(r1(s + i)); }

static inline int64_t random_seed(void) {
    uint64_t val = 0;
    FILE *f = fopen("/dev/urandom", "rb");
    if (f != NULL) {
        if (fread(&val, sizeof val, 1, f) != 1) {
            val = 0;
        }
        fclose(f);
    }
    return (int64_t)(val % 2147483646) + 1;
}

static inline int64_t read_seed(void) {
    char line[256];
    int64_t m = 0;
    if (fgets(line, sizeof line, stdin) != NULL) {
        char *end = NULL;
        long long parsed = strtoll(line, &end, 10);
        if (end != line) {
            m = (int64_t)parsed;
        }
    }
    return m == 0 ? 1 : random_seed();
}

static inline int64_t *alloc_run(int64_t count) {
    int64_t *run = malloc((size_t)(count > 0 ? count : 1) * sizeof *run);
    if (run == NULL) {
        exit(1);
    }
    return run;
}

/* The sort cases' shared run: eight seeded runs of 2000 values below 100000,
   each sorted in place by `sort` and folded into one rank-weighted checksum. */
static inline int64_t sorted_checksum(int64_t seed, void (*sort)(int64_t *, int64_t)) {
    enum { RUN = 2000 };
    const int64_t big_mod = 1000000007;
    int64_t acc = 0;
    for (int64_t t = 0; t < 8; t++) {
        int64_t xs[RUN];
        for (int64_t i = 0; i < RUN; i++) {
            xs[i] = hash_at(seed + t * 131, i) % 100000;
        }
        sort(xs, RUN);
        int64_t weight = 0;
        for (int64_t i = 0; i < RUN; i++) {
            weight = (weight + xs[i] * (i + 1)) % big_mod;
        }
        acc = (acc + weight) % big_mod;
    }
    return acc;
}

#endif
