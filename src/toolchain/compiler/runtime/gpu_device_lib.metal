// Device support for the kernels the compiler emits [GPU-BACKEND-DEVICE] and
// the builtin product [GPU-MATMUL]. A kernel cannot reach a runtime `Arith`
// policy, so a fallible integer operation or a division by zero records a
// fault word instead; the runtime reports a typed failure naming the kernel
// when a dispatch completes with a fault ([GPU-DEVICE-FAULT]). Follows gpu_f64.metal
// and gpu_math64.metal in the embedded shader source.

constant ulong FAULT_OVERFLOW = 1;
constant ulong FAULT_DIVIDE_BY_ZERO = 2;
constant ulong FAULT_REMAINDER_BY_ZERO = 3;

long osprey_fault(GPU_GLOBAL ulong *fault, ulong code) {
    fault[0] = code;
    return 0;
}

// Magnitudes as unsigned words: the device backends have no signed 64-bit
// division, so every checked product, quotient and remainder works on these.
ulong magnitude(long a) { return a < 0 ? 0ul - (ulong)a : (ulong)a; }
// Functions, not comparisons in the template: at an unsigned row these are
// always false, which a compiler rejects when written at the `long` cast.
bool is_negative(long a) { return a < 0; }
bool is_minus_one(long a) { return a == -1; }
long signed_word(ulong magnitude_word, bool negative) {
    return negative ? (long)(0ul - magnitude_word) : (long)magnitude_word;
}

// [GPU-DEVICE-NATIVE] One checked-arithmetic template for every integer row.
// Work in unsigned words, mask to the storage width, and fault before any
// invalid signed result is exposed. The default `int` keeps its public names.
#define CHECKED_INTEGER(SUFFIX, T, U, BITS, SIGNED) \
constant ulong INTEGER_SIGN##SUFFIX = ((ulong)1) << (BITS - 1); \
constant ulong INTEGER_MASK##SUFFIX = (ulong)(U)~(U)0; \
T osprey_convert##SUFFIX(long value, GPU_GLOBAL ulong *fault) { \
    long low = SIGNED ? (long)(0ul - INTEGER_SIGN##SUFFIX) : 0; \
    ulong high = SIGNED ? INTEGER_SIGN##SUFFIX - 1 : INTEGER_MASK##SUFFIX; \
    bool overflow = value < low || (value >= 0 && (ulong)value > high); \
    return overflow ? (T)osprey_fault(fault, FAULT_OVERFLOW) : (T)value; \
} \
T osprey_add##SUFFIX(T a, T b, GPU_GLOBAL ulong *fault) { \
    ulong x = (ulong)(U)a, y = (ulong)(U)b; \
    ulong r = (x + y) & INTEGER_MASK##SUFFIX; \
    bool overflow = SIGNED ? ((~(x ^ y) & (x ^ r) & INTEGER_SIGN##SUFFIX) != 0) : r < x; \
    return overflow ? (T)osprey_fault(fault, FAULT_OVERFLOW) : (T)(U)r; \
} \
T osprey_sub##SUFFIX(T a, T b, GPU_GLOBAL ulong *fault) { \
    ulong x = (ulong)(U)a, y = (ulong)(U)b; \
    ulong r = (x - y) & INTEGER_MASK##SUFFIX; \
    bool overflow = SIGNED ? (((x ^ y) & (x ^ r) & INTEGER_SIGN##SUFFIX) != 0) : x < y; \
    return overflow ? (T)osprey_fault(fault, FAULT_OVERFLOW) : (T)(U)r; \
} \
T osprey_mul##SUFFIX(T a, T b, GPU_GLOBAL ulong *fault) { \
    bool negative = SIGNED && ((is_negative((long)a)) != (is_negative((long)b))); \
    Wide p = wide_product(SIGNED ? magnitude((long)a) : (ulong)(U)a, SIGNED ? magnitude((long)b) : (ulong)(U)b); \
    ulong limit = SIGNED ? (negative ? INTEGER_SIGN##SUFFIX : INTEGER_SIGN##SUFFIX - 1) : INTEGER_MASK##SUFFIX; \
    if (p.high != 0 || p.low > limit) return (T)osprey_fault(fault, FAULT_OVERFLOW); \
    return (T)signed_word(p.low, negative); \
} \
T osprey_div##SUFFIX(T a, T b, GPU_GLOBAL ulong *fault) { \
    if (b == 0) return (T)osprey_fault(fault, FAULT_REMAINDER_BY_ZERO); \
    if (SIGNED && (ulong)(U)a == INTEGER_SIGN##SUFFIX && is_minus_one((long)b)) return (T)osprey_fault(fault, FAULT_OVERFLOW); \
    ulong x = SIGNED ? magnitude((long)a) : (ulong)(U)a, y = SIGNED ? magnitude((long)b) : (ulong)(U)b; \
    return (T)signed_word(x / y, SIGNED && ((is_negative((long)a)) != (is_negative((long)b)))); \
} \
T osprey_rem##SUFFIX(T a, T b, GPU_GLOBAL ulong *fault) { \
    if (b == 0) return (T)osprey_fault(fault, FAULT_REMAINDER_BY_ZERO); \
    ulong x = SIGNED ? magnitude((long)a) : (ulong)(U)a, y = SIGNED ? magnitude((long)b) : (ulong)(U)b; \
    return (T)signed_word(x % y, SIGNED && is_negative((long)a)); \
} \
T osprey_neg##SUFFIX(T a, GPU_GLOBAL ulong *fault) { \
    ulong x = (ulong)(U)a; \
    bool overflow = SIGNED ? x == INTEGER_SIGN##SUFFIX : x != 0; \
    return overflow ? (T)osprey_fault(fault, FAULT_OVERFLOW) : (T)(U)(0ul - x); \
} \
T osprey_abs##SUFFIX(T a, GPU_GLOBAL ulong *fault) { \
    return SIGNED && is_negative((long)a) ? osprey_neg##SUFFIX(a, fault) : a; \
}
CHECKED_INTEGER(_i8, signed char, unsigned char, 8, 1)
CHECKED_INTEGER(_i16, short, unsigned short, 16, 1)
CHECKED_INTEGER(_i32, int, uint, 32, 1)
CHECKED_INTEGER(, long, ulong, 64, 1)
CHECKED_INTEGER(_u8, unsigned char, unsigned char, 8, 0)
CHECKED_INTEGER(_u16, unsigned short, unsigned short, 16, 0)
CHECKED_INTEGER(_u32, uint, uint, 32, 0)
#undef CHECKED_INTEGER

ulong osprey_fdiv(ulong a, ulong b, GPU_GLOBAL ulong *fault) {
    if (!is_zero(b)) return divide64(a, b);
    long zero = osprey_fault(fault, FAULT_DIVIDE_BY_ZERO);
    return (ulong)zero;
}

