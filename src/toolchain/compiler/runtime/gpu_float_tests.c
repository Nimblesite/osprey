// The native-width device arithmetic compiled as host C and held to the host's
// own IEEE results [GPU-DEVICE-NATIVE]: every float32 operation, comparison
// and conversion, and the binary16 narrowing from binary64 and binary32, bit
// for bit over signed zeros, subnormals, NaN, infinities and the
// overflow/underflow boundaries. The host oracle for a binary16 narrowing is
// the runtime's independently written `osp_float_to_f16` / `osp_f16_to_f32`
// (float_spelling.c), never the shader's own formula.
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
#include "float_spelling.h"
#include "gpu_test_words.h"

static unsigned long checks;

static float f32_value(uint32_t w) {
  float f;
  memcpy(&f, &w, sizeof f);
  return f;
}

static uint32_t f32_word(float f) {
  uint32_t w;
  memcpy(&w, &f, sizeof w);
  return w;
}

static uint64_t flag(bool b) { return b ? 1 : 0; }

static const uint32_t f32_specials[] = {0, 0x80000000u, 1, 0x80000001u, 0x00000001u, 0x007fffffu,
    0x00800000u, 0x3f800000u, 0xbf800000u, 0x7f7fffffu, 0x7f800000u, 0xff800000u,
    0x7fc00000u, 0xffc00000u, 0x3e800000u, 0x33800000u};
#define F32_SPECIALS (sizeof f32_specials / sizeof *f32_specials)

static void expect_f32(const char *what, uint32_t a, uint32_t b, uint32_t device, uint32_t host) {
  checks++;
  if (device != host && !(isnan(f32_value(device)) && isnan(f32_value(host)))) {
    fprintf(stderr, "%s(%08x, %08x): device %08x host %08x\n", what, a, b, device, host);
    abort();
  }
}

// A separately computed IEEE ordering, including unordered NaNs and equal
// signed zeros. This oracle uses only words, not the comparison under test.
static void check_float32_comparisons(uint32_t wa, uint32_t wb) {
  float a = f32_value(wa), b = f32_value(wb);
  bool unordered = (wa & 0x7fffffffu) > 0x7f800000u || (wb & 0x7fffffffu) > 0x7f800000u;
  bool equal = !unordered && (wa == wb || !((wa | wb) & 0x7fffffffu));
  uint32_t ka = wa & 0x80000000u ? ~wa : wa ^ 0x80000000u;
  uint32_t kb = wb & 0x80000000u ? ~wb : wb ^ 0x80000000u;
  bool less = !unordered && !equal && ka < kb;
  expect_f32("f32 eq", wa, wb, flag(a == b), flag(equal));
  expect_f32("f32 ne", wa, wb, flag(a != b), flag(!equal));
  expect_f32("f32 lt", wa, wb, flag(a < b), flag(less));
  expect_f32("f32 le", wa, wb, flag(a <= b), flag(less || equal));
  expect_f32("f32 gt", wa, wb, flag(a > b), flag(!unordered && !less && !equal));
  expect_f32("f32 ge", wa, wb, flag(a >= b), flag(!unordered && !less));
}

