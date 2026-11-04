/* Count coprime pairs (i,j), 1<=i,j<=N — nested iteration + Euclidean gcd. */
#include <stdint.h>
#include <stdio.h>

static int64_t gcd(int64_t a, int64_t b) {
    return b == 0 ? a : gcd(b, a % b);
}

int main(void) {
    int64_t n = 2000;
    int64_t acc = 0;
    for (int64_t i = n; i > 0; i--) {
        for (int64_t j = n; j > 0; j--) {
            if (gcd(i, j) == 1) {
                acc++;
            }
        }
    }
    printf("%lld\n", (long long)acc);
    return 0;
}