// [GPU-DEVICE-NATIVE] [%] shares the exact software binary64 remainder.
ulong osprey_frem(ulong a, ulong b, GPU_GLOBAL ulong *fault) {
    if (!is_zero(b)) return fmod64(a, b);
    long zero = osprey_fault(fault, FAULT_REMAINDER_BY_ZERO);
    return (ulong)zero;
}

// [GPU-DEVICE-NATIVE]: explicit word boundaries; arithmetic stays binary32.
float gpu_float32_from_bits(uint bits) {
#if defined(__METAL_VERSION__)
    return as_type<float>(bits);
#elif defined(__OPENCL_VERSION__) || defined(__OPENCL_C_VERSION__)
    return as_float(bits);
#else
    union { uint bits; float value; } word = {bits};
    return word.value;
#endif
}

uint gpu_float32_bits(float value) {
#if defined(__METAL_VERSION__)
    return as_type<uint>(value);
#elif defined(__OPENCL_VERSION__) || defined(__OPENCL_C_VERSION__)
    return as_uint(value);
#else
    union { float value; uint bits; } word = {value};
    return word.bits;
#endif
}

// [GPU-DEVICE-NATIVE] binary16 <-> binary32 on integer bits, so `float16`
// computes in binary32 and narrows once: the exact widening of a half into a
// float, and the correctly rounded (ties to even) narrowing back, the same
// rounding the host's `osp_float_to_f16` applies from binary64.
#define GPU_F16_SUBNORMAL_UNIT 5.9604644775390625e-08f
float gpu_f16_to_f32(ushort bits) {
    uint sign = (uint)(bits & 0x8000u) << 16;
    uint exponent = (bits >> 10) & 0x1fu;
    uint fraction = bits & 0x03ffu;
    uint word;
    if (exponent == 0x1fu) {
        word = sign | 0x7f800000u | (fraction << 13);
    } else if (exponent) {
        word = sign | ((exponent + 112u) << 23) | (fraction << 13);
    } else {
        // A subnormal half is fraction * 2^-24 exactly, and the product is a
        // normal float, so one multiply widens it: no normalising loop, which
        // the real GGUF tensors hit on nine blocks in ten and which tripled a
        // quantized matmul's time.
        float small = (float)fraction * GPU_F16_SUBNORMAL_UNIT;
        return sign ? -small : small;
    }
    return gpu_float32_from_bits(word);
}

ushort gpu_f32_to_f16(float value) {
    uint bits = gpu_float32_bits(value);
    ushort sign = (ushort)((bits >> 16) & 0x8000u);
    uint exponent = (bits >> 23) & 0xffu;
    uint fraction = bits & 0x7fffffu;
    if (exponent == 0xffu) {
        if (!fraction) return (ushort)(sign | 0x7c00u);
        return (ushort)(sign | 0x7c00u | 0x0200u | (fraction >> 13));
    }
    if (!(bits & 0x7fffffffu)) return sign;
    uint significand = fraction | (exponent ? 0x800000u : 0u);
    int e2 = (int)exponent - 127;
    if (exponent && e2 >= -14) {
        if (e2 > 15) return (ushort)(sign | 0x7c00u);
        uint rounded = (significand >> 13) + (uint)((significand & 0x1fffu) > 0x1000u
            || ((significand & 0x1fffu) == 0x1000u && ((significand >> 13) & 1u)));
        return (ushort)(sign + (((uint)(exponent - 113)) << 10) + rounded);
    }
    int shift = exponent ? (-e2 - 1) : 125;
    if (shift >= 32) return sign;
    uint low = significand & (((uint)1 << (uint)shift) - 1u);
    uint narrowed = significand >> (uint)shift;
    uint halfway = (uint)1 << ((uint)shift - 1);
    if (low > halfway || (low == halfway && (narrowed & 1u))) narrowed++;
    return (ushort)(sign + narrowed);
}

float gpu_f32_narrow16(float value) { return gpu_f16_to_f32(gpu_f32_to_f16(value)); }

// binary64 -> binary16 in ONE rounding, never through binary32
// [GPU-DEVICE-NATIVE]: the same narrowing the host's `osp_float_to_f16`
// applies, computed on the binary64 word so a `float` kernel never double
// rounds. `gpu_round_shift` rounds `m >> shift` to nearest, ties to even.
static ulong gpu_round_shift(ulong m, int shift) {
    ulong kept = m >> (uint)shift;
    ulong rest = m & (((ulong)1 << (uint)shift) - 1ul);
    ulong tie = (ulong)1 << ((uint)shift - 1);
    return kept + (ulong)(rest > tie || (rest == tie && (kept & 1ul)));
}

ushort gpu_f64_to_f16(ulong x) {
    ushort sign = (ushort)((x >> 48) & 0x8000u);
    uint exponent = (uint)((x >> 52) & 0x7ffu);
    ulong fraction = x & 0x000ffffffffffffful;
    if (exponent == 0x7ffu) {
        if (!fraction) return (ushort)(sign | 0x7c00u);
        return (ushort)(sign | 0x7c00u | 0x0200u | (ushort)(fraction >> 42));
    }
    if (!(x & 0x7ffffffffffffffful)) return sign;
    ulong significand = fraction | (exponent ? (((ulong)1) << 52) : 0ul);
    int e2 = (int)exponent - 1023;
    if (exponent && e2 >= -14) {
        if (e2 > 15) return (ushort)(sign | 0x7c00u);
        ulong rounded = gpu_round_shift(significand, 42);
        return (ushort)(sign + (((uint)(exponent - 1009)) << 10) + (ushort)rounded);
    }
    int shift = 1051 - (int)exponent;
    if (shift >= 64) return sign;
    return (ushort)(sign + (ushort)gpu_round_shift(significand, shift));
}

float osprey_fdiv32(float a, float b, GPU_GLOBAL ulong *fault) {
    if (b != 0.0f) return a / b;
    long zero = osprey_fault(fault, FAULT_DIVIDE_BY_ZERO);
    return (float)zero;
}

// [GPU-DEVICE-NATIVE] Binary16 computes here and rounds once on storage.
float osprey_frem32(float a, float b, GPU_GLOBAL ulong *fault) {
    if (b == 0.0f) {
        long zero = osprey_fault(fault, FAULT_REMAINDER_BY_ZERO);
        return (float)zero;
    }
#if defined(__METAL_VERSION__) || defined(__OPENCL_VERSION__) || defined(__OPENCL_C_VERSION__)
    return fmod(a, b);
#else
    return fmodf(a, b);
#endif
}

