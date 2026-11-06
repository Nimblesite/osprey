/* Allocate + traverse many short-lived binary trees — allocation/memory stress.
 * Faithful port of binarytrees.osp: build 1200 trees of depth 13, sum checks. */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>

typedef struct Tree {
    struct Tree *left;
    struct Tree *right;
} Tree;

static Tree *make(int64_t d) {
    Tree *t = (Tree *)malloc(sizeof(Tree));
    if (d == 0) {
        t->left = NULL;
        t->right = NULL;
    } else {
        t->left = make(d - 1);
        t->right = make(d - 1);
    }
    return t;
}

static int64_t check(const Tree *t) {
    if (t == NULL) {
        return 0;
    }
    return 1 + check(t->left) + check(t->right);
}

static void freetree(Tree *t) {
    if (t == NULL) {
        return;
    }
    freetree(t->left);
    freetree(t->right);
    free(t);
}

int main(void) {
    int64_t acc = 0;
    for (int64_t i = 0; i < 1200; i++) {
        Tree *t = make(13);
        acc += check(t);
        freetree(t);
    }
    printf("%lld\n", (long long)acc);
    return 0;
}
