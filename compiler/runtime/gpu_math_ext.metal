// Implements [BUILTIN-MATH-LOG], [BUILTIN-MATH-ROUNDING], [BUILTIN-MATH-TRIG],
// [BUILTIN-MATH-HYPERBOLIC] and [BUILTIN-MATH-SPECIAL] on the device
// [GPU-BACKEND-DEVICE]: the binary float functions and the remaining unary
// families over binary64 words, built on the 128-bit fixed-point core of
// gpu_math64.metal. Extended unary tests allow 16 ulps against the rounded
// host long-double reference; pow, atan2 and hypot allow 64 against host
// double libm. These sampled bounds do not prove universal correct rounding.

// ---- Constants ----

constant ulong TWOPI_HI = 0x3fe45f306dc9c883ul;     // 2/pi, high part
constant ulong PI2_HI = 0x3ff921fb54442d18ul;       // pi/2, high part
constant ulong PI2_LO = 0x3c91a62633145c07ul;       // pi/2, low part
constant ulong PI = 0x400921fb54442d18ul;           // pi
constant ulong TWO_OVER_SQRT_PI = 0x3ff20dd750429b6dul; // 2/sqrt(pi)

// ---- Shared ln/exp scaffolding ----

// [BUILTIN-MATH-LOG] log2(x) = ln(x) / ln 2, composed from the verified ln64.
ulong log264(ulong x) {
    if (is_nan(x)) return x | QUIET;
    if (is_zero(x)) return INF | SIGN;
    if (x & SIGN) return nan_result(0, 0);
    if (x == INF) return INF;
    if (x == F64_ONE) return 0;
    return multiply64(ln64(x), 0x3ff71547652b82feul);   // 1/ln 2
}

// [BUILTIN-MATH-LOG] log10(x) = ln(x) / ln 10.
ulong log1064(ulong x) {
    if (is_nan(x)) return x | QUIET;
    if (is_zero(x)) return INF | SIGN;
    if (x & SIGN) return nan_result(0, 0);
    if (x == INF) return INF;
    if (x == F64_ONE) return 0;
    return multiply64(ln64(x), 0x3fdbcb7b1526e50dul);   // 1/ln 10
}

// [BUILTIN-MATH-LOG] expm1(x) = e^x - 1 without cancellation: the 128-bit
// e^x - 1 for x >= 0, 1 - e^x for x < 0.
ulong expm164(ulong x) {
    if (is_nan(x)) return x | QUIET;
    if (is_zero(x)) return x;
    if (x == INF) return INF;
    if (x == (INF | SIGN)) return 0xbff0000000000000ul;   // -1.0
    if (parts(x & ~SIGN).exponent < -52) return x;
    if (parts(x & ~SIGN).exponent >= 10) return (x & SIGN) ? 0xbff0000000000000ul : INF;
    Fixed a = exp_core(x);
    if (!(x & SIGN)) {
        int k = a.scale;
        if (k >= 64) return exp64(x);
        U128 diff = u128_sub(a.word, u128_bit((uint)(120 - k)));
        return fixed_to_double(0, diff, k - 120);
    }
    uint shift = (uint)(-a.scale);
    U128 shifted = shift >= 128 ? (U128){0, 0} : u128_shr(a.word, shift);
    U128 diff = u128_sub(u128_bit(120), shifted);
    return fixed_to_double(SIGN, diff, -120);
}

// The log1p series x - x^2/2 + x^3/3 - ... (or -x - x^2/2 - ... for x < 0),
// at Q120, for |x| <= 1/4.
static ulong log1p_series(ulong ax, bool negative) {
    Parts p = parts(ax);
    U128 u = u128_shl((U128){0, p.mantissa}, (uint)(p.exponent + 68));
    U128 term = u, sum = u;
    for (ulong n = 2; n < 64; n++) {
        term = u128_mul_shift(term, u, Q120);
        U128 part = u128_div_small(term, n);
        if (u128_zero(part)) break;
        bool add = negative ? true : ((n & 1) != 0);
        sum = add ? u128_add(sum, part) : u128_sub(sum, part);
    }
    return fixed_to_double(negative ? SIGN : 0, sum, -120);
}

