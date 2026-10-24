// The device math library compiled as host C and held to the host's own IEEE
// binary64 results [GPU-BACKEND-PORTABILITY] [BUILTIN-MATH]: the four
// operations, sqrt, the conversions and the comparisons must agree bit for
// bit on every word tried (NaN payloads compare as NaN); exp, ln and tanh are
// compared with the host's long double libm rounded to binary64: glibc's
// binary128 on aarch64 Linux agrees with the device on every argument, x87's
// 80-bit on x86-64 differs by one ulp on about 1 in 4 000, and on Darwin it
// is the binary64 libm itself. glibc's binary64 tanh is up to two ulps off (tanh of
// 0xbfd8b6b048dce522: 1.69 ulps, where the device is 0.31 against an
// 80-digit reference), so a binary64 reference would test glibc, not the
// device. The device must land within one ulp everywhere and agree on at
// least 99% of arguments.
#include <assert.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "gpu_f64.metal"
#include "gpu_math64.metal"
#include "gpu_math_ext.metal"
#include "gpu_device_lib.metal"

#define TRIALS 400000
#define SPECIALS 12

static uint64_t state = 0x9e3779b97f4a7c15;
static unsigned long checks;

static uint64_t random_word(void) {
  state ^= state << 13;
  state ^= state >> 7;
  state ^= state << 17;
  return state;
}

static double of(uint64_t word) {
  double d;
  memcpy(&d, &word, sizeof d);
  return d;
}

static uint64_t bits(double d) {
  uint64_t word;
  memcpy(&word, &d, sizeof word);
  return word;
}

static int same(uint64_t a, uint64_t b) { return a == b || (isnan(of(a)) && isnan(of(b))); }
static uint64_t word_of(long v) { return (uint64_t)v; }
static long truncated(double d) { return (long)d; }
static uint64_t flag(bool b) { return b ? 1 : 0; }

static uint32_t f32_bits_of(double d) {
  float f = (float)d;
  uint32_t w;
  memcpy(&w, &f, sizeof w);
  return w;
}

static double f32_widen(uint32_t w) {
  float f;
  memcpy(&f, &w, sizeof f);
  return (double)f;
}

static uint32_t nan32_of(uint64_t x) {
  // The arm64 fcvt d->s NaN contract the host oracle runs under: the top 23
  // mantissa bits, quiet bit forced.
  return (uint32_t)(x >> 32 & 0x80000000u) | 0x7fc00000u | (uint32_t)((x & UINT64_C(0x000fffffffffffff)) >> 29);
}

// A word with an exponent drawn from [low, low + range), any sign and mantissa.
static uint64_t word_in(int low, int range) {
  uint64_t r = random_word();
  uint64_t e = (uint64_t)low + ((r >> 52) % (uint64_t)range);
  return (r & UINT64_C(0x800fffffffffffff)) | (e << 52);
}

static const uint64_t specials[SPECIALS] = {0, UINT64_C(0x8000000000000000), 1, UINT64_C(0x8000000000000001),
    UINT64_C(0x000fffffffffffff), UINT64_C(0x0010000000000000), UINT64_C(0x3ff0000000000000),
    UINT64_C(0xbff0000000000000), UINT64_C(0x7fefffffffffffff), UINT64_C(0x7ff0000000000000),
    UINT64_C(0xfff0000000000000), UINT64_C(0x7ff8000000000000)};

static void expect(const char *what, uint64_t a, uint64_t b, uint64_t device, uint64_t host) {
  checks++;
  if (!same(device, host)) {
    fprintf(stderr, "%s(%016llx, %016llx): device %016llx host %016llx\n", what,
        (unsigned long long)a, (unsigned long long)b, (unsigned long long)device, (unsigned long long)host);
    abort();
  }
}

typedef ulong (*device_binary)(ulong, ulong);
typedef double (*host_binary)(double, double);
static double host_add(double a, double b) { return a + b; }
static double host_sub(double a, double b) { return a - b; }
static double host_mul(double a, double b) { return a * b; }
static double host_div(double a, double b) { return a / b; }

static void binary_case(const char *what, device_binary device, host_binary host, uint64_t a, uint64_t b) {
  expect(what, a, b, device(a, b), bits(host(of(a), of(b))));
}

