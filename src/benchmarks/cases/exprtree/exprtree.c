/* Expression tree — build a heap-allocated tree, then evaluate it. */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define MINSTD 16807
#define MODULUS 2147483647
#define BIG_MOD 1000000007

#define TAG_LIT 0
#define TAG_ADD 1
#define TAG_MUL 2

static int64_t r1(int64_t x) { return (x * MINSTD) % MODULUS; }

static int64_t hash_at(int64_t s, int64_t i) { return r1(r1(s + i)); }

static int64_t read_seed(void) {
    char line[256];
    int64_t m = 0;
    if (fgets(line, sizeof line, stdin) != NULL) {
        char *end = NULL;
        long long parsed = strtoll(line, &end, 10);
        if (end != line) {
            m = (int64_t)parsed;
        }
    }
    if (m == 0) {
        return 1;
    }
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

struct Expr {
    int tag;
    int64_t v;
    struct Expr *l, *r;
};

static struct Expr *build(int64_t s, int64_t idx, int64_t depth) {
    struct Expr *e = malloc(sizeof *e);
    if (e == NULL) {
        return NULL;
    }
    if (depth == 0) {
        e->tag = TAG_LIT;
        e->v = hash_at(s, idx) % 1000;
        e->l = NULL;
        e->r = NULL;
    } else {
        int64_t op = hash_at(s, idx) % 2;
        e->v = 0;
        e->l = build(s, idx * 2 + 1, depth - 1);
        e->r = build(s, idx * 2 + 2, depth - 1);
        e->tag = op == 0 ? TAG_ADD : TAG_MUL;
    }
    return e;
}

static int64_t eval(const struct Expr *e) {
    switch (e->tag) {
        case TAG_LIT:
            return e->v;
        case TAG_ADD:
            return (eval(e->l) + eval(e->r)) % BIG_MOD;
        default:
            return (eval(e->l) * eval(e->r)) % BIG_MOD;
    }
}

int main(void) {
    int64_t seed = read_seed();

    int64_t acc = 0;
    for (int64_t t = 0; t < 10; t++) {
        acc = (acc + eval(build(seed + t * 7, 1, 14))) % BIG_MOD;
    }

    printf("%lld\n", (long long)acc);
    return 0;
}
