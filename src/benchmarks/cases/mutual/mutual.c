#include <stdint.h>
#include <stdio.h>

static int is_odd(int64_t n);

static int is_even(int64_t n) {
    if (n == 0) {
        return 1;
    }
    return is_odd(n - 1);
}

static int is_odd(int64_t n) {
    if (n == 0) {
        return 0;
    }
    return is_even(n - 1);
}

int main(void) {
    int64_t acc = 0;
    for (int64_t i = 1; i < 130000; i++) {
        if (is_even(i % 1000)) {
            acc += 1;
        }
    }
    printf("%lld\n", (long long)acc);
    return 0;
}
