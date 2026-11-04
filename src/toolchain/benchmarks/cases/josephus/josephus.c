/* Josephus problem — survivor index for n people, step k=7, via the modular recurrence. */
#include <stdio.h>
#include <stdint.h>

int main(void) {
    int64_t acc = 0;
    for (int64_t i = 2; i < 10000001; i++) {
        acc = (acc + 7) % i;
    }
    printf("%lld\n", (long long)acc);
    return 0;
}