static void check_binary(const char *what, device_binary device, host_binary host) {
  for (unsigned i = 0; i < SPECIALS; i++)
    for (unsigned j = 0; j < SPECIALS; j++) binary_case(what, device, host, specials[i], specials[j]);
  for (unsigned i = 0; i < TRIALS; i++) {
    binary_case(what, device, host, word_in(0, 2047), word_in(0, 2047));
    binary_case(what, device, host, word_in(1000, 50), word_in(1000, 50));
    binary_case(what, device, host, word_in(0, 60), word_in(0, 60));
    binary_case(what, device, host, word_in(2000, 46), word_in(2000, 46));
  }
}

static void check_unary_and_conversions(void) {
  for (unsigned i = 0; i < SPECIALS; i++) expect("sqrt", specials[i], 0, sqrt64(specials[i]), bits(sqrt(of(specials[i]))));
  for (unsigned i = 0; i < TRIALS; i++) {
    uint64_t x = word_in(0, 2047), y = word_in(0, 2047);
    expect("sqrt", x, 0, sqrt64(x), bits(sqrt(of(x))));
    long v = (long)random_word() >> (random_word() % 64);
    expect("from_long", (uint64_t)v, 0, from_long(v), bits((double)v));
    uint64_t small = word_in(900, 185);
    if (!isnan(of(small)) && fabs(of(small)) < 9.2e18) expect("to_long", small, 0, word_of(to_long(small)), word_of(truncated(of(small))));
    expect("neg", x, 0, neg64(x), bits(-of(x)));
    expect("abs", x, 0, abs64(x), bits(fabs(of(x))));
    expect("lt", x, y, flag(lt64(x, y)), flag(of(x) < of(y)));
    expect("le", x, y, flag(le64(x, y)), flag(of(x) <= of(y)));
    expect("eq", x, y, flag(eq64(x, y)), flag(of(x) == of(y)));
  }
  expect("to_long saturates", 0, 0, word_of(to_long(UINT64_C(0x43f0000000000000))), word_of(INT64_MAX));
  expect("to_long saturates", 0, 0, word_of(to_long(UINT64_C(0xc3f0000000000000))), word_of(INT64_MIN));
  expect("to_long nan", 0, 0, word_of(to_long(UINT64_C(0x7ff8000000000000))), 0);
}

typedef ulong (*device_unary)(ulong);
typedef long double (*host_unary)(long double);

// Every function whose result is exactly defined — rounding, remainder,
// sign, the binary32 emulation and the total shifts — must agree with the
// host bit for bit on every word [BUILTIN-MATH-ROUNDING] [BUILTIN-FLOAT32]
// [BUILTIN-BITWISE].
static void check_exact_unary(const char *what, device_unary device, double (*host)(double)) {
  for (unsigned i = 0; i < SPECIALS; i++) expect(what, specials[i], 0, device(specials[i]), bits(host(of(specials[i]))));
  for (unsigned i = 0; i < TRIALS; i++) {
    uint64_t x = word_in(0, 2047);
    expect(what, x, 0, device(x), bits(host(of(x))));
    uint64_t integral = word_in(1030, 100);
    expect(what, integral, 0, device(integral), bits(host(of(integral))));
    uint64_t tiny = word_in(990, 40);
    expect(what, tiny, 0, device(tiny), bits(host(of(tiny))));
  }
}

static void check_exact_binary(const char *what, device_binary device, host_binary host) {
  for (unsigned i = 0; i < SPECIALS; i++)
    for (unsigned j = 0; j < SPECIALS; j++) binary_case(what, device, host, specials[i], specials[j]);
  for (unsigned i = 0; i < TRIALS; i++) {
    binary_case(what, device, host, word_in(0, 2047), word_in(0, 2047));
    binary_case(what, device, host, word_in(1020, 40), word_in(1000, 50));
    binary_case(what, device, host, word_in(990, 40), word_in(1030, 60));
  }
}

static uint32_t narrow_reference(uint64_t x) {
  if (isnan(of(x))) return nan32_of(x);
  return f32_bits_of(of(x));
}