#if defined(__METAL_VERSION__) || defined(__OPENCL_VERSION__) || defined(__OPENCL_C_VERSION__)
#define NATIVE_UNARY(NAME, DEVICE, HOST) float gpu32_##NAME(float x) { return DEVICE(x); }
#else
#define NATIVE_UNARY(NAME, DEVICE, HOST) float gpu32_##NAME(float x) { return HOST(x); }
#endif
NATIVE_UNARY(sqrt, sqrt, sqrtf)
NATIVE_UNARY(ln, log, logf)
NATIVE_UNARY(abs, fabs, fabsf)
#undef NATIVE_UNARY

// The host quantization oracle does not call these shader operations.
#ifndef GPU_HOST_QUANT_ORACLE
// [GPU-DEVICE-NATIVE] Shared binary32 exp and tanh use Cody-Waite reduction,
// a degree-9 Taylor polynomial, and exact power-of-two scaling. Keep a
// binary32 residual through the polynomial and quotient to avoid rounding
// away the low bits before the final result. Explicit FMA recovers product
// errors while implicit contraction stays off.
#define GPU_LOG2E 1.44269504088896341f
#define GPU_LN2_HI 0.693145751953125f
#define GPU_LN2_LO 1.428606765330187e-06f

float gpu32_scale_pow2(float value, int k) {
    if (k > 127) return value * gpu_float32_from_bits(0x7f000000u) * gpu_float32_from_bits((uint)(k - 127 + 127) << 23);
    if (k < -126) {
        int head = k < -252 ? -126 : k + 126;
        float partial = value * gpu_float32_from_bits((uint)(head + 127) << 23);
        return partial * gpu_float32_from_bits((uint)(k - head + 127) << 23);
    }
    return value * gpu_float32_from_bits((uint)(k + 127) << 23);
}

#if defined(__METAL_VERSION__) || defined(__OPENCL_VERSION__) || defined(__OPENCL_C_VERSION__)
#define GPU32_FMA fma
#else
#define GPU32_FMA fmaf
#endif

typedef struct { float value, error; } Gpu32Pair;
Gpu32Pair gpu32_add_pair(Gpu32Pair a, float b) {
    float sum = a.value + b;
    float tail = sum - a.value;
    float error = (a.value - (sum - tail)) + (b - tail) + a.error;
    float value = sum + error;
    return (Gpu32Pair){value, error - (value - sum)};
}

Gpu32Pair gpu32_mul_pair(Gpu32Pair a, float b) {
    float value = a.value * b;
    return (Gpu32Pair){value, GPU32_FMA(a.value, b, -value) + a.error * b};
}

Gpu32Pair gpu32_expm1_poly(float r) {
    Gpu32Pair p = {1.0f / 362880.0f, 0.0f};
    p = gpu32_add_pair(gpu32_mul_pair(p, r), 1.0f / 40320.0f);
    p = gpu32_add_pair(gpu32_mul_pair(p, r), 1.0f / 5040.0f);
    p = gpu32_add_pair(gpu32_mul_pair(p, r), 1.0f / 720.0f);
    p = gpu32_add_pair(gpu32_mul_pair(p, r), 1.0f / 120.0f);
    p = gpu32_add_pair(gpu32_mul_pair(p, r), 1.0f / 24.0f);
    p = gpu32_add_pair(gpu32_mul_pair(p, r), 1.0f / 6.0f);
    p = gpu32_add_pair(gpu32_mul_pair(p, r), 0.5f);
    return gpu32_add_pair(gpu32_mul_pair(gpu32_mul_pair(p, r), r), r);
}

typedef struct { Gpu32Pair mantissa; int exponent; } Gpu32Exp;

Gpu32Exp gpu32_exp_parts(float x) {
    float y = GPU32_FMA(x, GPU_LOG2E, 0.5f);
    int k = (int)y - (int)((float)(int)y > y);
    float kf = (float)k;
    float high = GPU32_FMA(-kf, GPU_LN2_HI, x);
    float r = GPU32_FMA(-kf, GPU_LN2_LO, high);
    float dr = GPU32_FMA(-kf, GPU_LN2_LO, high - r);
    Gpu32Pair p = gpu32_add_pair(gpu32_expm1_poly(r), 1.0f);
    return (Gpu32Exp){gpu32_add_pair(p, p.value * dr), k};
}

float gpu32_exp(float x) {
    if (x != x) return x;
    if (x > 88.7228394f) return gpu_float32_from_bits(0x7f800000u);
    if (x < -103.972084f) return 0.0f;
    Gpu32Exp parts = gpu32_exp_parts(x);
    return gpu32_scale_pow2(parts.mantissa.value, parts.exponent);
}

float gpu32_tanh(float x) {
    if (x != x) return x;
    uint sign = gpu_float32_bits(x) & 0x80000000u;
    float a = gpu_float32_from_bits(gpu_float32_bits(x) ^ sign);
    if (a > 9.0f) return gpu_float32_from_bits(0x3f800000u | sign);
    // At this magnitude the cubic correction rounds away.
    if (a < 0x1p-13f) return x;
    Gpu32Pair n;
    // Evaluate expm1 directly while 2a fits in the reduced interval.
    if (a < 0.17f) n = gpu32_expm1_poly(2.0f * a);
    else {
        Gpu32Exp parts = gpu32_exp_parts(2.0f * a);
        float scale = gpu32_scale_pow2(1.0f, parts.exponent);
        n = gpu32_add_pair(gpu32_mul_pair(parts.mantissa, scale), -1.0f);
    }
    Gpu32Pair d = gpu32_add_pair(n, 2.0f);
    float q = n.value / d.value;
    float r = GPU32_FMA(-q, d.value, n.value) + n.error - q * d.error;
    return gpu_float32_from_bits(gpu_float32_bits(q + r / d.value) | sign);
}

#undef GPU32_FMA
#endif

// [GPU-MATMUL]: one ordered kernel template at each supported float width.
float multiply32(float a, float b) { return a * b; }
float add32(float a, float b) { return a + b; }

#define MATRIX_KERNEL(NAME, WORD, MULTIPLY, ADD) \
kernel void NAME(GPU_GLOBAL const WORD *left GPU_BUFFER(0), \
                 GPU_GLOBAL const WORD *right GPU_BUFFER(1), \
                 GPU_GLOBAL WORD *out GPU_BUFFER(2), \
                 GPU_PARAM rows GPU_BUFFER(3), GPU_PARAM inner GPU_BUFFER(4), \
                 GPU_PARAM cols GPU_BUFFER(5) GPU_THREAD) { \
    ulong i = GPU_INDEX; \
    if (i >= rows * cols) return; \
    ulong row = i / cols, col = i % cols; \
    WORD total = 0; \
    for (ulong k = 0; k < inner; k++) { \
        WORD product = MULTIPLY(left[row * inner + k], right[k * cols + col]); \
        total = ADD(total, product); \
    } \
    out[i] = total; \
}
MATRIX_KERNEL(osprey_matmul64, ulong, multiply64, add64)
MATRIX_KERNEL(osprey_matmul32, float, multiply32, add32)
#undef MATRIX_KERNEL

