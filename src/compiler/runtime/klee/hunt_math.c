// libm for the mock libc [SYMEX-MOCK-STATELESS]. KLEE concretises floating
// point and lowers llvm.sqrt / llvm.exp / llvm.log to these names, so they are
// plain C: bit-level exponent handling plus short series on reduced
// arguments. Native tests pin special cases and sampled finite results;
// no universal ulp bound is established. Native replay uses the real libm.
#include "hunt_mock.h"

#include <math.h>

#define LN2 0.69314718055994530942
#define PI 3.14159265358979323846
#define SERIES_TERMS 24

typedef union {
  double value;
  uint64_t bits;
} bits64;

enum { EXPONENT_BIAS = 1023, EXPONENT_SHIFT = 52 };

double fabs(double x) { return __builtin_fabs(x); }

double trunc(double x) {
  if (!isfinite(x) || fabs(x) >= 4503599627370496.0) {
    return x;
  }
  return (double)(long long)x;
}

double floor(double x) {
  double whole = trunc(x);
  return whole > x ? whole - 1.0 : whole;
}

double ceil(double x) {
  double whole = trunc(x);
  return whole < x ? whole + 1.0 : whole;
}

double round(double x) { return x < 0.0 ? -floor(-x + 0.5) : floor(x + 0.5); }

double fmod(double x, double y) {
  if (y == 0.0 || !isfinite(x) || isnan(y)) {
    return NAN;
  }
  return x - trunc(x / y) * y;
}

double frexp(double x, int *exponent) {
  osp_hunt_note(OSP_HUNT_CALL_MATH);
  if (x == 0.0 || !isfinite(x)) {
    *exponent = 0;
    return x;
  }
  bits64 b = {x};
  int raw = (int)((b.bits >> EXPONENT_SHIFT) & 0x7ff);
  if (raw == 0) {
    double scaled = frexp(x * 18446744073709551616.0, exponent);
    *exponent -= 64;
    return scaled;
  }
  *exponent = raw - EXPONENT_BIAS + 1;
  b.bits = (b.bits & ~(0x7ffULL << EXPONENT_SHIFT)) | ((uint64_t)(EXPONENT_BIAS - 1) << EXPONENT_SHIFT);
  return b.value;
}

static double power_of_two(int exponent) {
  if (exponent > 1023) {
    return INFINITY;
  }
  if (exponent < -1022) {
    return exponent < -1074 ? 0.0 : power_of_two(exponent + 60) / 1152921504606846976.0;
  }
  bits64 b;
  b.bits = (uint64_t)(exponent + EXPONENT_BIAS) << EXPONENT_SHIFT;
  return b.value;
}

double ldexp(double x, int exponent) {
  while (exponent > 1000) {
    x *= power_of_two(1000);
    exponent -= 1000;
  }
  while (exponent < -1000) {
    x *= power_of_two(-1000);
    exponent += 1000;
  }
  return x * power_of_two(exponent);
}

double sqrt(double x) {
  osp_hunt_note(OSP_HUNT_CALL_MATH);
  if (x < 0.0 || isnan(x)) {
    return NAN;
  }
  if (x == 0.0 || isinf(x)) {
    return x;
  }
  int exponent;
  double mantissa = frexp(x, &exponent);
  if (exponent % 2 != 0) {
    mantissa *= 2.0;
    exponent--;
  }
  double guess = (1.0 + mantissa) / 2.0;
  for (int i = 0; i < 8; i++) {
    guess = 0.5 * (guess + mantissa / guess);
  }
  return ldexp(guess, exponent / 2);
}

// e^r for |r| <= ln2 / 2 by the Taylor series.
static double exp_reduced(double r) {
  double term = 1.0;
  double sum = 1.0;
  for (int i = 1; i < SERIES_TERMS; i++) {
    term *= r / i;
    sum += term;
  }
  return sum;
}

double exp(double x) {
  osp_hunt_note(OSP_HUNT_CALL_MATH);
  if (isnan(x)) {
    return x;
  }
  if (x > 709.8) {
    return INFINITY;
  }
  if (x < -745.2) {
    return 0.0;
  }
  double k = round(x / LN2);
  double r = x - k * LN2;
  return ldexp(exp_reduced(r), (int)k);
}

