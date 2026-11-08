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
// each weight rounds its scale product and its minimum. Reading the word once is
// what makes the kernel fast: a byte load per weight costs up to four times as
// much on the real tensors. `gpuMatMulQuant` checks that the weights start on a
// 4-byte boundary, and every block format below keeps its word fields on one.
void quant_levels(GPU_PRIVATE float *w, uint word, float d1, float m1) {
    float w0 = d1 * (float)(word & 255), w1 = d1 * (float)((word >> 8) & 255);
    float w2 = d1 * (float)((word >> 16) & 255), w3 = d1 * (float)(word >> 24);
    w[0] = w0 - m1;
    w[1] = w1 - m1;
    w[2] = w2 - m1;
    w[3] = w3 - m1;
}

// A lane's eight contributions to one output from one block: its eight decoded
// weights against the eight inputs it owns, each product rounded, then added in
// element order, exactly as one at a time.
float quant_lane(float acc, GPU_PRIVATE const float *x, GPU_PRIVATE const float *w) {
    for (uint m = 0; m < 8; m++) acc = quant_add(acc, x[m], w[m]);
    return acc;
}

// Q4_K: 256 weights per 144-byte block, a binary16 scale and minimum, eight
// 6-bit sub-block scales and minimums, 4-bit weights.
void q4_k_weights(GPU_GLOBAL const uchar *b, uint lane, GPU_PRIVATE float *w) {
    uint4 h = gpu_word4(b);
    float d = gpu_f16_to_f32((ushort)(h.x & 0xffffu)), dmin = gpu_f16_to_f32((ushort)(h.x >> 16));
    GPU_GLOBAL const uint *qs = (GPU_GLOBAL const uint *)(GPU_GLOBAL const void *)(b + 16);
    uint j = lane >> 2, g = j >> 1, high = j & 1;
    uint sc = q4k_scale(h, j), mn = q4k_min(h, j);
    float d1 = d * (float)sc;
    float m1 = dmin * (float)mn;
    for (uint l = 0; l < 2; l++)
        quant_levels(w + 4 * l, (qs[g * 8 + (lane & 3) * 2 + l] >> (high * 4)) & 0x0f0f0f0fu, d1, m1);
}

// Q6_K: 256 weights per 210-byte block, six bits per weight from a low nibble and
// two mask bits, a binary16 scale and sixteen 8-bit sub-block scales.
void q6_k_weights(GPU_GLOBAL const uchar *b, uint lane, GPU_PRIVATE float *w) {
    float d = quant_half(b, 208);
    GPU_GLOBAL const ushort *low = (GPU_GLOBAL const ushort *)(GPU_GLOBAL const void *)b;
    GPU_GLOBAL const ushort *high = (GPU_GLOBAL const ushort *)(GPU_GLOBAL const void *)(b + 128);
    uint j = lane >> 2, group = j >> 2, quarter = j & 3;
    GPU_GLOBAL const ushort *nibbles = low + 32 * group + 16 * (quarter & 1);
    GPU_GLOBAL const ushort *tops = high + 16 * group;
    for (uint m = 0; m < 4; m++) {
        uint i = (lane & 3) * 4 + m;
        float scale = d * (float)(signed char)b[192 + 8 * group + (i >> 3) + 2 * quarter];
        uint word = ((((uint)nibbles[i]) >> (4 * (quarter >> 1))) & 0x0f0fu) | ((((((uint)tops[i]) >> (2 * quarter)) & 0x0303u)) << 4);
        w[2 * m] = scale * (float)((int)(word & 255) - 32);
        w[2 * m + 1] = scale * (float)((int)(word >> 8) - 32);
    }
}

// Q8_0: 32 weights per 34-byte block, one binary16 scale and one signed byte per weight.
void q8_0_weights(GPU_GLOBAL const uchar *b, uint lane, GPU_PRIVATE float *w) {
    float d = quant_half(b, 0);
    for (uint m = 0; m < 8; m++) w[m] = d * (float)(signed char)b[2 + lane * 8 + m];
}

