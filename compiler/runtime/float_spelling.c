/* The spelling of a float: the smallest precision from 10 to 17 whose %g
 * spelling reads back as the same double. Every interpolated float and every
 * float a JSON document encodes goes through here, so it must be fast and it
 * must never change a spelling. Implements [BUILTIN-STRING].
 *
 * Formatting a double is the expensive step -- a 17-digit snprintf costs
 * about fifteen times a strtod -- so the digits are formatted ONCE, at 17
 * significant digits, and each shorter precision is rounded from that digit
 * string. Rounding a correctly rounded 17-digit string to p digits gives the
 * correctly rounded p-digit value unless the dropped digits are exactly
 * "500...0": the true value may then sit either side of the tie, so that one
 * precision asks snprintf instead.
 *
 * Most floats a program prints lie between 1e-16 and 1e17, and those need no
 * snprintf at all. A double is m * 2^e, so |d| * 10^s is m * 5^s * 2^(e+s):
 * for s <= 32 the product m * 5^s fits 128 bits, and the bits the shift
 * drops are the exact remainder. Every rounding decision -- ties included,
 * which go to the even digit as printf's do -- is then made on exact
 * integers. */
#include "float_spelling.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define FLOAT_SIGN_SHIFT 63
#define FLOAT_FRACTION_BITS 0x000FFFFFFFFFFFFFull
#define FLOAT_EXPONENT_BITS 0x7FF0000000000000ull
#define MIN_PRECISION 10
#define MAX_PRECISION 17

/* Significant digits d.ddd... and the decimal exponent of the first one. */
typedef struct {
    char digits[MAX_PRECISION];
    int count;
    int exponent;
} Digits;

/* floor(|d| * 10^(16 - exponent)), seventeen digits, and what it dropped. */
typedef struct {
    uint64_t truncated;
    int exponent;
    int dropped; /* 0 nothing, 1 under half a unit, 2 exactly half, 3 over */
} Exact;

static const uint64_t POW10[] = {1ull, 10ull, 100ull, 1000ull, 10000ull, 100000ull,
    1000000ull, 10000000ull, 100000000ull, 1000000000ull, 10000000000ull,
    100000000000ull, 1000000000000ull, 10000000000000ull, 100000000000000ull,
    1000000000000000ull, 10000000000000000ull, 100000000000000000ull};

/* Exact digits need native 128-bit integers. wasm32's sysroot ships no
 * compiler-rt, so there the shifts would not link; it spells from
 * snprintf's seventeen digits instead, which is exact too, only slower.
 * OSP_FLOAT_SPELLING_PORTABLE selects that path anywhere, so the C suite
 * proves it byte for byte on the host as well. */
#if defined(__SIZEOF_INT128__) && !defined(__wasm__) && !defined(OSP_FLOAT_SPELLING_PORTABLE)
typedef unsigned __int128 Wide;

#define POW5_U64_LIMIT 27
#define MAX_EXACT_SCALE 32

static Wide pow5(int s) {
    uint64_t low = 1;
    for (int i = 0; i < s && i < POW5_U64_LIMIT; i++) low *= 5;
    Wide p = low;
    for (int i = POW5_U64_LIMIT; i < s; i++) p *= 5;
    return p;
}

/* floor(e2 * log10(2)), which is the decimal exponent of 2^e2. */
static int decade_of_pow2(int e2) {
    int scaled = e2 * 78913; /* log10(2) * 2^18 */
    return scaled >= 0 ? scaled >> 18 : -((-scaled + (1 << 18) - 1) >> 18);
}

/* The seventeen digits of |d| = m * 2^e scaled by 10^s; 0 when they are not
 * exactly seventeen (the decimal exponent guess was one low). */
static int scaled_digits(uint64_t m, int e, int s, Exact *out) {
    Wide n = (Wide)m * pow5(s);
    int shift = e + s;
    Wide whole = shift >= 0 ? n << shift : n >> -shift;
    Wide rest = shift >= 0 ? 0 : n & (((Wide)1 << -shift) - 1);
    Wide half = shift >= 0 ? 1 : (Wide)1 << (-shift - 1);
    if (whole >= POW10[17]) return 0;
    out->truncated = (uint64_t)whole;
    out->dropped = rest == 0 ? 0 : rest < half ? 1 : rest == half ? 2 : 3;
    return 1;
}