// [BUILTIN-MATH-LOG] log1p(x) = ln(1 + x) without cancellation.
ulong log1p64(ulong x) {
    if (is_nan(x)) return x | QUIET;
    if (is_zero(x)) return x;
    if (x == INF) return INF;
    if (x == (INF | SIGN)) return nan_result(0, 0);
    ulong ax = x & ~SIGN;
    if ((x & SIGN) && ax > F64_ONE) return nan_result(0, 0);   // x < -1
    if ((x & SIGN) && ax == F64_ONE) return INF | SIGN;         // x = -1
    if (parts(ax).exponent < -52) return x;
    if (parts(ax).exponent <= -3) return log1p_series(ax, (x & SIGN) != 0);
    return ln64(add64(F64_ONE, x));
}

// Whether the double word holds an exact integer, and its low bit.
static bool double_is_integer(ulong y) {
    if (is_inf(y)) return false;
    Parts p = parts(y);
    if (p.exponent >= 52) return true;
    if (p.exponent < 0) return false;
    return (p.mantissa & ((ONE >> (uint)p.exponent) - 1ul)) == 0;
}

// [BUILTIN-MATH-ROUNDING] pow(x, y) with C semantics. The general positive
// case is exp(y ln x) at double precision; special bases and integer
// exponents follow C's rules exactly.
ulong pow64(ulong x, ulong y) {
    if (is_zero(y) || x == F64_ONE) return F64_ONE;
    if (is_nan(x) || is_nan(y)) return nan_result(x, y);
    bool y_neg = (y & SIGN) != 0;
    bool y_integer = double_is_integer(y);
    bool x_neg = (x & SIGN) != 0;
    ulong ax = x & ~SIGN;
    if (is_inf(y)) {
        if (ax > F64_ONE) return y_neg ? 0 : INF;
        if (ax < F64_ONE) return y_neg ? INF : 0;
        return F64_ONE;
    }
    bool odd = x_neg && y_integer && parts(y).exponent < 53 && (to_long(y) & 1);
    if (is_zero(ax)) {
        if (y_neg) return odd ? (INF | SIGN) : INF;
        return odd ? x : 0;
    }
    if (ax == INF) {
        if (y_neg) return odd ? (0x8000000000000000ul) : 0;
        return odd ? (INF | SIGN) : INF;
    }
    if (x_neg && !y_integer) return nan_result(0, 0);
    // General: x^y = exp(y ln |x|); the sign is folded from the odd exponent.
    // exp64 itself returns the correct infinity/zero on overflow/underflow.
    ulong z = multiply64(y, ln64(ax));
    ulong result = exp64(z);
    return odd ? (result | SIGN) : result;
}

// [BUILTIN-MATH-ROUNDING] hypot(x, y) = sqrt(x^2 + y^2) without overflow.
ulong hypot64(ulong x, ulong y) {
    if (is_inf(x) || is_inf(y)) return INF;
    if (is_nan(x) || is_nan(y)) return nan_result(x, y);
    if (is_zero(x)) return abs64(y);
    if (is_zero(y)) return abs64(x);
    ulong ax = abs64(x), ay = abs64(y);
    ulong big = lt64(ax, ay) ? ay : ax;
    ulong small = lt64(ax, ay) ? ax : ay;
    ulong t = divide64(small, big);
    ulong s = sqrt64(add64(F64_ONE, multiply64(t, t)));
    return multiply64(big, s);
}

// ---- Hyperbolic functions [BUILTIN-MATH-HYPERBOLIC] ----

// (e^|x| +- e^-|x|)/2 with e^r at Q120 and scale k = round(|x|/ln 2) >= 0:
// the result is (a.word +- b.word >> 2k) / 2^(121-k), cancellation-free.
static ulong sinh_cosh(ulong x, bool plus) {
    if (is_nan(x)) return x | QUIET;
    ulong sign = x & SIGN, ax = x & ~SIGN;
    if (is_zero(x)) return plus ? F64_ONE : x;
    if (ax == INF) return plus ? INF : x;
    if (parts(ax).exponent >= 10) return plus ? INF : (sign | INF);
    if (parts(ax).exponent < -27) return plus ? F64_ONE : x;
    Fixed a = exp_core(ax);
    Fixed b = exp_core(ax | SIGN);
    int k = a.scale;
    U128 shifted = (2 * k >= 128) ? (U128){0, 0} : u128_shr(b.word, (uint)(2 * k));
    U128 sum = plus ? u128_add(a.word, shifted) : u128_sub(a.word, shifted);
    return fixed_to_double(plus ? 0 : sign, sum, k - 121);
}

