/* Triple-nested counting loop accumulating (i*j*k) mod P — nested iteration. */
#include <stdio.h>
#include <stdint.h>

static const int64_t P = 1000000007;

static int64_t loop_k(int64_t i, int64_t j, int64_t k, int64_t acc) {
    while (k != 0) {
        acc = (acc + i * j * k) % P;
        k -= 1;
    }
    return acc;
}

static int64_t loop_j(int64_t i, int64_t j, int64_t n, int64_t acc) {
    while (j != 0) {
        acc = loop_k(i, j, n, acc);
        j -= 1;
    }
    return acc;
}

static int64_t loop_i(int64_t i, int64_t n, int64_t acc) {
    while (i != 0) {
        acc = loop_j(i, n, n, acc);
        i -= 1;
    }
    return acc;
}

int main(void) {
    printf("%lld\n", (long long)loop_i(250, 250, 0));
    return 0;
}
