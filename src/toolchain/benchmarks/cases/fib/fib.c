/* Naive recursive Fibonacci — exercises raw function-call + recursion overhead. */
#include <stdio.h>
#include <stdint.h>

static int64_t add(int64_t a, int64_t b) { return a + b; }
static int64_t sub(int64_t a, int64_t b) { return a - b; }

static int64_t fib(int64_t n) {
    switch (n) {
        case 0: return 0;
        case 1: return 1;
        default: return add(fib(sub(n, 1)), fib(sub(n, 2)));
    }
}

int main(void) {
    printf("%lld\n", (long long)fib(35));
    return 0;
}