ulong sinh64(ulong x) { return sinh_cosh(x, false); }
ulong cosh64(ulong x) { return sinh_cosh(x, true); }

// asinh(x) = x - x^3/6 + 3x^5/40 - ... for |x| < 1/2, where ln(x+sqrt(1+x^2))
// loses x's low bits in the sqrt(1+x^2) rounding.
static ulong asinh_series(ulong ax) {
    ulong square = multiply64(ax, ax);
    ulong term = ax, sum = ax;
    for (long n = 1; n < 40; n += 2) {
        ulong factor = divide64(multiply64(multiply64(square, from_long(n)), from_long(n)), from_long((n + 1) * (n + 2)));
        term = neg64(multiply64(term, factor));
        if (is_zero(term)) break;
        sum = add64(sum, term);
    }
    return sum;
}

// asinh(x) = ln(x + sqrt(x^2 + 1)); for |x| >= 2^28, ln(2|x|).
ulong asinh64(ulong x) {
    if (is_nan(x)) return x | QUIET;
    if (is_zero(x) || is_inf(x)) return x;
    ulong sign = x & SIGN, ax = x & ~SIGN;
    if (parts(ax).exponent <= -2) return sign | asinh_series(ax);
    if (parts(ax).exponent >= 28) return sign | add64(ln64(ax), 0x3fe62e42fefa39eful);
    return sign | ln64(add64(ax, sqrt64(add64(multiply64(ax, ax), F64_ONE))));
}

// acosh(x) = 2 ln(sqrt((x+1)/2) + sqrt((x-1)/2)), well-conditioned near x = 1;
// for x >= 2^28 it is ln(x) + ln 2. x >= 1, else NaN.
ulong acosh64(ulong x) {
    if (is_nan(x)) return x | QUIET;
    if (x == INF) return INF;
    if (is_zero(x) || (x & SIGN)) return nan_result(0, 0);
    if (x == F64_ONE) return 0;
    if (parts(x).exponent >= 28) return add64(ln64(x), 0x3fe62e42fefa39eful);
    ulong halfway = 0x3fe0000000000000ul;
    ulong lo = sqrt64(multiply64(sub64(x, F64_ONE), halfway));
    ulong hi = sqrt64(multiply64(add64(x, F64_ONE), halfway));
    return multiply64(ln64(add64(hi, lo)), from_long(2));
}

// atanh(x) = 0.5 ln((1+x)/(1-x)); the series for |x| <= 1/4.
ulong atanh64(ulong x) {
    if (is_nan(x)) return x | QUIET;
    if (is_zero(x)) return x;
    ulong ax = x & ~SIGN;
    if (ax > F64_ONE) return nan_result(0, 0);
    if (ax == F64_ONE) return (x & SIGN) ? (INF | SIGN) : INF;
    if (parts(ax).exponent <= -3) {
        Parts p = parts(ax);
        U128 u = u128_shl((U128){0, p.mantissa}, (uint)(p.exponent + 68));
        U128 twice = u128_shl(atanh_series(u, Q120), 1);
        return fixed_to_double(x & SIGN, twice, -121);
    }
    ulong ratio = divide64(add64(F64_ONE, x), sub64(F64_ONE, x));
    return multiply64(ln64(ratio), 0x3fe0000000000000ul);   // 0.5 * ln(ratio)
}

// ---- Trigonometric functions [BUILTIN-MATH-TRIG] ----

// sin(r) and cos(r) Taylor series for |r| <= pi/4.
static ulong sin_series(ulong r) {
    ulong r2 = multiply64(r, r);
    ulong term = r, sum = r;
    for (long n = 3; n < 21; n += 2) {
        term = neg64(divide64(multiply64(term, r2), from_long(n * (n - 1))));
        sum = add64(sum, term);
    }
    return sum;
}

