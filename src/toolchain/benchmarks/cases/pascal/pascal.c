/* Binomial coefficient via naive (un-memoised) Pascal recursion C(n,k)=C(n-1,k-1)+C(n-1,k). */
#include <stdio.h>
#include <stdint.h>

static int64_t binom(int64_t n, int64_t k) {
    if (k == 0) {
        return 1;
    } else if (k == n) {
        return 1;
    } else {
        return binom(n - 1, k - 1) + binom(n - 1, k);
    }
}

int main(void) {
    printf("%lld\n", (long long)binom(27, 13));
    return 0;
}