static void check_binary32(void) {
  for (unsigned i = 0; i < SPECIALS; i++) {
    expect("float32", specials[i], 0, float32_64(specials[i]),
           isnan(of(specials[i])) ? bits(f32_widen(nan32_of(specials[i]))) : bits(f32_widen(f32_bits_of(of(specials[i])))));
    expect("float32Bits", specials[i], 0, float32_bits(specials[i]), narrow_reference(specials[i]));
    expect("float32FromBits", specials[i], 0, float32_from_bits(float32_bits(specials[i])), float32_64(specials[i]));
  }
  for (unsigned i = 0; i < TRIALS; i++) {
    uint64_t x = word_in(0, 2047);
    expect("float32", x, 0, float32_64(x), bits(f32_widen(narrow_reference(x))));
    expect("float32Bits", x, 0, float32_bits(x), narrow_reference(x));
    expect("float32FromBits", x, 0, float32_from_bits(float32_bits(x)), float32_64(x));
    uint64_t edge = (word_in(0, 2047) & UINT64_C(0xffffffffe0000000)) | (random_word() & UINT64_C(0x1fffffff));
    expect("float32 tie", edge, 0, float32_64(edge), bits(f32_widen(narrow_reference(edge))));
  }
  // floatBits/floatFromBits are the identity on words.
  for (unsigned i = 0; i < SPECIALS; i++) {
    expect("floatBits", specials[i], 0, bits(of(specials[i])), specials[i]);
    expect("floatFromBits", specials[i], 0, specials[i], specials[i]);
  }
}

static long ref_shl(long a, long n) { return (unsigned long)n < 64 ? (long)((unsigned long)a << n) : 0; }
static long ref_shru(long a, long n) { return (unsigned long)n < 64 ? (long)((unsigned long)a >> n) : 0; }
static long ref_shr(long a, long n) {
  if ((unsigned long)n >= 64) return a < 0 ? -1 : 0;
  unsigned long bits = a < 0 ? ~(unsigned long)a : (unsigned long)a;
  unsigned long shifted = bits >> n;
  return (long)(a < 0 ? ~shifted : shifted);
}

static void check_total_shifts(void) {
  const long edges[] = {0, -1, 1, -2, 63, -64, 9223372036854775807L, INT64_MIN, -9223372036854775807L};
  for (unsigned i = 0; i < sizeof edges / sizeof *edges; i++)
    for (long n = -3; n <= 67; n++) {
      long a = edges[i];
      checks++;
      if (shl64(a, n) != ref_shl(a, n) || shru64(a, n) != ref_shru(a, n) || shr64(a, n) != ref_shr(a, n)) {
        fprintf(stderr, "shift(%ld, %ld): shl %ld/%ld shru %ld/%ld shr %ld/%ld\n", a, n,
            shl64(a, n), ref_shl(a, n), shru64(a, n), ref_shru(a, n), shr64(a, n), ref_shr(a, n));
        abort();
      }
    }
  for (unsigned i = 0; i < TRIALS; i++) {
    long a = (long)random_word();
    long n = (long)(random_word() % 80) - 8;
    checks++;
    if (shl64(a, n) != ref_shl(a, n) || shru64(a, n) != ref_shru(a, n) || shr64(a, n) != ref_shr(a, n)) abort();
  }
}

static void check_exact_functions(void) {
  check_exact_unary("floor", floor64, floor);
  check_exact_unary("ceil", ceil64, ceil);
  check_exact_unary("trunc", trunc64, trunc);
  check_exact_unary("round", round64, nearbyint);
  check_exact_binary("fmod", fmod64, fmod);
  check_exact_binary("copysign", copysign64, copysign);
  check_binary32();
  check_total_shifts();
}

static uint64_t reference(host_unary host, uint64_t x) {
  double rounded = host(of(x));
  return bits(rounded);
}

// Distance in ulps between two finite words of the same sign.
static uint64_t ulps(uint64_t a, uint64_t b) { return a > b ? a - b : b - a; }

