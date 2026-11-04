// Implements [BUILTIN-MATH] on the device [GPU-BACKEND-DEVICE]: exp, ln and
// tanh over binary64 words, evaluated in 128-bit fixed point with about 115
// significant bits and then rounded once, so the result is the correctly
// rounded double in every case short of a rounding boundary closer than
// 2^-115 — closer than the known worst cases of these functions. Follows the
// arithmetic core in gpu_f64.metal in the embedded shader source.

typedef struct { ulong hi; ulong lo; } U128;
// A fixed-point magnitude with `scale` fractional bits: value = word / 2^scale.
typedef struct { U128 word; int scale; } Fixed;

constant ulong LN2_HI = 0x00b17217f7d1cf79ul;   // ln 2 * 2^120, high limb
constant ulong LN2_LO = 0xabc9e3b39803f2f6ul;   // ln 2 * 2^120, low limb
constant ulong LN2_EXT = 0xaf40f343267298b6ul;  // bits 121..184 of ln 2
constant ulong LN2_Q116_HI = 0x000b17217f7d1cf7ul;
constant ulong LN2_Q116_LO = 0x9abc9e3b39803f2ful;
constant ulong INV_LN2_Q62 = 0x5c551d94ae0bf85dul;
constant ulong SQRT2_MANTISSA = 0x16a09e667f3bccul;   // sqrt 2 * 2^52
constant ulong TANH_SATURATES = 0x40330fc504816f00ul; // 19.0616: tanh rounds to 1
constant uint Q120 = 120;

U128 u128_add(U128 a, U128 b) {
    ulong lo = a.lo + b.lo;
    return (U128){a.hi + b.hi + (ulong)(lo < a.lo), lo};
}
U128 u128_sub(U128 a, U128 b) {
    return (U128){a.hi - b.hi - (ulong)(a.lo < b.lo), a.lo - b.lo};
}
bool u128_lt(U128 a, U128 b) { return a.hi < b.hi || (a.hi == b.hi && a.lo < b.lo); }
bool u128_zero(U128 a) { return !(a.hi | a.lo); }
uint u128_clz(U128 a) { return a.hi ? (uint)clz(a.hi) : 64u + (uint)clz(a.lo); }
U128 u128_shl(U128 a, uint n) {
    if (!n) return a;
    if (n >= 64) return (U128){a.lo << (n - 64), 0};
    return (U128){(a.hi << n) | (a.lo >> (64 - n)), a.lo << n};
}
U128 u128_shr(U128 a, uint n) {
    if (!n) return a;
    if (n >= 64) return (U128){0, a.hi >> (n - 64)};
    return (U128){a.hi >> n, (a.lo >> n) | (a.hi << (64 - n))};
}
U128 u128_bit(uint n) { return u128_shl((U128){0, 1}, n); }

// The 128 bits of the 256-bit product starting at `shift` (64..192):
// floor(a * b / 2^shift) when that fits.
U128 product_window(ulong p0, ulong p1, ulong p2, ulong p3, uint shift) {
    uint word = shift / 64, bit = shift % 64;
    ulong w0 = word == 0 ? p0 : word == 1 ? p1 : word == 2 ? p2 : p3;
    ulong w1 = word == 0 ? p1 : word == 1 ? p2 : word == 2 ? p3 : 0ul;
    ulong w2 = word == 0 ? p2 : word == 1 ? p3 : 0ul;
    if (!bit) return (U128){w1, w0};
    return (U128){(w1 >> bit) | (w2 << (64 - bit)), (w0 >> bit) | (w1 << (64 - bit))};
}

U128 u128_mul_shift(U128 a, U128 b, uint shift) {
    Wide ll = wide_product(a.lo, b.lo), lh = wide_product(a.lo, b.hi);
    Wide hl = wide_product(a.hi, b.lo), hh = wide_product(a.hi, b.hi);
    ulong p1 = ll.high + lh.low;
    ulong carry = (ulong)(p1 < ll.high);
    ulong p1b = p1 + hl.low;
    carry += (ulong)(p1b < p1);
    ulong p2 = lh.high + hl.high;
    ulong carry2 = (ulong)(p2 < lh.high);
    ulong p2b = p2 + hh.low;
    carry2 += (ulong)(p2b < p2);
    ulong p2c = p2b + carry;
    carry2 += (ulong)(p2c < p2b);
    return product_window(ll.low, p1b, p2c, hh.high + carry2, shift);
}