// [LLM-MATMUL-QUANT] The product of a float32 matrix and quantized GGUF
// weights. Every block format the product decodes shares one contract: a
// weight is decoded from its block exactly as ggml's `dequantize_row_<type>`
// decodes it, multiplied by x, then added, every step rounded at binary32 with
// nothing contracted. Row c of the weights is `inner / <weights per block>`
// consecutive blocks.
#if !defined(__METAL_VERSION__) && !defined(__OPENCL_VERSION__) && !defined(__OPENCL_C_VERSION__)
typedef unsigned char uchar;
typedef struct { uint x, y, z, w; } uint4;
#endif

// Sixteen bytes at a 16-byte-aligned address as four words: one load on the
// devices, where a quantized block's header (scales and sub-block scales) is
// read by every lane of a simdgroup and byte loads cost a load each.
uint4 gpu_word4(GPU_GLOBAL const uchar *p) {
#if defined(__METAL_VERSION__)
    return *(GPU_GLOBAL const uint4 *)(GPU_GLOBAL const void *)p;
#elif defined(__OPENCL_VERSION__) || defined(__OPENCL_C_VERSION__)
    return vload4(0, (GPU_GLOBAL const uint *)(GPU_GLOBAL const void *)p);
#else
    uint4 h;
    __builtin_memcpy(&h, p, sizeof h);
    return h;
#endif
}

// Byte n (0..11) of the twelve that follow the first word of a 16-byte header.
uint gpu_word4_byte(uint4 h, uint n) {
    uint w = n < 4 ? h.y : (n < 8 ? h.z : h.w);
    return (w >> (8 * (n & 3))) & 255u;
}

// The 6-bit scale and minimum of sub-block j, packed as ggml's
// `get_scale_min_k4` packs them in the twelve bytes after d and dmin, shared
// by Q4_K and Q5_K; `h` is the block's 16-byte header.
uint q4k_scale(uint4 h, uint j) {
    return j < 4 ? (gpu_word4_byte(h, j) & 63) : ((gpu_word4_byte(h, j + 4) & 15) | ((gpu_word4_byte(h, j - 4) >> 6) << 4));
}
uint q4k_min(uint4 h, uint j) {
    return j < 4 ? (gpu_word4_byte(h, j + 4) & 63) : ((gpu_word4_byte(h, j + 4) >> 4) | ((gpu_word4_byte(h, j) >> 6) << 4));
}

// One weight's contribution: the product is a separate value, then added, so
// no step contracts ([GPU-MATMUL]'s order contract).
float quant_add(float acc, float x, float weight) {
    float product = x * weight;
    return acc + product;
}

// The binary16 scale a GGUF block stores at byte `at`.
float quant_half(GPU_GLOBAL const uchar *b, uint at) {
    return gpu_f16_to_f32((ushort)(b[at] | (b[at + 1] << 8)));
}

// Four weights of a linear block from one little-endian word of 8-bit levels:
// each weight rounds its scale product and its minimum, each contribution rounds
// its product, and the additions run in element order, exactly as one at a time.
// Reading the word once is what makes the kernel fast: a byte load per weight
// costs up to four times as much on the real tensors. `gpuMatMulQuant` checks
// that the weights start on a 4-byte boundary, and every block format below
// keeps its word fields on one.
float quant_lanes(float acc, GPU_GLOBAL const float *x, uint word, float d1, float m1) {
    float w0 = d1 * (float)(word & 255), w1 = d1 * (float)((word >> 8) & 255);
    float w2 = d1 * (float)((word >> 16) & 255), w3 = d1 * (float)(word >> 24);
    w0 = w0 - m1;
    w1 = w1 - m1;
    w2 = w2 - m1;
    w3 = w3 - m1;
    float p0 = x[0] * w0, p1 = x[1] * w1, p2 = x[2] * w2, p3 = x[3] * w3;
    acc = acc + p0;
    acc = acc + p1;
    acc = acc + p2;
    return acc + p3;
}

// Q4_K: 256 weights per 144-byte block, a binary16 scale and minimum, eight
// 6-bit sub-block scales and minimums, 4-bit weights.
float q4_k_lane(float acc, GPU_GLOBAL const float *x, GPU_GLOBAL const uchar *b, uint lane) {
    uint4 h = gpu_word4(b);
    float d = gpu_f16_to_f32((ushort)(h.x & 0xffffu)), dmin = gpu_f16_to_f32((ushort)(h.x >> 16));
    GPU_GLOBAL const uint *qs = (GPU_GLOBAL const uint *)(GPU_GLOBAL const void *)(b + 16);
    uint j = lane >> 2, g = j >> 1, high = j & 1;
    uint sc = q4k_scale(h, j), mn = q4k_min(h, j);
    float d1 = d * (float)sc;
    float m1 = dmin * (float)mn;
    for (uint l = (lane & 3) * 2; l < (lane & 3) * 2 + 2; l++)
        acc = quant_lanes(acc, x + j * 32 + l * 4, (qs[g * 8 + l] >> (high * 4)) & 0x0f0f0f0fu, d1, m1);
    return acc;
}

// Q6_K: 256 weights per 210-byte block, six bits per weight from a low nibble and
// two mask bits, a binary16 scale and sixteen 8-bit sub-block scales.
float q6_k_lane(float acc, GPU_GLOBAL const float *x, GPU_GLOBAL const uchar *b, uint lane) {
    float d = quant_half(b, 208);
    GPU_GLOBAL const ushort *low = (GPU_GLOBAL const ushort *)(GPU_GLOBAL const void *)b;
    GPU_GLOBAL const ushort *high = (GPU_GLOBAL const ushort *)(GPU_GLOBAL const void *)(b + 128);
    uint j = lane >> 2, group = j >> 2, quarter = j & 3;
    GPU_GLOBAL const ushort *nibbles = low + 32 * group + 16 * (quarter & 1);
    GPU_GLOBAL const ushort *tops = high + 16 * group;
    GPU_GLOBAL const float *xq = x + j * 32;
    for (uint i = (lane & 3) * 4; i < (lane & 3) * 4 + 4; i++) {
        float scale = d * (float)(signed char)b[192 + 8 * group + (i >> 3) + 2 * quarter];
        uint word = ((((uint)nibbles[i]) >> (4 * (quarter >> 1))) & 0x0f0fu) | ((((((uint)tops[i]) >> (2 * quarter)) & 0x0303u)) << 4);
        float w0 = scale * (float)((int)(word & 255) - 32), w1 = scale * (float)((int)(word >> 8) - 32);
        float p0 = xq[2 * i] * w0, p1 = xq[2 * i + 1] * w1;
        acc = acc + p0;
        acc = acc + p1;
    }
    return acc;
}

