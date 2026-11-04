// Device support for the kernels the compiler emits [GPU-BACKEND-DEVICE] and
// the builtin product [GPU-MATMUL]. A kernel cannot reach a runtime `Arith`
// policy, so a fallible integer operation or a division by zero records a
// fault word instead; the runtime aborts the program naming the kernel when a
// dispatch completes with a fault ([GPU-DEVICE-FAULT]). Follows gpu_f64.metal
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