// Exercise the actual shared device helpers against binary64 host arithmetic
// rounded to binary32. Subtraction uses the emitted native expression; its
// oracle still computes at a separate width. No expression is its own oracle.
static void check_float32_pair(uint32_t wa, uint32_t wb) {
  float a = f32_value(wa), b = f32_value(wb);
  double wide_a = (double)a, wide_b = (double)b;
  expect_f32("f32 add", wa, wb, f32_word(add32(a, b)), f32_word((float)(wide_a + wide_b)));
  expect_f32("f32 sub", wa, wb, f32_word(a - b), f32_word((float)(wide_a - wide_b)));
  expect_f32("f32 mul", wa, wb, f32_word(multiply32(a, b)), f32_word((float)(wide_a * wide_b)));
  ulong fault = 0;
  float quotient = osprey_fdiv32(a, b, &fault);
  if (b == 0.0f) { assert(fault == FAULT_DIVIDE_BY_ZERO && quotient == 0.0f); checks += 2; }
  else { assert(fault == 0); expect_f32("f32 div", wa, wb, f32_word(quotient), f32_word((float)(wide_a / wide_b))); }
  fault = 0;
  float remainder = osprey_frem32(a, b, &fault);
  if (b == 0.0f) { assert(fault == FAULT_REMAINDER_BY_ZERO && remainder == 0.0f); checks += 2; }
  else { assert(fault == 0); expect_f32("f32 rem", wa, wb, f32_word(remainder), f32_word((float)fmod(wide_a, wide_b))); }
  fault = 0;
  ulong remainder64 = osprey_frem(bits(wide_a), bits(wide_b), &fault);
  if (b == 0.0f) { assert(fault == FAULT_REMAINDER_BY_ZERO && remainder64 == 0); checks += 2; }
  else { assert(fault == 0); assert(remainder64 == bits(fmod(wide_a, wide_b)) || (isnan(of(remainder64)) && isnan(fmod(wide_a, wide_b)))); checks += 2; }
  check_float32_comparisons(wa, wb);
}

static void check_float32_operations(void) {
  for (unsigned i = 0; i < F32_SPECIALS; i++)
    for (unsigned j = 0; j < F32_SPECIALS; j++)
      check_float32_pair(f32_specials[i], f32_specials[j]);
  for (unsigned i = 0; i < 100000; i++) {
    uint32_t wa = (uint32_t)random_word(), wb = (uint32_t)random_word();
    check_float32_pair(wa, wb);
  }
}

static void expect_f16(const char *what, uint64_t x, ushort device, ushort host) {
  checks++;
  if (device != host) {
    fprintf(stderr, "%s(%016llx): device %04x host %04x\n", what,
        (unsigned long long)x, device, host);
    abort();
  }
}

// binary16 widening and narrowing, each held to the runtime's independent
// oracle. The binary64 -> binary16 path is ONE rounding: the halfway value
// 1 + 2^-11 + 2^-30 must round up to 1 + 2^-10, which a binary64 -> binary32
// -> binary16 double rounding loses to 1.0.
static void check_float16_conversions(void) {
  for (uint32_t b = 0; b < 0x10000u; b++) {
    ushort h = (ushort)b;
    checks++;
    if (f32_word(gpu_f16_to_f32(h)) != f32_word(osp_f16_to_f32(h))) {
      fprintf(stderr, "f16 widen %04x: device %08x host %08x\n", b,
          f32_word(gpu_f16_to_f32(h)), f32_word(osp_f16_to_f32(h)));
      abort();
    }
    ushort back = gpu_f32_to_f16(gpu_f16_to_f32(h));
    checks++;
    if ((h & 0x7c00u) == 0x7c00u && (h & 0x03ffu)) {
      if ((back & 0x7c00u) != 0x7c00u || !(back & 0x03ffu)) {
        fprintf(stderr, "f16 nan %04x -> %04x\n", b, back);
        abort();
      }
    } else if (back != h) {
      fprintf(stderr, "f16 round trip %04x -> %04x\n", b, back);
      abort();
    }
  }
  for (unsigned i = 0; i < F32_SPECIALS; i++) {
    float x = f32_value(f32_specials[i]);
    expect_f16("f32->f16", f32_specials[i], gpu_f32_to_f16(x), osp_float_to_f16((double)x));
    checks++;
    if (f32_word(gpu_f32_narrow16(x)) != f32_word(osp_f16_to_f32(osp_float_to_f16((double)x)))) {
      fprintf(stderr, "f16 narrow widen %08x\n", f32_specials[i]);
      abort();
    }
  }
  for (unsigned i = 0; i < 100000; i++) {
    uint32_t w = (uint32_t)random_word();
    float x = f32_value(w);
    expect_f16("f32->f16", w, gpu_f32_to_f16(x), osp_float_to_f16((double)x));
    checks++;
    if (f32_word(gpu_f32_narrow16(x)) != f32_word(osp_f16_to_f32(osp_float_to_f16((double)x)))) {
      fprintf(stderr, "f16 narrow widen %08x\n", w);
      abort();
    }
  }
  // The binary64 narrowing is the one-rounding boundary case above.
  const uint64_t halfway = bits(1.0 + ldexp(1.0, -11) + ldexp(1.0, -30));
  expect_f16("f64->f16 halfway", halfway, gpu_f64_to_f16(halfway), osp_float_to_f16(of(halfway)));
  for (unsigned i = 0; i < 100000; i++) {
    uint64_t w = random_word();
    expect_f16("f64->f16", w, gpu_f64_to_f16(w), osp_float_to_f16(of(w)));
  }
}