// Q8_0: 32 weights per 34-byte block, one binary16 scale and one signed byte per weight.
float q8_0_lane(float acc, GPU_GLOBAL const float *x, GPU_GLOBAL const uchar *b, uint lane) {
    float d = quant_half(b, 0);
    for (uint j = lane * 8; j < lane * 8 + 8; j++)
        acc = quant_add(acc, x[j], d * (float)(signed char)b[2 + j]);
    return acc;
}

// Q5_K: 256 weights per 176-byte block, Q4_K's four low bits plus a high-bit mask,
// a binary16 scale and minimum and eight 6-bit sub-block scales and minimums.
float q5_k_lane(float acc, GPU_GLOBAL const float *x, GPU_GLOBAL const uchar *b, uint lane) {
    uint4 h = gpu_word4(b);
    float d = gpu_f16_to_f32((ushort)(h.x & 0xffffu)), dmin = gpu_f16_to_f32((ushort)(h.x >> 16));
    GPU_GLOBAL const uint *mask = (GPU_GLOBAL const uint *)(GPU_GLOBAL const void *)(b + 16);
    GPU_GLOBAL const uint *qs = (GPU_GLOBAL const uint *)(GPU_GLOBAL const void *)(b + 48);
    uint sub = lane >> 2, g = sub >> 1, hi = sub & 1;
    uint sc = q4k_scale(h, sub), mn = q4k_min(h, sub);
    float d1 = d * (float)sc;
    float m1 = dmin * (float)mn;
    for (uint l = (lane & 3) * 2; l < (lane & 3) * 2 + 2; l++) {
        uint low = (qs[g * 8 + l] >> (hi * 4)) & 0x0f0f0f0fu;
        acc = quant_lanes(acc, x + sub * 32 + l * 4, low | (((mask[l] >> sub) & 0x01010101u) << 4), d1, m1);
    }
    return acc;
}

// Q3_K's descrambled 6-bit scale `index` of a block: ggml's `dequantize_row_q3_K`
// builds four words from the twelve packed bytes, keeping the low four bits of a
// byte and taking two more from the third.
uint q3k_scale(GPU_GLOBAL const uchar *b, uint index) {
    GPU_GLOBAL const uchar *s = b + 96;
    uint low = (uint)s[0] | ((uint)s[1] << 8) | ((uint)s[2] << 16) | ((uint)s[3] << 24);
    uint high = (uint)s[4] | ((uint)s[5] << 8) | ((uint)s[6] << 16) | ((uint)s[7] << 24);
    uint packed = (uint)s[8] | ((uint)s[9] << 8) | ((uint)s[10] << 16) | ((uint)s[11] << 24);
    uint word = index >> 2, part = index & 3;
    uint bytes = word < 2 ? (word ? high : low) : ((word == 2 ? low : high) >> 4);
    uint more = ((packed >> (2 * word)) & 0x03030303u) << 4;
    return (((bytes & 0x0f0f0f0fu) | more) >> (8 * part)) & 255u;
}

// Q3_K: 256 weights per 110-byte block, two low bits per weight plus a high-bit
// mask, a binary16 scale and sixteen 6-bit sub-block scales.
float q3_k_lane(float acc, GPU_GLOBAL const float *x, GPU_GLOBAL const uchar *b, uint lane) {
    float d = quant_half(b, 108);
    uint g = lane >> 4, j = (lane >> 2) & 3, side = (lane >> 1) & 1;
    uint sc = q3k_scale(b, 8 * g + 2 * j + side);
    float dl = d * (float)((int)sc - 32);
    GPU_GLOBAL const uchar *q = b + 32 + 32 * g + 16 * side;
    GPU_GLOBAL const uchar *hm = b + 16 * side;
    for (uint l = (lane & 1) * 8; l < (lane & 1) * 8 + 8; l++) {
        int low = (int)((q[l] >> (2 * j)) & 3);
        int high = (int)((hm[l] >> (4 * g + j)) & 1);
        acc = quant_add(acc, x[g * 128 + j * 32 + side * 16 + l], dl * (float)(low - (high ? 0 : 4)));
    }
    return acc;
}

// ggml's `kvalues_iq4nl`: the sixteen non-linear levels IQ4_NL and IQ4_XS use
// (ggml-common.h at the commit the gguf port pins).
constant float IQ4_NL_LEVELS[16] = {-127.0f, -104.0f, -83.0f, -65.0f, -49.0f, -35.0f, -22.0f, -10.0f, 1.0f, 13.0f, 25.0f, 38.0f, 53.0f, 69.0f, 89.0f, 113.0f};

// IQ4_NL: 32 weights per 18-byte block, one of the sixteen non-linear levels per
// weight and a binary16 scale.
float iq4_nl_lane(float acc, GPU_GLOBAL const float *x, GPU_GLOBAL const uchar *b, uint lane) {
    float d = quant_half(b, 0);
    for (uint j = lane * 8; j < lane * 8 + 8; j++) {
        uint level = j < 16 ? (uint)(b[2 + j] & 15) : (uint)(b[2 + j - 16] >> 4);
        acc = quant_add(acc, x[j], d * IQ4_NL_LEVELS[level]);
    }
    return acc;
}

// IQ4_XS: 256 weights per 136-byte block, IQ4_NL's levels with a six-bit
// sub-block scale and a binary16 block scale.
float iq4_xs_lane(float acc, GPU_GLOBAL const float *x, GPU_GLOBAL const uchar *b, uint lane) {
    float d = quant_half(b, 0);
    uint scales_h = (uint)(b[2] | (b[3] << 8));
    GPU_GLOBAL const uint *qs = (GPU_GLOBAL const uint *)(GPU_GLOBAL const void *)(b + 8);
    uint ib = lane >> 2;
    uint ls = (uint)((b[4 + (ib >> 1)] >> (4 * (ib & 1))) & 15) | (((scales_h >> (2 * ib)) & 3) << 4);
    float dl = d * (float)((int)ls - 32);
    for (uint l = (lane & 3) * 2; l < (lane & 3) * 2 + 2; l++) {
        uint word = qs[ib * 4 + (l & 3)] >> (4 * (l >> 2));
        GPU_GLOBAL const float *xe = x + 32 * ib + 4 * l;
        float p0 = xe[0] * (dl * IQ4_NL_LEVELS[word & 15]), p1 = xe[1] * (dl * IQ4_NL_LEVELS[(word >> 8) & 15]);
        float p2 = xe[2] * (dl * IQ4_NL_LEVELS[(word >> 16) & 15]), p3 = xe[3] * (dl * IQ4_NL_LEVELS[(word >> 24) & 15]);
        acc = acc + p0;
        acc = acc + p1;
        acc = acc + p2;
        acc = acc + p3;
    }
    return acc;
}

