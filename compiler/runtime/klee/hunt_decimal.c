// Exact decimal expansion of a double for the mock printf
// [SYMEX-MOCK-STATELESS]. A double is m * 2^e with integer m, so its exact
// digits are the integer m * 2^e (e >= 0) or m * 5^-e with -e fraction digits.
// Both fit a small base-1e9 bignum; nothing here approximates, so `%.17g`
// prints what the host libc prints and the native replay sees the same text.
#include "hunt_mock.h"

#include <stdint.h>
#include <string.h>

enum { LIMB_BASE = 1000000000, LIMBS_MAX = 96, DOUBLE_BIAS = 1075 };

typedef struct {
  uint32_t limb[LIMBS_MAX]; // least significant first
  int used;
} bignum;

static void multiply_small(bignum *n, uint32_t factor) {
  uint64_t carry = 0;
  for (int i = 0; i < n->used; i++) {
    uint64_t product = (uint64_t)n->limb[i] * factor + carry;
    n->limb[i] = (uint32_t)(product % LIMB_BASE);
    carry = product / LIMB_BASE;
  }
  while (carry != 0 && n->used < LIMBS_MAX) {
    n->limb[n->used++] = (uint32_t)(carry % LIMB_BASE);
    carry /= LIMB_BASE;
  }
}

static void from_u64(bignum *n, uint64_t value) {
  n->used = 0;
  do {
    n->limb[n->used++] = (uint32_t)(value % LIMB_BASE);
    value /= LIMB_BASE;
  } while (value != 0);
}

// Multiplies by base^count, `chunk` powers at a time (a chunk fits a limb).
static void scale(bignum *n, uint32_t base, int count, int chunk) {
  uint32_t chunk_factor = 1;
  for (int i = 0; i < chunk; i++) {
    chunk_factor *= base;
  }
  for (; count >= chunk; count -= chunk) {
    multiply_small(n, chunk_factor);
  }
  for (; count > 0; count--) {
    multiply_small(n, base);
  }
}

static int to_digits(const bignum *n, char *out) {
  int length = 0;
  for (int i = n->used - 1; i >= 0; i--) {
    char chunk[9];
    uint32_t value = n->limb[i];
    for (int d = 8; d >= 0; d--) {
      chunk[d] = (char)('0' + value % 10);
      value /= 10;
    }
    int start = 0;
    while (length == 0 && start < 8 && chunk[start] == '0') {
      start++;
    }
    memcpy(out + length, chunk + start, (size_t)(9 - start));
    length += 9 - start;
  }
  return length;
}

void osp_hunt_decimal(double magnitude, struct osp_hunt_decimal *out) {
  union {
    double value;
    uint64_t bits;
  } raw = {magnitude};
  int field = (int)((raw.bits >> 52) & 0x7ff);
  uint64_t mantissa = raw.bits & ((1ULL << 52) - 1);
  if (field != 0) {
    mantissa |= 1ULL << 52;
  }
  out->count = 0;
  out->point = 0;
  if (mantissa == 0) {
    return;
  }
  int exponent = (field == 0 ? 1 : field) - DOUBLE_BIAS;
  bignum n;
  from_u64(&n, mantissa);
  if (exponent >= 0) {
    scale(&n, 2, exponent, 29);
  } else {
    scale(&n, 5, -exponent, 13);
  }
  out->count = to_digits(&n, out->digits);
  out->point = out->count - (exponent < 0 ? -exponent : 0);
}

static int any_nonzero_from(const struct osp_hunt_decimal *d, int index) {
  for (int i = index; i < d->count; i++) {
    if (d->digits[i] != '0') {
      return 1;
    }
  }
  return 0;
}

// Rounds up at `keep`, carrying a new leading 1 (and the point) when needed.
static void carry_up(struct osp_hunt_decimal *d, int keep) {
  int i = keep - 1;
  while (i >= 0 && d->digits[i] == '9') {
    d->digits[i--] = '0';
  }
  if (i >= 0) {
    d->digits[i]++;
    return;
  }
  memmove(d->digits + 1, d->digits, (size_t)keep);
  d->digits[0] = '1';
  d->count = keep + 1;
  d->point++;
}

void osp_hunt_decimal_round(struct osp_hunt_decimal *d, int keep) {
  if (keep >= d->count) {
    return;
  }
  if (keep < 0) {
    d->count = 0;
    return;
  }
  char next = d->digits[keep];
  int odd = keep > 0 && ((d->digits[keep - 1] - '0') & 1);
  int up = next > '5' || (next == '5' && (any_nonzero_from(d, keep + 1) || odd));
  d->count = keep;
  if (up) {
    carry_up(d, keep);
  }
}
