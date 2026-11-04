/* Count primes below a limit by trial division — integer % in a tight loop. */
#include <stdio.h>
#include <stdint.h>

static int has_factor(int64_t n, int64_t d) {
    if (d * d > n) {
        return 0;
    } else if (n % d == 0) {
        return 1;
    } else {
        return has_factor(n, d + 1);
    }
}

static int is_prime(int64_t n) {
    if (n < 2) {
        return 0;
    }
    return !has_factor(n, 2);
}

int main(void) {
    int64_t acc = 0;
    for (int64_t n = 2; n < 200000; n++) {
        if (is_prime(n)) {
            acc += 1;
        }
    }
    printf("%lld\n", (long long)acc);
    return 0;
}