// ln m for m in [0.5, 1) through 2 atanh((m - 1) / (m + 1)).
static double log_reduced(double m) {
  double z = (m - 1.0) / (m + 1.0);
  double z2 = z * z;
  double term = z;
  double sum = 0.0;
  for (int i = 1; i < 2 * SERIES_TERMS; i += 2) {
    sum += term / i;
    term *= z2;
  }
  return 2.0 * sum;
}

double log(double x) {
  osp_hunt_note(OSP_HUNT_CALL_MATH);
  if (isnan(x) || x < 0.0) {
    return NAN;
  }
  if (x == 0.0) {
    return -INFINITY;
  }
  if (isinf(x)) {
    return x;
  }
  int exponent;
  double mantissa = frexp(x, &exponent);
  if (mantissa < 0.70710678118654752440) {
    mantissa *= 2.0;
    exponent--;
  }
  return log_reduced(mantissa) + exponent * LN2;
}

double log10(double x) { return log(x) / 2.30258509299404568402; }

double log2(double x) { return log(x) / LN2; }

static double integer_power(double base, long long exponent) {
  double result = 1.0;
  base = exponent < 0 ? 1.0 / base : base;
  unsigned long long remaining = exponent < 0 ? 0ULL - (unsigned long long)exponent : (unsigned long long)exponent;
  for (; remaining != 0; remaining >>= 1) {
    if (remaining & 1) {
      result *= base;
    }
    base *= base;
  }
  return result;
}

static double infinite_power(double magnitude, double exponent) {
  return magnitude == 1.0 ? 1.0 : (magnitude > 1.0) == (exponent > 0.0) ? INFINITY : 0.0;
}

static double limiting_power(double base, double exponent, int odd) {
  double result = (base == 0.0) == (exponent > 0.0) ? 0.0 : INFINITY;
  return signbit(base) && odd ? -result : result;
}

double pow(double base, double exponent) {
  osp_hunt_note(OSP_HUNT_CALL_MATH);
  if (exponent == 0.0 || base == 1.0) return 1.0;
  if (isnan(base) || isnan(exponent)) return NAN;
  if (isinf(exponent)) return infinite_power(fabs(base), exponent);
  int integral = exponent == trunc(exponent);
  int small = fabs(exponent) < 0x1p53;
  int odd = integral && small && ((long long)exponent & 1);
  if (base == 0.0 || isinf(base)) return limiting_power(base, exponent, odd);
  if (base < 0.0 && !integral) return NAN;
  if (integral && small) return integer_power(base, (long long)exponent);
  return exp(exponent * log(fabs(base)));
}

// sin r for |r| <= pi by the Taylor series.
static double sin_reduced(double r) {
  double term = r;
  double sum = r;
  double r2 = r * r;
  for (int i = 1; i < SERIES_TERMS; i++) {
    term *= -r2 / ((2 * i) * (2 * i + 1));
    sum += term;
  }
  return sum;
}

static double reduce_angle(double x) {
  double turns = round(x / (2.0 * PI));
  return x - turns * 2.0 * PI;
}

double sin(double x) {
  osp_hunt_note(OSP_HUNT_CALL_MATH);
  if (!isfinite(x)) {
    return NAN;
  }
  return sin_reduced(reduce_angle(x));
}

double cos(double x) { return sin(x + PI / 2.0); }

double tan(double x) { return sin(x) / cos(x); }

double tanh(double x) {
  osp_hunt_note(OSP_HUNT_CALL_MATH);
  if (isnan(x)) {
    return x;
  }
  if (fabs(x) > 20.0) {
    return x > 0.0 ? 1.0 : -1.0;
  }
  double e = exp(2.0 * x);
  return (e - 1.0) / (e + 1.0);
}

// KLEE optimises the module before it lowers llvm.floor and friends to these
// names; without a live reference the definitions would be dropped first and
// the lowered calls left undefined.
__attribute__((used)) static double (*const libm_roots[])(double) = {
    fabs, trunc, floor, ceil, round, sqrt, exp, log, log10, log2, sin, cos, tan, tanh,
};
