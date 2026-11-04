/* Sum of decimal digit-sums over 1..N — integer division (n/10) and modulo in recursion. */
#include <stdio.h>
#include <stdint.h>

static int64_t digsum(int64_t n) {
    return n < 10 ? n : (n % 10) + digsum(n / 10);
}

int main(void) {
    int64_t acc = 0;
    for (int64_t i = 1; i < 2000001; i++) acc += digsum(i);
    printf("%lld\n", (long long)acc);
    return 0;
}