// ggml's `iq3s_grid`: 512 words, each four 4-bit grid values packed little-endian
// (ggml-common.h at the commit the gguf port pins).
constant uint IQ3S_GRID[512] = {
    0x01010101u, 0x01010103u, 0x01010105u, 0x0101010bu, 0x0101010fu, 0x01010301u, 0x01010303u, 0x01010305u,
    0x01010309u, 0x0101030du, 0x01010501u, 0x01010503u, 0x0101050bu, 0x01010707u, 0x01010901u, 0x01010905u,
    0x0101090bu, 0x0101090fu, 0x01010b03u, 0x01010b07u, 0x01010d01u, 0x01010d05u, 0x01010f03u, 0x01010f09u,
    0x01010f0fu, 0x01030101u, 0x01030103u, 0x01030105u, 0x01030109u, 0x01030301u, 0x01030303u, 0x0103030bu,
    0x01030501u, 0x01030507u, 0x0103050fu, 0x01030703u, 0x0103070bu, 0x01030909u, 0x01030d03u, 0x01030d0bu,
    0x01030f05u, 0x01050101u, 0x01050103u, 0x0105010bu, 0x0105010fu, 0x01050301u, 0x01050307u, 0x0105030du,
    0x01050503u, 0x0105050bu, 0x01050701u, 0x01050709u, 0x01050905u, 0x0105090bu, 0x0105090fu, 0x01050b03u,
    0x01050b07u, 0x01050f01u, 0x01050f07u, 0x01070107u, 0x01070303u, 0x0107030bu, 0x01070501u, 0x01070505u,
    0x01070703u, 0x01070707u, 0x0107070du, 0x01070909u, 0x01070b01u, 0x01070b05u, 0x01070d0fu, 0x01070f03u,
    0x01070f0bu, 0x01090101u, 0x01090307u, 0x0109030fu, 0x01090503u, 0x01090509u, 0x01090705u, 0x01090901u,
    0x01090907u, 0x01090b03u, 0x01090f01u, 0x010b0105u, 0x010b0109u, 0x010b0501u, 0x010b0505u, 0x010b050du,
    0x010b0707u, 0x010b0903u, 0x010b090bu, 0x010b090fu, 0x010b0d0du, 0x010b0f07u, 0x010d010du, 0x010d0303u,
    0x010d0307u, 0x010d0703u, 0x010d0b05u, 0x010d0f03u, 0x010f0101u, 0x010f0105u, 0x010f0109u, 0x010f0501u,
    0x010f0505u, 0x010f050du, 0x010f0707u, 0x010f0b01u, 0x010f0b09u, 0x03010101u, 0x03010103u, 0x03010105u,
    0x03010109u, 0x03010301u, 0x03010303u, 0x03010307u, 0x0301030bu, 0x0301030fu, 0x03010501u, 0x03010505u,
    0x03010703u, 0x03010709u, 0x0301070du, 0x03010b09u, 0x03010b0du, 0x03010d03u, 0x03010f05u, 0x03030101u,
    0x03030103u, 0x03030107u, 0x0303010du, 0x03030301u, 0x03030309u, 0x03030503u, 0x03030701u, 0x03030707u,
    0x03030903u, 0x03030b01u, 0x03030b05u, 0x03030f01u, 0x03030f0du, 0x03050101u, 0x03050305u, 0x0305030bu,
    0x0305030fu, 0x03050501u, 0x03050509u, 0x03050705u, 0x03050901u, 0x03050907u, 0x03050b0bu, 0x03050d01u,
    0x03050f05u, 0x03070103u, 0x03070109u, 0x0307010fu, 0x03070301u, 0x03070307u, 0x03070503u, 0x0307050fu,
    0x03070701u, 0x03070709u, 0x03070903u, 0x03070d05u, 0x03070f01u, 0x03090107u, 0x0309010bu, 0x03090305u,
    0x03090309u, 0x03090703u, 0x03090707u, 0x03090905u, 0x0309090du, 0x03090b01u, 0x03090b09u, 0x030b0103u,
    0x030b0301u, 0x030b0307u, 0x030b0503u, 0x030b0701u, 0x030b0705u, 0x030b0b03u, 0x030d0501u, 0x030d0509u,
    0x030d050fu, 0x030d0909u, 0x030d090du, 0x030f0103u, 0x030f0107u, 0x030f0301u, 0x030f0305u, 0x030f0503u,
    0x030f070bu, 0x030f0903u, 0x030f0d05u, 0x030f0f01u, 0x05010101u, 0x05010103u, 0x05010107u, 0x0501010bu,
    0x0501010fu, 0x05010301u, 0x05010305u, 0x05010309u, 0x0501030du, 0x05010503u, 0x05010507u, 0x0501050fu,
    0x05010701u, 0x05010705u, 0x05010903u, 0x05010907u, 0x0501090bu, 0x05010b01u, 0x05010b05u, 0x05010d0fu,
    0x05010f01u, 0x05010f07u, 0x05010f0bu, 0x05030101u, 0x05030105u, 0x05030301u, 0x05030307u, 0x0503030fu,
    0x05030505u, 0x0503050bu, 0x05030703u, 0x05030709u, 0x05030905u, 0x05030b03u, 0x05050103u, 0x05050109u,
    0x0505010fu, 0x05050503u, 0x05050507u, 0x05050701u, 0x0505070fu, 0x05050903u, 0x05050b07u, 0x05050b0fu,
    0x05050f03u, 0x05050f09u, 0x05070101u, 0x05070105u, 0x0507010bu, 0x05070303u, 0x05070505u, 0x05070509u,
    0x05070703u, 0x05070707u, 0x05070905u, 0x05070b01u, 0x05070d0du, 0x05090103u, 0x0509010fu, 0x05090501u,
    0x05090507u, 0x05090705u, 0x0509070bu, 0x05090903u, 0x05090f05u, 0x05090f0bu, 0x050b0109u, 0x050b0303u,
    0x050b0505u, 0x050b070fu, 0x050b0901u, 0x050b0b07u, 0x050b0f01u, 0x050d0101u, 0x050d0105u, 0x050d010fu,
    0x050d0503u, 0x050d0b0bu, 0x050d0d03u, 0x050f010bu, 0x050f0303u, 0x050f050du, 0x050f0701u, 0x050f0907u,
    0x050f0b01u, 0x07010105u, 0x07010303u, 0x07010307u, 0x0701030bu, 0x0701030fu, 0x07010505u, 0x07010703u,
    0x07010707u, 0x0701070bu, 0x07010905u, 0x07010909u, 0x0701090fu, 0x07010b03u, 0x07010d07u, 0x07010f03u,
    0x07030103u, 0x07030107u, 0x0703010bu, 0x07030309u, 0x07030503u, 0x07030507u, 0x07030901u, 0x07030d01u,
    0x07030f05u, 0x07030f0du, 0x07050101u, 0x07050305u, 0x07050501u, 0x07050705u, 0x07050709u, 0x07050b01u,
    0x07070103u, 0x07070301u, 0x07070309u, 0x07070503u, 0x07070507u, 0x0707050fu, 0x07070701u, 0x07070903u,
    0x07070907u, 0x0707090fu, 0x07070b0bu, 0x07070f07u, 0x07090107u, 0x07090303u, 0x0709030du, 0x07090505u,
    0x07090703u, 0x07090b05u, 0x07090d01u, 0x07090d09u, 0x070b0103u, 0x070b0301u, 0x070b0305u, 0x070b050bu,
    0x070b0705u, 0x070b0909u, 0x070b0b0du, 0x070b0f07u, 0x070d030du, 0x070d0903u, 0x070f0103u, 0x070f0107u,
    0x070f0501u, 0x070f0505u, 0x070f070bu, 0x09010101u, 0x09010109u, 0x09010305u, 0x09010501u, 0x09010509u,
    0x0901050fu, 0x09010705u, 0x09010903u, 0x09010b01u, 0x09010f01u, 0x09030105u, 0x0903010fu, 0x09030303u,
    0x09030307u, 0x09030505u, 0x09030701u, 0x0903070bu, 0x09030907u, 0x09030b03u, 0x09030b0bu, 0x09050103u,
    0x09050107u, 0x09050301u, 0x0905030bu, 0x09050503u, 0x09050707u, 0x09050901u, 0x09050b0fu, 0x09050d05u,
    0x09050f01u, 0x09070109u, 0x09070303u, 0x09070307u, 0x09070501u, 0x09070505u, 0x09070703u, 0x0907070bu,
    0x09090101u, 0x09090105u, 0x09090509u, 0x0909070fu, 0x09090901u, 0x09090f03u, 0x090b010bu, 0x090b010fu,
    0x090b0503u, 0x090b0d05u, 0x090d0307u, 0x090d0709u, 0x090d0d01u, 0x090f0301u, 0x090f030bu, 0x090f0701u,
    0x090f0907u, 0x090f0b03u, 0x0b010105u, 0x0b010301u, 0x0b010309u, 0x0b010505u, 0x0b010901u, 0x0b010909u,
    0x0b01090fu, 0x0b010b05u, 0x0b010d0du, 0x0b010f09u, 0x0b030103u, 0x0b030107u, 0x0b03010bu, 0x0b030305u,
    0x0b030503u, 0x0b030705u, 0x0b030f05u, 0x0b050101u, 0x0b050303u, 0x0b050507u, 0x0b050701u, 0x0b05070du,
    0x0b050b07u, 0x0b070105u, 0x0b07010fu, 0x0b070301u, 0x0b07050fu, 0x0b070909u, 0x0b070b03u, 0x0b070d0bu,
    0x0b070f07u, 0x0b090103u, 0x0b090109u, 0x0b090501u, 0x0b090705u, 0x0b09090du, 0x0b0b0305u, 0x0b0b050du,
    0x0b0b0b03u, 0x0b0b0b07u, 0x0b0d0905u, 0x0b0f0105u, 0x0b0f0109u, 0x0b0f0505u, 0x0d010303u, 0x0d010307u,
    0x0d01030bu, 0x0d010703u, 0x0d010707u, 0x0d010d01u, 0x0d030101u, 0x0d030501u, 0x0d03050fu, 0x0d030d09u,
    0x0d050305u, 0x0d050709u, 0x0d050905u, 0x0d050b0bu, 0x0d050d05u, 0x0d050f01u, 0x0d070101u, 0x0d070309u,
    0x0d070503u, 0x0d070901u, 0x0d09050bu, 0x0d090907u, 0x0d090d05u, 0x0d0b0101u, 0x0d0b0107u, 0x0d0b0709u,
    0x0d0b0d01u, 0x0d0d010bu, 0x0d0d0901u, 0x0d0f0303u, 0x0d0f0307u, 0x0f010101u, 0x0f010109u, 0x0f01010fu,
    0x0f010501u, 0x0f010505u, 0x0f01070du, 0x0f010901u, 0x0f010b09u, 0x0f010d05u, 0x0f030105u, 0x0f030303u,
    0x0f030509u, 0x0f030907u, 0x0f03090bu, 0x0f050103u, 0x0f050109u, 0x0f050301u, 0x0f05030du, 0x0f050503u,
    0x0f050701u, 0x0f050b03u, 0x0f070105u, 0x0f070705u, 0x0f07070bu, 0x0f070b07u, 0x0f090103u, 0x0f09010bu,
    0x0f090307u, 0x0f090501u, 0x0f090b01u, 0x0f0b0505u, 0x0f0b0905u, 0x0f0d0105u, 0x0f0d0703u, 0x0f0f0101u,
};