static ulong cos_series(ulong r) {
    ulong r2 = multiply64(r, r);
    ulong term = F64_ONE, sum = F64_ONE;
    for (long n = 2; n < 20; n += 2) {
        term = neg64(divide64(multiply64(term, r2), from_long(n * (n - 1))));
        sum = add64(sum, term);
    }
    return sum;
}

// Reduce |x| mod pi/2: n = round(x * 2/pi), then r = x - n*pi/2 computed with
// Veltkamp splitting so n*pi/2 is exact to the last bit (correct for |x| < 2^26).
typedef struct { int quadrant; ulong r; } Reduction;

static Reduction trig_reduce(ulong ax) {
    ulong nf = round64(multiply64(ax, TWOPI_HI));
    long n = to_long(nf);
    ulong prod = multiply64(from_long(n), PI2_HI);       // rounded n*pi/2_hi
    ulong c = 0x41a0000002000000ul;                       // 2^27 + 1
    ulong ahi = multiply64(c, PI2_HI);
    ulong hi = sub64(ahi, sub64(ahi, PI2_HI));            // top 26 bits
    ulong lo = sub64(PI2_HI, hi);
    ulong t1 = multiply64(from_long(n), hi);              // exact
    ulong t2 = multiply64(from_long(n), lo);              // exact
    ulong residual = add64(sub64(t1, prod), t2);          // n*pi/2_hi - prod, exact
    ulong correction = add64(multiply64(from_long(n), PI2_LO), residual);
    ulong r = sub64(ax, prod);
    r = sub64(r, correction);
    return (Reduction){(int)(n & 3), r};
}

ulong sin64(ulong x) {
    if (is_nan(x)) return x | QUIET;
    if (is_inf(x)) return nan_result(0, 0);
    if (is_zero(x)) return x;
    ulong sign = x & SIGN, ax = x & ~SIGN;
    Reduction red = trig_reduce(ax);
    ulong s = sin_series(red.r), c = cos_series(red.r);
    ulong v = red.quadrant == 0 ? s : red.quadrant == 1 ? c : red.quadrant == 2 ? neg64(s) : neg64(c);
    return v ^ sign;
}

ulong cos64(ulong x) {
    if (is_nan(x)) return x | QUIET;
    if (is_inf(x)) return nan_result(0, 0);
    if (is_zero(x)) return F64_ONE;
    ulong ax = x & ~SIGN;
    Reduction red = trig_reduce(ax);
    ulong s = sin_series(red.r), c = cos_series(red.r);
    ulong v = red.quadrant == 0 ? c : red.quadrant == 1 ? neg64(s) : red.quadrant == 2 ? neg64(c) : s;
    return v;
}

ulong tan64(ulong x) {
    if (is_nan(x)) return x | QUIET;
    if (is_inf(x)) return nan_result(0, 0);
    if (is_zero(x)) return x;
    return divide64(sin64(x), cos64(x));
}

// atan(x) = x - x^3/3 + ... for |x| <= 1/2; beyond that a half-angle-like
// reduction atan(x) = pi/4 + atan((x-1)/(1+x)) keeps the series argument
// small, and |x| > 1 uses atan(x) = pi/2 - atan(1/x).
static ulong atan_series(ulong ax) {
    ulong square = multiply64(ax, ax);
    ulong term = ax, sum = ax;
    for (long n = 3; n < 44; n += 2) {
        term = divide64(multiply64(multiply64(term, square), from_long(n - 2)), from_long(n));
        if (is_zero(term)) break;
        sum = ((n & 3) == 1) ? add64(sum, term) : sub64(sum, term);
    }
    return sum;
}

static ulong atan_reduced(ulong ax) {
    if (ax > 0x3fe0000000000000ul) {          // > 0.5
        ulong t = divide64(sub64(ax, F64_ONE), add64(ax, F64_ONE));
        return add64(0x3fe921fb54442d18ul, atan_series(t));   // pi/4 + atan(t)
    }
    return atan_series(ax);
}