// Q5_K: 256 weights per 176-byte block, Q4_K's four low bits plus a high-bit mask,
// a binary16 scale and minimum and eight 6-bit sub-block scales and minimums.
void q5_k_weights(GPU_GLOBAL const uchar *b, uint lane, GPU_PRIVATE float *w) {
    uint4 h = gpu_word4(b);
    float d = gpu_f16_to_f32((ushort)(h.x & 0xffffu)), dmin = gpu_f16_to_f32((ushort)(h.x >> 16));
    GPU_GLOBAL const uint *mask = (GPU_GLOBAL const uint *)(GPU_GLOBAL const void *)(b + 16);
    GPU_GLOBAL const uint *qs = (GPU_GLOBAL const uint *)(GPU_GLOBAL const void *)(b + 48);
    uint sub = lane >> 2, g = sub >> 1, hi = sub & 1;
    uint sc = q4k_scale(h, sub), mn = q4k_min(h, sub);
    float d1 = d * (float)sc;
    float m1 = dmin * (float)mn;
    for (uint m = 0; m < 2; m++) {
        uint l = (lane & 3) * 2 + m;
        uint low = (qs[g * 8 + l] >> (hi * 4)) & 0x0f0f0f0fu;
        quant_levels(w + 4 * m, low | (((mask[l] >> sub) & 0x01010101u) << 4), d1, m1);
    }
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
void q3_k_weights(GPU_GLOBAL const uchar *b, uint lane, GPU_PRIVATE float *w) {
    float d = quant_half(b, 108);
    uint g = lane >> 4, j = (lane >> 2) & 3, side = (lane >> 1) & 1;
    uint sc = q3k_scale(b, 8 * g + 2 * j + side);
    float dl = d * (float)((int)sc - 32);
    GPU_GLOBAL const uchar *q = b + 32 + 32 * g + 16 * side;
    GPU_GLOBAL const uchar *hm = b + 16 * side;
    for (uint m = 0; m < 8; m++) {
        uint l = (lane & 1) * 8 + m;
        int low = (int)((q[l] >> (2 * j)) & 3);
        int high = (int)((hm[l] >> (4 * g + j)) & 1);
        w[m] = dl * (float)(low - (high ? 0 : 4));
    }
}

// ggml's `kvalues_iq4nl`: the sixteen non-linear levels IQ4_NL and IQ4_XS use
// (ggml-common.h at the commit the gguf port pins).
constant float IQ4_NL_LEVELS[16] = {-127.0f, -104.0f, -83.0f, -65.0f, -49.0f, -35.0f, -22.0f, -10.0f, 1.0f, 13.0f, 25.0f, 38.0f, 53.0f, 69.0f, 89.0f, 113.0f};

// IQ4_NL: 32 weights per 18-byte block, one of the sixteen non-linear levels per
// weight and a binary16 scale.
void iq4_nl_weights(GPU_GLOBAL const uchar *b, uint lane, GPU_PRIVATE float *w) {
    float d = quant_half(b, 0);
    for (uint m = 0; m < 8; m++) {
        uint j = lane * 8 + m;
        uint level = j < 16 ? (uint)(b[2 + j] & 15) : (uint)(b[2 + j - 16] >> 4);
        w[m] = d * IQ4_NL_LEVELS[level];
    }
}

// IQ4_XS: 256 weights per 136-byte block, IQ4_NL's levels with a six-bit
// sub-block scale and a binary16 block scale.
void iq4_xs_weights(GPU_GLOBAL const uchar *b, uint lane, GPU_PRIVATE float *w) {
    float d = quant_half(b, 0);
    uint scales_h = (uint)(b[2] | (b[3] << 8));
    GPU_GLOBAL const uint *qs = (GPU_GLOBAL const uint *)(GPU_GLOBAL const void *)(b + 8);
    uint ib = lane >> 2;
    uint ls = (uint)((b[4 + (ib >> 1)] >> (4 * (ib & 1))) & 15) | (((scales_h >> (2 * ib)) & 3) << 4);
    float dl = d * (float)((int)ls - 32);
    for (uint m = 0; m < 2; m++) {
        uint l = (lane & 3) * 2 + m;
        uint word = qs[ib * 4 + (l & 3)] >> (4 * (l >> 2));
        w[4 * m] = dl * IQ4_NL_LEVELS[word & 15];
        w[4 * m + 1] = dl * IQ4_NL_LEVELS[(word >> 8) & 15];
        w[4 * m + 2] = dl * IQ4_NL_LEVELS[(word >> 16) & 15];
        w[4 * m + 3] = dl * IQ4_NL_LEVELS[(word >> 24) & 15];
    }
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
void iq3_s_weights(GPU_GLOBAL const uchar *b, uint lane, GPU_PRIVATE float *w) {
    float d = quant_half(b, 0);
    uint group = lane >> 3, side = (lane >> 2) & 1, l = lane & 3;
    uint sc = (uint)b[106 + group];
    float db = d * (float)(1 + 2 * (int)(side ? (sc >> 4) : (sc & 15)));
    uint mask = (uint)b[66 + 2 * group + side];
    uint signs = (uint)b[74 + 8 * group + 4 * side + l];
    for (uint m = 0; m < 8; m++) {
        uint qs = (uint)b[2 + 16 * group + 8 * side + 2 * l + (m >> 2)];
        uint index = qs | ((mask << (8 - 2 * l - (m >> 2))) & 256);
        float weight = db * (float)((IQ3S_GRID[index] >> (8 * (m & 3))) & 255);
        w[m] = ((signs >> m) & 1) ? -weight : weight;
    }
}

// [LLM-MATMUL-QUANT] One tile of QUANT_ROWS rows by QUANT_COLS columns of
// outputs per 32 consecutive threads: lane l owns weights [8l, 8l + 8) of every
// block it visits (a 256-weight block has 32 lanes; eight 32-weight blocks share
// them). Each block it decodes each column's eight weights once, reads each row's
// eight inputs once and multiplies every row into every column, summing each
// output's products in block order from 0.0f; quant_reduce then combines the
// lanes output by output. A prompt of T rows so reads and decodes the weights
// T / QUANT_ROWS times instead of T, and the inputs cols / QUANT_COLS times
// instead of cols ([LLM-PREFILL]); one row is one tile with the other rows idle,
// the decode path unchanged. The tile loops have constant bounds so the tile
// stays in registers (an 8 by 8 tile spills and runs twice as slow; staging the
// inputs in threadgroup memory costs more in barriers than it saves). Each
// output's sum is the same lane-for-lane chain whatever the tile holds, so the
// host oracle in gpu_host.c, which compiles these same lane functions and runs
// the same rounds one output at a time, writes the same words as every backend.
// Threadgroups are a multiple of 32 threads (gpu_metal.m, gpu_opencl.c), at most
// QUANT_GROUP, so a lane's group never straddles two.
#define QUANT_LANES 32
#define QUANT_GROUP 256
#define QUANT_ROWS 8
#define QUANT_COLS 4

// The 32 lane sums of QUANT_COLS outputs in a fixed butterfly: each round adds
// to every lane the lane `off` away, so lane 0 ends with the totals, left in
// `acc`. The order never depends on which lane reads first: a + b and b + a are
// the same word. Where the hardware's SIMD group is exactly the 32 lanes (Apple
// GPUs) the partner's word comes through a SIMD shuffle; anywhere else through
// threadgroup memory, between barriers.
void quant_reduce(GPU_LOCAL float *sums, uint lane, uint simd_width, GPU_PRIVATE float *acc) {
    if (simd_width == QUANT_LANES) {
        for (uint off = QUANT_LANES / 2; off > 0; off >>= 1)
            for (uint j = 0; j < QUANT_COLS; j++) acc[j] = acc[j] + GPU_SIMD_XOR(acc[j], off);
        return;
    }
    for (uint j = 0; j < QUANT_COLS; j++) sums[j * QUANT_GROUP + lane] = acc[j];
    GPU_BARRIER();
    for (uint off = QUANT_LANES / 2; off > 0; off >>= 1) {
        float other[QUANT_COLS];
        for (uint j = 0; j < QUANT_COLS; j++) other[j] = sums[j * QUANT_GROUP + (lane ^ off)];
        GPU_BARRIER();
        for (uint j = 0; j < QUANT_COLS; j++) sums[j * QUANT_GROUP + lane] = sums[j * QUANT_GROUP + lane] + other[j];
        GPU_BARRIER();
    }
    for (uint j = 0; j < QUANT_COLS; j++) acc[j] = sums[j * QUANT_GROUP + lane];
}

#define QUANT_KERNEL(TYPE, WEIGHTS, ELEMENTS, BYTES) \
kernel void osprey_matmul_##TYPE(GPU_GLOBAL const float *x GPU_BUFFER(0), \
                                 GPU_GLOBAL const uchar *w GPU_BUFFER(1), \
                                 GPU_GLOBAL float *out GPU_BUFFER(2), \
                                 GPU_PARAM rows GPU_BUFFER(3), GPU_PARAM inner GPU_BUFFER(4), \
                                 GPU_PARAM cols GPU_BUFFER(5) GPU_LANE_THREAD) { \
    GPU_LOCAL float sums[QUANT_COLS * QUANT_GROUP]; \
    ulong i = GPU_INDEX / QUANT_LANES, tiles = (rows + QUANT_ROWS - 1) / QUANT_ROWS, ctiles = (cols + QUANT_COLS - 1) / QUANT_COLS; \
    uint lane = (uint)(GPU_INDEX % QUANT_LANES), per_block = (ELEMENTS) / 8, per_span = QUANT_LANES / per_block; \
    bool live = i < tiles * ctiles; \
    ulong r0 = live ? (i / ctiles) * QUANT_ROWS : 0, c0 = live ? (i % ctiles) * QUANT_COLS : 0; \
    uint span = live ? (uint)(rows - r0 < QUANT_ROWS ? rows - r0 : QUANT_ROWS) : 0; \
    uint cspan = live ? (uint)(cols - c0 < QUANT_COLS ? cols - c0 : QUANT_COLS) : 0; \
    uint blocks = live ? (uint)(inner / (ELEMENTS)) : 0; \
    ulong stride = (ulong)(inner / (ELEMENTS)) * (BYTES); \
    GPU_GLOBAL const float *xr = x + r0 * inner; \
    GPU_GLOBAL const uchar *row = w + c0 * stride; \
    float acc[QUANT_ROWS][QUANT_COLS]; \
    for (uint k = 0; k < QUANT_ROWS; k++) \
        for (uint j = 0; j < QUANT_COLS; j++) acc[k][j] = 0.0f; \
    for (uint blk = lane / per_block; blk < blocks; blk += per_span) { \
        float wt[QUANT_COLS][8]; \
        for (uint j = 0; j < QUANT_COLS; j++) \
            if (j < cspan) WEIGHTS(row + j * stride + (ulong)blk * (BYTES), lane % per_block, wt[j]); \
        for (uint k = 0; k < QUANT_ROWS; k++) { \
            if (k < span) { \
                GPU_GLOBAL const float *xb = xr + k * inner + (ulong)blk * (ELEMENTS) + (lane % per_block) * 8; \
                float xv[8]; \
                for (uint m = 0; m < 8; m++) xv[m] = xb[m]; \
                for (uint j = 0; j < QUANT_COLS; j++) \
                    if (j < cspan) acc[k][j] = quant_lane(acc[k][j], xv, wt[j]); \
            } \
        } \
    } \
    for (uint k = 0; k < QUANT_ROWS; k++) { \
        quant_reduce(sums, GPU_LOCAL_INDEX, GPU_SIMD_WIDTH, acc[k]); \
        for (uint j = 0; j < QUANT_COLS; j++) \
            if (live && lane == 0 && k < span && j < cspan) out[(r0 + k) * cols + c0 + j] = acc[k][j]; \
    } \
}
QUANT_KERNEL(q4_k, q4_k_weights, 256, 144)
QUANT_KERNEL(q8_0, q8_0_weights, 32, 34)
QUANT_KERNEL(q6_k, q6_k_weights, 256, 210)
QUANT_KERNEL(q5_k, q5_k_weights, 256, 176)
QUANT_KERNEL(q3_k, q3_k_weights, 256, 110)
QUANT_KERNEL(iq4_nl, iq4_nl_weights, 32, 18)
QUANT_KERNEL(iq4_xs, iq4_xs_weights, 256, 136)
QUANT_KERNEL(iq3_s, iq3_s_weights, 256, 110)
#undef QUANT_KERNEL

// The many-row product's tile, which the host's thread count reads too.
#define QUANT_MM_ROWS_TILE 128
#define QUANT_MM_COLS_TILE 64
#define QUANT_MM_STEP 32

#if defined(__METAL_VERSION__)
// [LLM-MATMUL-QUANT] The many-row product (QUANT_MM_ROWS rows or more, a prompt
// chunk): each 256-thread group computes a 128-row by 64-column tile of `out`.
// Every k-step of 32 the group decodes the tile's 64 weight rows once into
// threadgroup memory beside its 128 input rows, then its eight SIMD groups
// multiply 8x8 float32 tiles in hardware, four by four tiles each. The hardware
// fuses and orders those sums itself, so this kernel is held to the host oracle
// within a recorded tolerance (gpu_device_tests.c) instead of bit for bit; the
// few-row kernel above keeps the order contract for decoding one token.
#define QUANT_MM_KERNEL(TYPE, WEIGHTS, ELEMENTS, BYTES) \
[[max_total_threads_per_threadgroup(QUANT_GROUP)]] \
kernel void osprey_matmul_##TYPE##_mm(device const float *x [[buffer(0)]], device const uchar *w [[buffer(1)]], \
                                      device float *out [[buffer(2)]], constant ulong &rows [[buffer(3)]], \
                                      constant ulong &inner [[buffer(4)]], constant ulong &cols [[buffer(5)]], \
                                      uint index [[thread_position_in_grid]], uint t [[thread_position_in_threadgroup]]) { \
    threadgroup float sa[QUANT_MM_ROWS_TILE * QUANT_MM_STEP], sb[QUANT_MM_COLS_TILE * QUANT_MM_STEP]; \
    ulong tile = index / QUANT_GROUP, ctiles = (cols + QUANT_MM_COLS_TILE - 1) / QUANT_MM_COLS_TILE; \
    ulong r0 = (tile / ctiles) * QUANT_MM_ROWS_TILE, c0 = (tile % ctiles) * QUANT_MM_COLS_TILE; \
    uint lr = t / 4, lg = t % 4, sg = t / 32, lane = t % 32, sr = (sg / 2) * 32, sc = (sg % 2) * 32; \
    ulong stride = (inner / (ELEMENTS)) * (BYTES), c = c0 + lr; \
    simdgroup_float8x8 acc[4][4]; \
    for (uint i = 0; i < 4; i++) \
        for (uint j = 0; j < 4; j++) acc[i][j] = make_filled_simdgroup_matrix<float, 8, 8>(0.0f); \
    for (ulong k0 = 0; k0 < inner; k0 += QUANT_MM_STEP) { \
        ulong k = k0 + lg * 8; \
        float wt[8]; \
        if (c < cols) WEIGHTS(w + c * stride + (k / (ELEMENTS)) * (BYTES), (uint)((k % (ELEMENTS)) / 8), wt); \
        for (uint m = 0; m < 8; m++) sb[lr * QUANT_MM_STEP + lg * 8 + m] = c < cols ? wt[m] : 0.0f; \
        for (uint h = 0; h < 2; h++) { \
            ulong r = r0 + lr + h * 64; \
            device const float4 *xp = (device const float4 *)(x + r * inner + k); \
            float4 lo = r < rows ? xp[0] : float4(0.0f), hi = r < rows ? xp[1] : float4(0.0f); \
            threadgroup float4 *sp = (threadgroup float4 *)(sa + (lr + h * 64) * QUANT_MM_STEP + lg * 8); \
            sp[0] = lo; \
            sp[1] = hi; \
        } \
        threadgroup_barrier(mem_flags::mem_threadgroup); \
        for (uint kk = 0; kk < QUANT_MM_STEP; kk += 8) { \
            simdgroup_float8x8 a[4], b[4]; \
            for (uint i = 0; i < 4; i++) simdgroup_load(a[i], sa + (sr + 8 * i) * QUANT_MM_STEP + kk, QUANT_MM_STEP); \
            for (uint j = 0; j < 4; j++) \
                simdgroup_load(b[j], sb + (sc + 8 * j) * QUANT_MM_STEP + kk, QUANT_MM_STEP, ulong2(0, 0), true); \
            for (uint i = 0; i < 4; i++) \
                for (uint j = 0; j < 4; j++) simdgroup_multiply_accumulate(acc[i][j], a[i], b[j], acc[i][j]); \
        } \
        threadgroup_barrier(mem_flags::mem_threadgroup); \
    } \
    threadgroup float *own = sa + sg * 64; \
    for (uint i = 0; i < 4; i++) \
        for (uint j = 0; j < 4; j++) { \
            simdgroup_store(acc[i][j], own, 8); \
            simdgroup_barrier(mem_flags::mem_threadgroup); \
            for (uint e = lane; e < 64; e += 32) { \
                ulong orow = r0 + sr + 8 * i + e / 8, ocol = c0 + sc + 8 * j + e % 8; \
                if (orow < rows && ocol < cols) out[orow * cols + ocol] = own[e]; \
            } \
            simdgroup_barrier(mem_flags::mem_threadgroup); \
        } \
}
QUANT_MM_KERNEL(q4_k, q4_k_weights, 256, 144)
QUANT_MM_KERNEL(q8_0, q8_0_weights, 32, 34)
QUANT_MM_KERNEL(q6_k, q6_k_weights, 256, 210)
QUANT_MM_KERNEL(q5_k, q5_k_weights, 256, 176)
QUANT_MM_KERNEL(q3_k, q3_k_weights, 256, 110)
QUANT_MM_KERNEL(iq4_nl, iq4_nl_weights, 32, 18)
QUANT_MM_KERNEL(iq4_xs, iq4_xs_weights, 256, 136)
QUANT_MM_KERNEL(iq3_s, iq3_s_weights, 256, 110)
#undef QUANT_MM_KERNEL
#endif