// binary32 ulps between a value and the host's binary64 reference rounded once.
static unsigned ulps_from(float value, double reference) {
  float rounded = (float)reference;
  if (isnan(value) || isnan(rounded)) return isnan(value) == isnan(rounded) ? 0 : 1000;
  if (isinf(value) || isinf(rounded)) return value == rounded ? 0 : 1000;
  int32_t a, b;
  memcpy(&a, &value, sizeof a);
  memcpy(&b, &rounded, sizeof b);
  if (a < 0) a = INT32_MIN - a;
  if (b < 0) b = INT32_MIN - b;
  return (unsigned)llabs((long long)a - (long long)b);
}

// [GPU-DEVICE-NATIVE] The shared binary32 exp and tanh: within 2 ulps of the
// host's binary64 libm rounded once, exact at the special points, and never a
// different byte from the host build of the same source — which is this one.
static void check_float32_transcendentals(void) {
  static const float points[] = {0.0f, -0.0f, 1.0f, -1.0f, 88.7f, -87.3f, -103.9f, -104.0f, 89.0f, 1e-20f, -1e-20f, 0.5f, 9.5f, -9.5f,
                                 0.6931471805599453f, 2.0f, -2.0f, 1e-45f, 3.4028235e38f, -3.4028235e38f, 0.25f, 0.3f, -0.3f};
  for (unsigned i = 0; i < sizeof points / sizeof points[0]; i++) {
    assert(ulps_from(gpu32_exp(points[i]), exp((double)points[i])) <= 2);
    assert(ulps_from(gpu32_tanh(points[i]), tanh((double)points[i])) <= 2);
    checks += 2;
  }
  assert(isnan(gpu32_exp(NAN)) && isnan(gpu32_tanh(NAN)));
  assert(gpu32_exp(INFINITY) == INFINITY && gpu32_exp(-INFINITY) == 0.0f);
  assert(gpu32_tanh(INFINITY) == 1.0f && gpu32_tanh(-INFINITY) == -1.0f);
  checks += 6;
  unsigned worst_exp = 0, worst_tanh = 0;
  for (unsigned i = 0; i < 2000000; i++) {
    uint32_t w = (uint32_t)random_word();
    float x;
    memcpy(&x, &w, sizeof x);
    if (isnan(x) || isinf(x)) continue;
    float scaled = x > 150.0f || x < -150.0f ? fmodf(x, 150.0f) : x;
    unsigned e = ulps_from(gpu32_exp(scaled), exp((double)scaled)), t = ulps_from(gpu32_tanh(scaled), tanh((double)scaled));
    if (e > worst_exp) worst_exp = e;
    if (t > worst_tanh) worst_tanh = t;
    checks += 2;
  }
  printf("[ok] binary32 exp worst %u ulps, tanh worst %u ulps\n", worst_exp, worst_tanh);
  assert(worst_exp <= 2 && worst_tanh <= 2);
}

int main(void) {
  check_float32_transcendentals();
  check_float32_operations();
  check_float16_conversions();
  printf("[ok] device native-width arithmetic: %lu host parity checks\n", checks);
  return 0;
}