ulong atan64(ulong x) {
    if (is_nan(x)) return x | QUIET;
    if (is_zero(x)) return x;
    ulong sign = x & SIGN, ax = x & ~SIGN;
    if (ax == INF) return sign | PI2_HI;
    if (ax > F64_ONE) return sign | sub64(PI2_HI, atan_reduced(divide64(F64_ONE, ax)));
    return sign | atan_reduced(ax);
}

ulong asin64(ulong x) {
    if (is_nan(x)) return x | QUIET;
    if (is_zero(x)) return x;
    ulong sign = x & SIGN, ax = x & ~SIGN;
    if (ax > F64_ONE) return nan_result(0, 0);
    if (ax == F64_ONE) return sign | PI2_HI;
    ulong den = sqrt64(sub64(F64_ONE, multiply64(ax, ax)));
    return sign | atan64(divide64(ax, den));
}

ulong acos64(ulong x) {
    if (is_nan(x)) return x | QUIET;
    if (x == F64_ONE) return 0;
    if ((x & ~SIGN) > F64_ONE) return nan_result(0, 0);
    return sub64(PI2_HI, asin64(x));
}

ulong atan264(ulong y, ulong x) {
    if (is_nan(x) || is_nan(y)) return nan_result(x, y);
    if (is_zero(y)) return (x & SIGN) ? ((y & SIGN) | PI) : y;
    if (is_inf(x) && is_inf(y)) {
        ulong pi4 = 0x3fe921fb54442d18ul, pi34 = 0x4002d97c7f3321d2ul;
        if (x & SIGN) return (y & SIGN) ? (pi34 | SIGN) : pi34;
        return (y & SIGN) ? (pi4 | SIGN) : pi4;
    }
    if (is_zero(x)) return (y & SIGN) ? (PI2_HI | SIGN) : PI2_HI;
    if (is_inf(x)) return (x & SIGN) ? ((y & SIGN) ? (PI | SIGN) : PI) : ((y & SIGN) ? (0x8000000000000000ul) : 0);
    if (is_inf(y)) return (y & SIGN) ? (PI2_HI | SIGN) : PI2_HI;
    ulong q = atan64(divide64(y & ~SIGN, x & ~SIGN));
    if (!(x & SIGN)) return (y & SIGN) ? neg64(q) : q;
    ulong base = sub64(PI, q);
    return (y & SIGN) ? neg64(base) : base;
}

// ---- Error functions [BUILTIN-MATH-SPECIAL] ----

// erf(x) = (2/sqrt(pi)) * sum (-1)^n x^(2n+1)/(n!(2n+1)) for |x| <= 4;
// beyond that erfc via the asymptotic series and erf = 1 - erfc.
ulong erf64(ulong x) {
    if (is_nan(x)) return x | QUIET;
    ulong sign = x & SIGN, ax = x & ~SIGN;
    if (ax == INF) return sign | F64_ONE;
    if (is_zero(x)) return x;
    if (parts(ax).exponent <= 2) {
        ulong square = multiply64(x, x);
        ulong term = x, sum = x;
        for (long n = 1; n < 80; n++) {
            term = divide64(multiply64(multiply64(term, square), from_long(-(2 * n - 1))), from_long(n * (2 * n + 1)));
            if (is_zero(term)) break;
            sum = add64(sum, term);
        }
        return multiply64(sum, TWO_OVER_SQRT_PI);
    }
    ulong x2 = multiply64(ax, ax);
    ulong e = exp64(neg64(x2));
    ulong s = F64_ONE, term = F64_ONE;
    for (long n = 1; n < 32; n++) {
        term = divide64(multiply64(term, from_long(-(2 * n - 1))), multiply64(from_long(2), x2));
        if (is_zero(term)) break;
        s = add64(s, term);
    }
    ulong erfc = divide64(multiply64(e, s), multiply64(ax, TWO_OVER_SQRT_PI));
    return sign | sub64(F64_ONE, erfc);
}

ulong erfc64(ulong x) {
    if (is_nan(x)) return x | QUIET;
    if (x == INF) return 0;
    if (x == (INF | SIGN)) return from_long(2);
    return sub64(F64_ONE, erf64(x));
}
