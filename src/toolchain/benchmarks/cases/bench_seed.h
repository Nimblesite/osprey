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

#endif