static int exact_digits(uint64_t bits, Exact *out) {
    int biased = (int)((bits & FLOAT_EXPONENT_BITS) >> 52);
    if (biased == 0) return 0; /* subnormal: the general path */
    uint64_t m = (bits & FLOAT_FRACTION_BITS) | (1ull << 52);
    int x = decade_of_pow2(biased - 1023);
    for (int attempt = 0; attempt < 2; attempt++, x++) {
        int s = MAX_PRECISION - 1 - x;
        if (s < 0 || s > MAX_EXACT_SCALE) return 0;
        if (scaled_digits(m, biased - 1075, s, out)) {
            out->exponent = x;
            return 1;
        }
    }
    return 0;
}

#else
static int exact_digits(uint64_t bits, Exact *out) {
    (void)bits;
    (void)out;
    return 0;
}
#endif

/* `x` correctly rounded to `precision` digits, exact ties to even. */
static void exactly_rounded(const Exact *x, int precision, Digits *out) {
    int cut = MAX_PRECISION - precision;
    uint64_t kept = x->truncated / POW10[cut];
    uint64_t rest = x->truncated % POW10[cut];
    uint64_t half = cut > 0 ? 5 * POW10[cut - 1] : 0;
    int over = cut > 0 ? rest > half || (rest == half && x->dropped > 0)
                       : x->dropped == 3;
    int tie = cut > 0 ? rest == half && x->dropped == 0 : x->dropped == 2;
    kept += over || (tie && (kept & 1));
    out->exponent = x->exponent;
    if (kept == POW10[precision]) {
        kept = POW10[precision - 1];
        out->exponent++;
    }
    out->count = precision;
    for (int i = precision - 1; i >= 0; i--, kept /= 10) {
        out->digits[i] = (char)('0' + kept % 10);
    }
}

/* Whole values below 1e10 have at most ten digits, so %.10g spells them as
 * their integer exactly; writing the digits directly skips dtoa entirely.
 * Returns the length written, or 0 when `d` is not such a value. */
static int whole_spelling(double d, uint64_t bits, char *buf) {
    if (!(d > -1e10 && d < 1e10) || d != (double)(int64_t)d) return 0;
    char digits[12];
    int count = 0;
    int64_t whole = (int64_t)d;
    for (int64_t v = whole < 0 ? -whole : whole; count == 0 || v > 0; v /= 10) {
        digits[count++] = (char)('0' + v % 10);
    }
    int n = 0;
    if (bits >> FLOAT_SIGN_SHIFT) buf[n++] = '-';
    while (count > 0) buf[n++] = digits[--count];
    buf[n] = '\0';
    return n;
}

/* The 17 correctly rounded significant digits of finite, nonzero |d|. */
static int seventeen_digits(double d, Digits *out) {
    char text[32];
    int n = snprintf(text, sizeof(text), "%.16e", d < 0 ? -d : d);
    if (n < 20 || (size_t)n >= sizeof(text)) return 0;
    out->digits[0] = text[0];
    memcpy(out->digits + 1, text + 2, MAX_PRECISION - 1);
    out->count = MAX_PRECISION;
    out->exponent = (int)strtol(text + 19, NULL, 10);
    return 1;
}

/* Round `all` to `precision` digits; 0 when the dropped digits are exactly
 * a tie, which the 17 digits alone cannot settle. */
static int rounded(const Digits *all, int precision, Digits *out) {
    *out = *all;
    out->count = precision;
    if (precision >= all->count) return 1;
    int tie = all->digits[precision] == '5';
    for (int i = precision + 1; i < all->count && tie; i++) {
        tie = all->digits[i] == '0';
    }
    if (tie) return 0;
    if (all->digits[precision] < '5') return 1;
    int i = precision - 1;
    while (i >= 0 && out->digits[i] == '9') out->digits[i--] = '0';
    if (i >= 0) {
        out->digits[i]++;
    } else {
        out->digits[0] = '1'; /* 99.9 -> 100: one more decimal place */
        out->exponent++;
    }
    return 1;
}

/* "e+05", "e-308": a sign and at least two digits, as %g writes them. */
static int exponent_spelling(int x, char *buf) {
    int magnitude = x < 0 ? -x : x;
    int n = 0;
    buf[n++] = 'e';
    buf[n++] = x < 0 ? '-' : '+';
    if (magnitude >= 100) buf[n++] = (char)('0' + magnitude / 100);
    buf[n++] = (char)('0' + magnitude / 10 % 10);
    buf[n++] = (char)('0' + magnitude % 10);
    buf[n] = '\0';
    return n;
}