// IQ3_S: 256 weights per 110-byte block, a nine-bit grid index and a sign bit per
// weight, a binary16 scale and eight 2-bit sub-block scales.
float iq3_s_lane(float acc, GPU_GLOBAL const float *x, GPU_GLOBAL const uchar *b, uint lane) {
    float d = quant_half(b, 0);
    uint group = lane >> 3, side = (lane >> 2) & 1, l = lane & 3;
    uint sc = (uint)b[106 + group];
    float db = d * (float)(1 + 2 * (int)(side ? (sc >> 4) : (sc & 15)));
    uint mask = (uint)b[66 + 2 * group + side];
    uint signs = (uint)b[74 + 8 * group + 4 * side + l];
    for (uint w = 0; w < 8; w++) {
        uint qs = (uint)b[2 + 16 * group + 8 * side + 2 * l + (w >> 2)];
        uint index = qs | ((mask << (8 - 2 * l - (w >> 2))) & 256);
        float weight = db * (float)((IQ3S_GRID[index] >> (8 * (w & 3))) & 255);
        if ((signs >> w) & 1) weight = -weight;
        acc = quant_add(acc, x[group * 64 + side * 32 + l * 8 + w], weight);
    }
    return acc;
}

// [LLM-MATMUL-QUANT] One tile of QUANT_ROWS outputs of one column per 32
// consecutive threads: lane l owns weights [8l, 8l + 8) of every block it visits
// (a 256-weight block has 32 lanes; eight 32-weight blocks share them), decodes
// each weight once and multiplies it into every row of the tile, summing each
// row's products in block order from 0.0f, and quant_reduce combines the lanes
// row by row. A prompt of T rows so reads and decodes the weights T / QUANT_ROWS
// times instead of T ([LLM-PREFILL]); one row is one tile with the other rows
// idle, the decode path unchanged. Each output's sum is the same lane-for-lane
// chain whatever the tile holds, so the host oracle in gpu_host.c, which compiles
// these same lane functions and runs the same rounds one output at a time,
// writes the same words as every backend. Threadgroups are a multiple of 32
// threads (gpu_metal.m, gpu_opencl.c), at most QUANT_GROUP, so a lane's group
// never straddles two.
#define QUANT_LANES 32
#define QUANT_GROUP 256
#define QUANT_ROWS 8