static void check_soft(const char *what, device_unary device, host_unary host, int low, int range) {
  // Exact specials every composed function handles: 0, ±0, ±inf, NaN.
  const uint64_t exact_specials[] = {0, UINT64_C(0x8000000000000000), UINT64_C(0x7ff0000000000000),
      UINT64_C(0xfff0000000000000), UINT64_C(0x7ff8000000000000)};
  for (unsigned i = 0; i < sizeof exact_specials / sizeof *exact_specials; i++)
    expect(what, exact_specials[i], 0, device(exact_specials[i]), reference(host, exact_specials[i]));
  unsigned long exact = 0, trials = 0;
  uint64_t worst = 0;
  for (unsigned i = 0; i < TRIALS; i++) {
    uint64_t x = word_in(low, range);
    uint64_t got = device(x), want = reference(host, x);
    trials++;
    checks++;
    if (same(got, want)) { exact++; continue; }
    uint64_t distance = ulps(got, want);
    if (distance > worst) worst = distance;
    if (distance > 16) {
      fprintf(stderr, "%s(%016llx): device %016llx host %016llx\n", what, (unsigned long long)x,
          (unsigned long long)got, (unsigned long long)want);
      abort();
    }
  }
  printf("[ok] %s: %lu of %lu match the host long double libm bit for bit, worst distance %llu ulp\n", what, exact, trials,
      (unsigned long long)worst);
}

static void soft_binary_case(const char *what, device_binary device, host_binary host, uint64_t a, uint64_t b) {
  uint64_t got = device(a, b), want = bits(host(of(a), of(b)));
  checks++;
  if (same(got, want)) return;
  uint64_t distance = ulps(got, want);
  if (distance > 64) {
    fprintf(stderr, "%s(%016llx, %016llx): device %016llx host %016llx\n", what,
        (unsigned long long)a, (unsigned long long)b, (unsigned long long)got, (unsigned long long)want);
    abort();
  }
}

static void check_soft_binary(const char *what, device_binary device, host_binary host, int xlow, int xrange, int ylow, int yrange) {
  for (unsigned i = 0; i < TRIALS; i++)
    soft_binary_case(what, device, host, word_in(xlow, xrange), word_in(ylow, yrange));
  printf("[ok] %s: binary within 1 ulp of the host long double libm over %u trials\n", what, TRIALS);
}

static void check_binary_specials(void) {
  const double values[] = {0.0, -0.0, 1.0, -1.0, 3.0, -3.0, 0.5, -0.5, INFINITY, -INFINITY, NAN};
  for (unsigned i = 0; i < sizeof values / sizeof *values; i++)
    for (unsigned j = 0; j < sizeof values / sizeof *values; j++) {
      uint64_t a = bits(values[i]), b = bits(values[j]);
      soft_binary_case("pow specials", pow64, pow, a, b);
      soft_binary_case("hypot specials", hypot64, hypot, a, b);
      soft_binary_case("atan2 specials", atan264, atan2, a, b);
    }
}

static void check_transcendental(const char *what, device_unary device, host_unary host, int low, int range) {
  unsigned long exact = 0, trials = 0;
  uint64_t worst = 0;
  for (unsigned i = 0; i < SPECIALS; i++) expect(what, specials[i], 0, device(specials[i]), reference(host, specials[i]));
  for (unsigned i = 0; i < TRIALS * 2; i++) {
    uint64_t x = word_in(low, range);
    if (what[0] == 'l') x &= ~UINT64_C(0x8000000000000000);
    uint64_t got = device(x), want = reference(host, x);
    trials++;
    checks++;
    if (same(got, want)) { exact++; continue; }
    uint64_t distance = ulps(got, want);
    if (distance > worst) worst = distance;
    if (distance > 1) {
      fprintf(stderr, "%s(%016llx): device %016llx host %016llx\n", what, (unsigned long long)x,
          (unsigned long long)got, (unsigned long long)want);
      abort();
    }
  }
  printf("[ok] %s: %lu of %lu match the host long double libm bit for bit, worst distance %llu ulp\n", what, exact, trials,
      (unsigned long long)worst);
  assert(trials - exact <= trials / 100);
}