// floor(a / n) for n < 2^32, by 32-bit limbs.
U128 u128_div_small(U128 a, ulong n) {
    ulong limbs[4] = {a.hi >> 32, a.hi & 0xfffffffful, a.lo >> 32, a.lo & 0xfffffffful};
    ulong rem = 0;
    for (uint i = 0; i < 4; i++) {
        ulong cur = (rem << 32) | limbs[i];
        limbs[i] = cur / n;
        rem = cur % n;
    }
    return (U128){(limbs[0] << 32) | limbs[1], (limbs[2] << 32) | limbs[3]};
}

// floor(n * 2^shift / d) by restoring division; n and d below 2^127.
U128 u128_div_shift(U128 n, U128 d, uint shift) {
    U128 q = {0, 0}, rem = n;
    for (uint i = 0; i <= shift; i++) {
        q = u128_shl(q, 1);
        if (!u128_lt(rem, d)) { rem = u128_sub(rem, d); q.lo |= 1; }
        if (i < shift) rem = u128_shl(rem, 1);
    }
    return q;
}

// A fixed-point magnitude as the nearest double: the top 56 bits round, the
// rest is sticky.
ulong fixed_to_double(ulong sign, U128 value, int scale) {
    if (u128_zero(value)) return sign;
    uint leading = u128_clz(value);
    int top = 127 - (int)leading;
    int drop = top - 55;
    if (drop <= 0) return rounded(sign, top + scale, u128_shl(value, (uint)(-drop)).lo);
    U128 kept = u128_shr(value, (uint)drop);
    U128 lost = u128_sub(value, u128_shl(kept, (uint)drop));
    return rounded(sign, top + scale, kept.lo | (ulong)(!u128_zero(lost)));
}

// k * ln 2 as Q8.120, modulo 2^128, with the next 64 bits of ln 2 carried in.
U128 k_ln2(ulong k) {
    Wide low = wide_product(k, LN2_LO);
    Wide ext = wide_product(k, LN2_EXT);
    return u128_add((U128){k * LN2_HI + low.high, low.low}, (U128){0, ext.high});
}

// sum of r^n / n! with the sign of r applied to the odd terms, Q8.120.
U128 exp_series(U128 r, bool negative) {
    U128 sum = u128_bit(Q120), term = sum;
    for (ulong n = 1; n < 64; n++) {
        term = u128_div_small(u128_mul_shift(term, r, Q120), n);
        if (u128_zero(term)) break;
        sum = (negative && (n & 1)) ? u128_sub(sum, term) : u128_add(sum, term);
    }
    return sum;
}

// e^x for finite x with 2^-55 <= |x| < 1024, as word * 2^(scale - 120):
// x = k ln 2 + r with |r| <= ln 2 / 2, word = e^r in Q8.120 and scale = k.
Fixed exp_core(ulong x) {
    Parts p = parts(x);
    bool negative = (x & SIGN) != 0;
    ulong scaled = p.exponent >= 0 ? p.mantissa << (uint)p.exponent : p.mantissa >> (uint)(-p.exponent);
    ulong k = (wide_product(scaled, INV_LN2_Q62).high + (1ul << 49)) >> 50;
    U128 r = u128_sub(u128_shl((U128){0, p.mantissa}, (uint)(p.exponent + 68)), k_ln2(k));
    bool below = (r.hi >> 63) != 0;
    if (below) r = u128_sub((U128){0, 0}, r);
    return (Fixed){exp_series(r, below != negative), negative ? -(int)k : (int)k};
}

ulong exp64(ulong x) {
    if (is_nan(x)) return x | QUIET;
    if (x == INF) return INF;
    if (x == (INF | SIGN)) return 0;
    Parts p = parts(x);
    if (p.exponent >= 10) return (x & SIGN) ? 0ul : INF;
    if (is_zero(x) || p.exponent < -55) return F64_ONE;
    Fixed e = exp_core(x);
    return fixed_to_double(0, e.word, e.scale - (int)Q120);
}

// u + u^3/3 + u^5/5 + ... at `scale` fractional bits.
U128 atanh_series(U128 u, uint scale) {
    U128 square = u128_mul_shift(u, u, scale), term = u, sum = u;
    for (ulong n = 3; n < 256; n += 2) {
        term = u128_mul_shift(term, square, scale);
        U128 part = u128_div_small(term, n);
        if (u128_zero(part)) break;
        sum = u128_add(sum, part);
    }
    return sum;
}

