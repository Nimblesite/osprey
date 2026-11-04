/* Collatz (3n+1) stopping time summed over 1..N — integer division (n/2) in deep recursion. */
#include <stdio.h>
#include <stdint.h>

static int64_t collatz(int64_t n) {
    if (n == 1) return 0;
    return (n % 2 == 0) ? 1 + collatz(n / 2) : 1 + collatz(3 * n + 1);
}

int main(void) {
    int64_t acc = 0;
    for (int64_t i = 1; i < 100001; i++) acc += collatz(i);
    printf("%lld\n", (long long)acc);
    return 0;
}