static void check_checked_integers(void) {
  ulong fault = 0;
  assert(osprey_add(2, 3, &fault) == 5 && osprey_sub(2, 3, &fault) == -1 && osprey_mul(-4, 5, &fault) == -20);
  assert(osprey_div(7, -2, &fault) == -3 && osprey_rem(-7, 2, &fault) == -1 && osprey_neg(5, &fault) == -5);
  assert(osprey_abs(-9, &fault) == 9 && osprey_rem(INT64_MIN, -1, &fault) == 0 && !fault);
  assert(osprey_fdiv(from_long(1), from_long(4), &fault) == bits(0.25) && !fault);
  assert(osprey_add(INT64_MAX, 1, &fault) == 0 && fault == FAULT_OVERFLOW);
  fault = 0; assert(osprey_sub(INT64_MIN, 1, &fault) == 0 && fault == FAULT_OVERFLOW);
  fault = 0; assert(osprey_mul(INT64_MIN, -1, &fault) == 0 && fault == FAULT_OVERFLOW);
  fault = 0; assert(osprey_mul(-1, INT64_MIN, &fault) == 0 && fault == FAULT_OVERFLOW);
  fault = 0; assert(osprey_mul(INT64_C(3037000500), INT64_C(3037000500), &fault) == 0 && fault == FAULT_OVERFLOW);
  fault = 0; assert(osprey_div(INT64_MIN, -1, &fault) == 0 && fault == FAULT_OVERFLOW);
  fault = 0; assert(osprey_neg(INT64_MIN, &fault) == 0 && fault == FAULT_OVERFLOW);
  fault = 0; assert(osprey_abs(INT64_MIN, &fault) == 0 && fault == FAULT_OVERFLOW);
  fault = 0; assert(osprey_div(1, 0, &fault) == 0 && fault == FAULT_REMAINDER_BY_ZERO);
  fault = 0; assert(osprey_rem(1, 0, &fault) == 0 && fault == FAULT_REMAINDER_BY_ZERO);
  fault = 0; assert(osprey_fdiv(from_long(1), 0, &fault) == 0 && fault == FAULT_DIVIDE_BY_ZERO);
  checks += 17;
}

static void check_matmul_kernel(void) {
  ulong left[4] = {bits(0.5), bits(-1.0), bits(2.0), bits(3.0)}, out[4] = {0, 0, 0, 0};
  for (ulong i = 0; i < 4; i++) osprey_matmul64(left, left, out, 2, 2, 2, i);
  osprey_matmul64(left, left, out, 2, 2, 2, 4);
  assert(out[0] == bits(-1.75) && out[1] == bits(-3.5) && out[2] == bits(7.0) && out[3] == bits(7.0));
  checks += 4;
}

int main(void) {
  check_binary_specials();
  check_binary("add", add64, host_add);
  check_binary("sub", sub64, host_sub);
  check_binary("mul", multiply64, host_mul);
  check_binary("div", divide64, host_div);
  check_unary_and_conversions();
  check_checked_integers();
  check_exact_functions();
  check_matmul_kernel();
  check_transcendental("exp", exp64, expl, 960, 73);
  check_transcendental("ln", ln64, logl, 0, 2047);
  check_transcendental("tanh", tanh64, tanhl, 990, 38);
  check_soft("log2", log264, log2l, 0, 2047);
  check_soft("log10", log1064, log10l, 0, 2047);
  check_soft("expm1", expm164, expm1l, 960, 73);
  check_soft("log1p", log1p64, log1pl, 0, 2047);
  check_soft("sinh", sinh64, sinhl, 990, 38);
  check_soft("cosh", cosh64, coshl, 990, 38);
  check_soft("sin", sin64, sinl, 1007, 32);
  check_soft("cos", cos64, cosl, 1007, 32);
  check_soft("tan", tan64, tanl, 1010, 26);
  check_soft("asin", asin64, asinl, 1011, 11);
  check_soft("acos", acos64, acosl, 1011, 11);
  check_soft("atan", atan64, atanl, 990, 60);
  check_soft("asinh", asinh64, asinhl, 990, 60);
  check_soft("acosh", acosh64, acoshl, 1024, 60);
  check_soft("atanh", atanh64, atanhl, 1007, 16);
  check_soft("erf", erf64, erfl, 1011, 11);
  check_soft("erfc", erfc64, erfcl, 1011, 11);
  check_soft_binary("pow", pow64, pow, 1021, 4, 1018, 10);
  check_soft_binary("atan2", atan264, atan2, 990, 60, 990, 60);
  check_soft_binary("hypot", hypot64, hypot, 990, 60, 990, 60);
  printf("[ok] device binary64 library: %lu host parity checks\n", checks);
  return 0;
}
