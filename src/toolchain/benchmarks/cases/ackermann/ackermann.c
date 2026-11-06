/* Ackermann–Péter function — deep, non-tail mutual self-recursion. */
#include <stdint.h>
#include <stdio.h>

static int64_t add(int64_t a, int64_t b) { return a + b; }
static int64_t sub(int64_t a, int64_t b) { return a - b; }

static int64_t ack(int64_t m, int64_t n) {
    if (m == 0) {
        return add(n, 1);
    } else if (n == 0) {
        return ack(sub(m, 1), 1);
    } else {
        return ack(sub(m, 1), ack(m, sub(n, 1)));
    }
}

int main(void) {
    printf("%lld\n", (long long)ack(3, 10));
    return 0;
}