// [LLM-DELTA-RULE] `steps` tokens of the recurrent gated delta rule, fused into
// one kernel ([LLM-PREFILL]): thread t is value element v = t % dim of head
// h = t / dim and owns column v of the [heads, dim, dim] state outright, so it
// steps through the tokens with no other thread to wait for. Each step it decays
// its column, recalls through that token's keys, writes the updated column and
// reads it out through that token's queries. `out` holds the final state then
// the steps * heads * dim outputs in token order, and every product is rounded
// before its add, as the host oracle in gpu_host.c does.
kernel void osprey_delta_rule(GPU_GLOBAL const float *state GPU_BUFFER(0), GPU_GLOBAL const float *queries GPU_BUFFER(1),
                              GPU_GLOBAL const float *keys GPU_BUFFER(2), GPU_GLOBAL const float *values GPU_BUFFER(3),
                              GPU_GLOBAL const float *beta GPU_BUFFER(4), GPU_GLOBAL const float *decay GPU_BUFFER(5),
                              GPU_GLOBAL float *out GPU_BUFFER(6), GPU_PARAM heads GPU_BUFFER(7),
                              GPU_PARAM dim GPU_BUFFER(8), GPU_PARAM steps GPU_BUFFER(9) GPU_THREAD) {
    uint t = (uint)GPU_INDEX, d = (uint)dim, cells = (uint)(heads * dim);
    if ((ulong)t >= heads * dim) return;
    uint h = t / d, v = t % d, plane = h * d * d;
    GPU_GLOBAL const float *column = state;
    for (uint s = 0; s < (uint)steps; s++) {
        GPU_GLOBAL const float *q = queries + s * cells, *k = keys + s * cells;
        float fade = decay[s * (uint)heads + h], recalled = 0.0f;
        for (uint i = 0; i < d; i++) {
            float kept = column[plane + i * d + v] * fade;
            float product = kept * k[h * d + i];
            recalled = recalled + product;
        }
        float delta = (values[s * cells + t] - recalled) * beta[s * (uint)heads + h], read = 0.0f;
        for (uint i = 0; i < d; i++) {
            float kept = column[plane + i * d + v] * fade;
            float update = k[h * d + i] * delta;
            float next = kept + update;
            out[plane + i * d + v] = next;
            float product = next * q[h * d + i];
            read = read + product;
        }
        out[(uint)(heads * dim * dim) + s * cells + t] = read;
        column = out;
    }
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