/* The digits as %g lays them out: fixed for exponents from -4 up to the
 * precision, scientific otherwise, trailing fraction zeros dropped. */
static int lay_out(const Digits *r, int negative, char *buf) {
    int len = r->count;
    while (len > 1 && r->digits[len - 1] == '0') len--;
    int x = r->exponent;
    int n = 0;
    if (negative) buf[n++] = '-';
    if (x < -4 || x >= r->count) {
        buf[n++] = r->digits[0];
        if (len > 1) buf[n++] = '.';
        memcpy(buf + n, r->digits + 1, (size_t)(len - 1));
        n += len - 1;
        return n + exponent_spelling(x, buf + n);
    }
    if (x < 0) {
        memcpy(buf + n, "0.0000", (size_t)(1 - x));
        n += 1 - x;
    }
    for (int i = 0; i < len || i <= x; i++) {
        if (i == x + 1 && i > 0) buf[n++] = '.';
        buf[n++] = i < len ? r->digits[i] : '0';
    }
    buf[n] = '\0';
    return n;
}

/* Where a spelling's digits come from: exact integers when the magnitude
 * allows, else snprintf's seventeen digits, else snprintf itself. */
typedef struct {
    int exact;
    Exact integers;
    const Digits *text;
    Digits seventeen;
} Source;

static void open_source(double d, uint64_t bits, Source *src) {
    src->exact = exact_digits(bits, &src->integers);
    src->text = !src->exact && seventeen_digits(d, &src->seventeen) ? &src->seventeen : NULL;
}

static int spelled_at(double d, const Source *src, int precision, char *buf, size_t cap) {
    Digits r;
    if (src->exact) {
        exactly_rounded(&src->integers, precision, &r);
        return lay_out(&r, d < 0, buf);
    }
    if (src->text && rounded(src->text, precision, &r)) return lay_out(&r, d < 0, buf);
    return snprintf(buf, cap, "%.*g", precision, d);
}

/* A precision that round-trips keeps round-tripping as digits are added --
 * except at a power of two, whose round-trip interval is narrower below than
 * above -- so the search is binary everywhere but there. Seventeen digits
 * always round-trip. */
static int shortest_spelling(double d, uint64_t bits, char *buf, size_t cap) {
    Source src;
    open_source(d, bits, &src);
    int lopsided = (bits & FLOAT_FRACTION_BITS) == 0;
    int low = MIN_PRECISION;
    int high = MAX_PRECISION;
    int spelled = -1;
    int n = -1;
    while (low < high) {
        int probe = lopsided || low == MIN_PRECISION ? low : low + (high - low) / 2;
        n = spelled_at(d, &src, probe, buf, cap);
        spelled = probe;
        if (n < 0 || (size_t)n >= cap) return n;
        if (strtod(buf, NULL) == d) {
            high = probe;
        } else {
            low = probe + 1;
        }
    }
    return spelled == high ? n : spelled_at(d, &src, high, buf, cap);
}

int osp_float_spelling(double d, char *buf, size_t cap) {
    uint64_t bits;
    memcpy(&bits, &d, sizeof(bits));
    if (cap < 32) return snprintf(buf, cap, "%.*g", MAX_PRECISION, d);
    int n = whole_spelling(d, bits, buf);
    if (n > 0) return n;
    if ((bits & FLOAT_EXPONENT_BITS) == FLOAT_EXPONENT_BITS) {
        return snprintf(buf, cap, "%.*g", MAX_PRECISION, d); /* nan, inf */
    }
    return shortest_spelling(d, bits, buf, cap);
}

/* Osprey's `round` is LLVM's roundeven [BUILTIN-MATH]: halfway cases go to the
 * even neighbour. Where the CPU has no such instruction (x86-64 below SSE4.1)
 * LLVM calls the C23 function roundeven instead, and MinGW's C runtime has
 * none, so a Windows program that rounds could not link. nearbyint rounds the
 * same way in the default rounding mode, which Osprey never changes. */
#if defined(__MINGW32__)
#include <math.h>

double roundeven(double value) { return nearbyint(value); }
#endif
