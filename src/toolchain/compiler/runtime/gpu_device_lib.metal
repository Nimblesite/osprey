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
constant ulong INTEGER_SIGN##SUFFIX = 1ul << (BITS - 1); \
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
        if (!fraction) return gpu_float32_from_bits(sign);
        uint significand = fraction;
        uint e = 113u;
        while (!(significand & 0x0400u)) { significand <<= 1; e--; }
        word = sign | (e << 23) | ((significand & 0x03ffu) << 13);
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
    ulong significand = fraction | (exponent ? (1ul << 52) : 0ul);
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

// Precise Metal exp/tanh still differ by one binary32 ulp from the host.
// Use the shared wider approximation, then round once at the result boundary;
// surrounding tensor arithmetic and storage remain native binary32.
#define ROUNDED_UNARY(NAME) float gpu32_##NAME(float x) { \
    return gpu_float32_from_bits(narrow32(NAME##64(widen32(gpu_float32_bits(x))))); \
}
ROUNDED_UNARY(exp)
ROUNDED_UNARY(tanh)
#undef ROUNDED_UNARY

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
#endif

// The 6-bit scale and minimum of sub-block j, packed as ggml's
// `get_scale_min_k4` packs them, shared by Q4_K and Q5_K.
uint q4k_scale(GPU_GLOBAL const uchar *scales, uint j) {
    return j < 4 ? (uint)(scales[j] & 63) : (uint)((scales[j + 4] & 15) | ((scales[j - 4] >> 6) << 4));
}
uint q4k_min(GPU_GLOBAL const uchar *scales, uint j) {
    return j < 4 ? (uint)(scales[j + 4] & 63) : (uint)((scales[j + 4] >> 4) | ((scales[j] >> 6) << 4));
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

// Q4_K: 256 weights per 144-byte block, a binary16 scale and minimum, eight
// 6-bit sub-block scales and minimums, 4-bit weights.
float q4_k_step(float acc, GPU_GLOBAL const float *x, GPU_GLOBAL const uchar *b) {
    float d = quant_half(b, 0), dmin = quant_half(b, 2);
    for (uint j = 0; j < 8; j++) {
        uint sc = q4k_scale(b + 4, j), mn = q4k_min(b + 4, j);
        float d1 = d * (float)sc;
        float m1 = dmin * (float)mn;
        uint g = j >> 1, high = j & 1;
        GPU_GLOBAL const uchar *q = b + 16 + g * 32;
        GPU_GLOBAL const float *xe = x + g * 64 + high * 32;
        for (uint l = 0; l < 32; l++) {
            uint nib = high ? (uint)(q[l] >> 4) : (uint)(q[l] & 15);
            float weight = d1 * (float)nib;
            weight = weight - m1;
            acc = quant_add(acc, xe[l], weight);
        }
    }
    return acc;
}

// Q6_K: 256 weights per 210-byte block, six bits per weight from a low nibble and
// two mask bits, a binary16 scale and sixteen 8-bit sub-block scales.
float q6_k_step(float acc, GPU_GLOBAL const float *x, GPU_GLOBAL const uchar *b) {
    float d = quant_half(b, 208);
    for (uint j = 0; j < 256; j++) {
        uint group = j >> 7, quarter = (j & 127) >> 5, l = j & 31;
        uint low = (uint)(b[64 * group + l + 32 * (quarter & 1)] >> (4 * (quarter >> 1))) & 15;
        uint q = low | (((uint)(b[128 + 32 * group + l] >> (2 * quarter)) & 3) << 4);
        float scale = (float)(signed char)b[192 + 8 * group + (l >> 4) + 2 * quarter];
        acc = quant_add(acc, x[j], d * scale * (float)((int)q - 32));
    }
    return acc;
}

// Q8_0: 32 weights per 34-byte block, one binary16 scale and one signed byte per weight.
float q8_0_step(float acc, GPU_GLOBAL const float *x, GPU_GLOBAL const uchar *b) {
    float d = quant_half(b, 0);
    for (uint j = 0; j < 32; j++)
        acc = quant_add(acc, x[j], d * (float)(signed char)b[2 + j]);
    return acc;
}

// Q5_K: 256 weights per 176-byte block, Q4_K's four low bits plus a high-bit mask,
// a binary16 scale and minimum and eight 6-bit sub-block scales and minimums.
float q5_k_step(float acc, GPU_GLOBAL const float *x, GPU_GLOBAL const uchar *b) {
    float d = quant_half(b, 0), dmin = quant_half(b, 2);
    for (uint g = 0; g < 4; g++) {
        for (uint h = 0; h < 2; h++) {
            uint sub = 2 * g + h;
            uint sc = q4k_scale(b + 4, sub), mn = q4k_min(b + 4, sub);
            float d1 = d * (float)sc;
            float m1 = dmin * (float)mn;
            GPU_GLOBAL const uchar *ql = b + 48 + 32 * g;
            for (uint l = 0; l < 32; l++) {
                uint nib = h ? (uint)(ql[l] >> 4) : (uint)(ql[l] & 15);
                uint high = (uint)((b[16 + l] >> sub) & 1);
                float weight = d1 * (float)(nib + 16 * high);
                weight = weight - m1;
                acc = quant_add(acc, x[g * 64 + h * 32 + l], weight);
            }
        }
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
float q3_k_step(float acc, GPU_GLOBAL const float *x, GPU_GLOBAL const uchar *b) {
    float d = quant_half(b, 108);
    for (uint g = 0; g < 2; g++) {
        for (uint j = 0; j < 4; j++) {
            for (uint side = 0; side < 2; side++) {
                uint sc = q3k_scale(b, 8 * g + 2 * j + side);
                float dl = d * (float)((int)sc - 32);
                GPU_GLOBAL const uchar *q = b + 32 + 32 * g + 16 * side;
                GPU_GLOBAL const uchar *hm = b + 16 * side;
                for (uint l = 0; l < 16; l++) {
                    int low = (int)((q[l] >> (2 * j)) & 3);
                    int high = (int)((hm[l] >> (4 * g + j)) & 1);
                    acc = quant_add(acc, x[g * 128 + j * 32 + side * 16 + l], dl * (float)(low - (high ? 0 : 4)));
                }
            }
        }
    }
    return acc;
}

// ggml's `kvalues_iq4nl`: the sixteen non-linear levels IQ4_NL and IQ4_XS use
// (ggml-common.h at the commit the gguf port pins).
constant int IQ4_NL_LEVELS[16] = {-127, -104, -83, -65, -49, -35, -22, -10, 1, 13, 25, 38, 53, 69, 89, 113};

// IQ4_NL: 32 weights per 18-byte block, one of the sixteen non-linear levels per
// weight and a binary16 scale.
float iq4_nl_step(float acc, GPU_GLOBAL const float *x, GPU_GLOBAL const uchar *b) {
    float d = quant_half(b, 0);
    for (uint j = 0; j < 32; j++) {
        uint level = j < 16 ? (uint)(b[2 + j] & 15) : (uint)(b[2 + j - 16] >> 4);
        acc = quant_add(acc, x[j], d * (float)IQ4_NL_LEVELS[level]);
    }
    return acc;
}

// IQ4_XS: 256 weights per 136-byte block, IQ4_NL's levels with a six-bit
// sub-block scale and a binary16 block scale.
float iq4_xs_step(float acc, GPU_GLOBAL const float *x, GPU_GLOBAL const uchar *b) {
    float d = quant_half(b, 0);
    uint scales_h = (uint)(b[2] | (b[3] << 8));
    for (uint ib = 0; ib < 8; ib++) {
        uint ls = (uint)((b[4 + (ib >> 1)] >> (4 * (ib & 1))) & 15) | (((scales_h >> (2 * ib)) & 3) << 4);
        float dl = d * (float)((int)ls - 32);
        for (uint j = 0; j < 32; j++) {
            uint level = j < 16 ? (uint)(b[8 + 16 * ib + j] & 15) : (uint)(b[8 + 16 * ib + j - 16] >> 4);
            acc = quant_add(acc, x[32 * ib + j], dl * (float)IQ4_NL_LEVELS[level]);
        }
    }
    return acc;
}

// Thread t computes part p = t % parts of output i = t / parts: the blocks
// from p * blocks / parts up to (p + 1) * blocks / parts, in ggml's block order
// from 0.0f, into partial[i * parts + p]. One template for every block format,
// so the host oracle in gpu_host.c runs the same loop shape over the same
// per-block steps. osprey_sum_parts then adds the parts of each output in order.
#define QUANT_KERNEL(TYPE, STEP, ELEMENTS, BYTES) \
kernel void osprey_matmul_##TYPE(GPU_GLOBAL const float *x GPU_BUFFER(0), \
                                 GPU_GLOBAL const uchar *w GPU_BUFFER(1), \
                                 GPU_GLOBAL float *partial GPU_BUFFER(2), \
                                 GPU_PARAM rows GPU_BUFFER(3), GPU_PARAM inner GPU_BUFFER(4), \
                                 GPU_PARAM cols GPU_BUFFER(5), GPU_PARAM parts GPU_BUFFER(6) GPU_THREAD) { \
    ulong t = GPU_INDEX; \
    if (t >= rows * cols * parts) return; \
    ulong i = t / parts, p = t % parts; \
    ulong r = i / cols, c = i % cols; \
    uint blocks = (uint)(inner / (ELEMENTS)); \
    uint first = (uint)(p * blocks / parts), last = (uint)((p + 1) * blocks / parts); \
    GPU_GLOBAL const float *xr = x + r * inner; \
    GPU_GLOBAL const uchar *row = w + c * (ulong)blocks * (BYTES); \
    float acc = 0.0f; \
    for (uint blk = first; blk < last; blk++) \
        acc = STEP(acc, xr + (ulong)blk * (ELEMENTS), row + (ulong)blk * (BYTES)); \
    partial[t] = acc; \
}
QUANT_KERNEL(q4_k, q4_k_step, 256, 144)
QUANT_KERNEL(q8_0, q8_0_step, 32, 34)
QUANT_KERNEL(q6_k, q6_k_step, 256, 210)
QUANT_KERNEL(q5_k, q5_k_step, 256, 176)
QUANT_KERNEL(q3_k, q3_k_step, 256, 110)
QUANT_KERNEL(iq4_nl, iq4_nl_step, 32, 18)
QUANT_KERNEL(iq4_xs, iq4_xs_step, 256, 136)
#undef QUANT_KERNEL

// out[i] = the `parts` partial sums of output i added in order from 0.0f.
kernel void osprey_sum_parts(GPU_GLOBAL const float *partial GPU_BUFFER(0),
                             GPU_GLOBAL float *out GPU_BUFFER(1),
                             GPU_PARAM count GPU_BUFFER(2), GPU_PARAM parts GPU_BUFFER(3) GPU_THREAD) {
    ulong i = GPU_INDEX;
    if (i >= count) return;
    float acc = 0.0f;
    for (ulong p = 0; p < parts; p++) acc = acc + partial[i * parts + p];
    out[i] = acc;
}