// The 32 lane sums of one output in a fixed butterfly: each round adds to every
// lane the lane `off` away, so lane 0 ends with the total. The order never
// depends on which lane reads first: a + b and b + a are the same word.
float quant_reduce(GPU_LOCAL float *sums, uint lane, float acc) {
    sums[lane] = acc;
    GPU_BARRIER();
    for (uint off = QUANT_LANES / 2; off > 0; off >>= 1) {
        float other = sums[lane ^ off];
        GPU_BARRIER();
        sums[lane] = sums[lane] + other;
        GPU_BARRIER();
    }
    return sums[lane];
}

#define QUANT_KERNEL(TYPE, LANE, ELEMENTS, BYTES) \
kernel void osprey_matmul_##TYPE(GPU_GLOBAL const float *x GPU_BUFFER(0), \
                                 GPU_GLOBAL const uchar *w GPU_BUFFER(1), \
                                 GPU_GLOBAL float *out GPU_BUFFER(2), \
                                 GPU_PARAM rows GPU_BUFFER(3), GPU_PARAM inner GPU_BUFFER(4), \
                                 GPU_PARAM cols GPU_BUFFER(5) GPU_LANE_THREAD) { \
    GPU_LOCAL float sums[QUANT_GROUP]; \
    ulong i = GPU_INDEX / QUANT_LANES, tiles = (rows + QUANT_ROWS - 1) / QUANT_ROWS; \
    uint lane = (uint)(GPU_INDEX % QUANT_LANES), per_block = (ELEMENTS) / 8, per_span = QUANT_LANES / per_block; \
    bool live = i < tiles * cols; \
    ulong r0 = live ? (i / cols) * QUANT_ROWS : 0, c = live ? i % cols : 0; \
    uint span = live ? (uint)(rows - r0 < QUANT_ROWS ? rows - r0 : QUANT_ROWS) : 0; \
    uint blocks = live ? (uint)(inner / (ELEMENTS)) : 0; \
    GPU_GLOBAL const float *xr = x + r0 * inner; \
    GPU_GLOBAL const uchar *row = w + c * (ulong)(inner / (ELEMENTS)) * (BYTES); \
    float acc[QUANT_ROWS]; \
    for (uint k = 0; k < QUANT_ROWS; k++) acc[k] = 0.0f; \
    for (uint blk = lane / per_block; blk < blocks; blk += per_span) \
        for (uint k = 0; k < span; k++) \
            acc[k] = LANE(acc[k], xr + k * inner + (ulong)blk * (ELEMENTS), row + (ulong)blk * (BYTES), lane % per_block); \
    for (uint k = 0; k < QUANT_ROWS; k++) { \
        float total = quant_reduce(sums, GPU_LOCAL_INDEX, acc[k]); \
        if (live && lane == 0 && k < span) out[(r0 + k) * cols + c] = total; \
    } \
}
QUANT_KERNEL(q4_k, q4_k_lane, 256, 144)
QUANT_KERNEL(q8_0, q8_0_lane, 32, 34)
QUANT_KERNEL(q6_k, q6_k_lane, 256, 210)
QUANT_KERNEL(q5_k, q5_k_lane, 256, 176)
QUANT_KERNEL(q3_k, q3_k_lane, 256, 110)
QUANT_KERNEL(iq4_nl, iq4_nl_lane, 32, 18)
QUANT_KERNEL(iq4_xs, iq4_xs_lane, 256, 136)
QUANT_KERNEL(iq3_s, iq3_s_lane, 256, 110)
#undef QUANT_KERNEL

// [LLM-DELTA-RULE] One token of the recurrent gated delta rule, fused into one
// kernel: thread t is value element v = t % dim of head h = t / dim. It decays
// its column of the [heads, dim, dim] state, recalls through the keys, writes the
// updated column and reads it out through the queries. `out` holds the new state
// then the heads * dim outputs, and every product is rounded before its add, as
// the host oracle in gpu_host.c does.
kernel void osprey_delta_rule(GPU_GLOBAL const float *state GPU_BUFFER(0), GPU_GLOBAL const float *queries GPU_BUFFER(1),
                              GPU_GLOBAL const float *keys GPU_BUFFER(2), GPU_GLOBAL const float *values GPU_BUFFER(3),
                              GPU_GLOBAL const float *beta GPU_BUFFER(4), GPU_GLOBAL const float *decay GPU_BUFFER(5),
                              GPU_GLOBAL float *out GPU_BUFFER(6), GPU_PARAM heads GPU_BUFFER(7),
                              GPU_PARAM dim GPU_BUFFER(8) GPU_THREAD) {
    uint t = (uint)GPU_INDEX, d = (uint)dim;
    if ((ulong)t >= heads * dim) return;
    uint h = t / d, v = t % d, plane = h * d * d;
    float fade = decay[h], recalled = 0.0f;
    for (uint k = 0; k < d; k++) {
        float kept = state[plane + k * d + v] * fade;
        float product = kept * keys[h * d + k];
        recalled = recalled + product;
    }
    float delta = (values[t] - recalled) * beta[h], read = 0.0f;
    for (uint k = 0; k < d; k++) {
        float kept = state[plane + k * d + v] * fade;
        float update = keys[h * d + k] * delta;
        float next = kept + update;
        out[plane + k * d + v] = next;
        float product = next * queries[h * d + k];
        read = read + product;
    }
    out[(uint)(heads * dim * dim) + t] = read;
}

// [LLM-SIGMOID] The logistic function of every element, on the target's native
// binary32 exp and division: an activation runs thousands of them per token, far
// too many for the shared wider exp that `gpu32_exp` rounds from. The host runs
// the same formula on its own exp, so results agree within exp's ulps, not bit
// for bit.
kernel void osprey_sigmoid(GPU_GLOBAL const float *x GPU_BUFFER(0), GPU_GLOBAL float *out GPU_BUFFER(1),
                           GPU_PARAM count GPU_BUFFER(2) GPU_THREAD) {
    ulong i = GPU_INDEX;
    if (i >= count) return;
#if defined(__METAL_VERSION__) || defined(__OPENCL_VERSION__) || defined(__OPENCL_C_VERSION__)
    out[i] = 1.0f / (1.0f + exp(-x[i]));
#else
    out[i] = 1.0f / (1.0f + expf(-x[i]));
#endif
}
