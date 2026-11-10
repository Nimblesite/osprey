/* Expression tree — build a heap-allocated tree, then evaluate it. */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "../bench_seed.h"

#define BIG_MOD 1000000007

#define TAG_LIT 0
#define TAG_ADD 1
#define TAG_MUL 2

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