// |e| ln 2 at scale 116, modulo 2^128.
U128 e_ln2(ulong e) {
    Wide low = wide_product(e, LN2_Q116_LO);
    return (U128){e * LN2_Q116_HI + low.high, low.low};
}

// a - b with signs, for magnitudes at one scale: the result's sign and word.
Fixed signed_sum(U128 a, bool a_negative, U128 b, bool b_negative, int scale) {
    if (a_negative == b_negative) return (Fixed){u128_add(a, b), a_negative ? -scale : scale};
    bool a_larger = u128_lt(b, a);
    U128 word = a_larger ? u128_sub(a, b) : u128_sub(b, a);
    return (Fixed){word, (a_larger ? a_negative : b_negative) ? -scale : scale};
}

// ln x for finite positive x: with f = x / 2^e in [sqrt(1/2), sqrt 2) and
// d = f - 1, ln x = e ln 2 + 2 atanh(d / (2 + d)). When e is 0 the series runs
// at a scale aligned to d, so a result near 0 keeps its full relative precision.
ulong ln_finite(ulong x) {
    Parts p = parts(x);
    uint fbits = p.mantissa > SQRT2_MANTISSA ? 53 : 52;
    int e = p.exponent + (int)(fbits - 52);
    bool d_negative = p.mantissa < (1ul << fbits);
    ulong d = d_negative ? (1ul << fbits) - p.mantissa : p.mantissa - (1ul << fbits);
    ulong denominator = d_negative ? (1ul << (fbits + 1)) - d : (1ul << (fbits + 1)) + d;
    uint scale = e ? 116 : Q120 + fbits - (63 - (uint)clz(d));
    U128 u = u128_div_shift((U128){0, d}, (U128){0, denominator}, scale);
    U128 twice = u128_shl(atanh_series(u, scale), 1);
    if (!e) return fixed_to_double(d_negative ? SIGN : 0ul, twice, -(int)scale);
    Fixed total = signed_sum(e_ln2((ulong)(e < 0 ? -e : e)), e < 0, twice, d_negative, 116);
    return fixed_to_double(total.scale < 0 ? SIGN : 0ul, total.word, -116);
}

ulong ln64(ulong x) {
    if (is_nan(x)) return x | QUIET;
    if (is_zero(x)) return INF | SIGN;
    if (x & SIGN) return nan_result(0, 0);
    if (x == INF) return INF;
    if (x == F64_ONE) return 0;
    return ln_finite(x);
}

// tanh |x| for 1 <= |x| < 19.0616 as 1 - 2 / (e^(2|x|) + 1), Q8.120.
U128 tanh_large(ulong ax) {
    Fixed e = exp_core(ax + ONE);
    uint unit = Q120 - (uint)e.scale;
    U128 denominator = u128_add(e.word, u128_bit(unit));
    return u128_sub(u128_bit(Q120), u128_div_shift(u128_bit(unit + 1), denominator, Q120));
}

// tanh |x| for 2^-27 <= |x| < 1 as m / (m + 2) with m = e^(2|x|) - 1 summed
// at a scale aligned to 2|x|, so the result keeps its relative precision.
Fixed tanh_small(ulong ax) {
    Parts p = parts(ax);
    uint scale = Q120 - (uint)(p.exponent + 1);
    U128 r = u128_shl((U128){0, p.mantissa}, 68), term = r, sum = r;
    for (ulong n = 2; n < 64; n++) {
        term = u128_div_small(u128_mul_shift(term, r, scale), n);
        if (u128_zero(term)) break;
        sum = u128_add(sum, term);
    }
    U128 denominator = u128_add(u128_bit(Q120 + 1), u128_shr(sum, scale - Q120));
    return (Fixed){u128_div_shift(sum, denominator, Q120), (int)scale};
}

ulong tanh64(ulong x) {
    if (is_nan(x)) return x | QUIET;
    ulong sign = x & SIGN, ax = x & ~SIGN;
    if (ax >= TANH_SATURATES) return sign | F64_ONE;
    if (is_zero(x) || parts(ax).exponent < -27) return x;
    if (parts(ax).exponent >= 0) return fixed_to_double(sign, tanh_large(ax), -(int)Q120);
    Fixed t = tanh_small(ax);
    return fixed_to_double(sign, t.word, -t.scale);
}
