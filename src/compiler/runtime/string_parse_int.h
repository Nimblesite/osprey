// Internal implementation of [BUILTIN-STRING-PARSING], included once by string_runtime.c.
#ifndef OSP_STRING_PARSE_INT_H
#define OSP_STRING_PARSE_INT_H
#include <stdint.h>

/* Strict decimal parsing; unsigned magnitude permits INT64_MIN without overflow. */
#ifdef OSP_SYMBOLIC
#ifdef __clang__
/* optnone is clang-only; gcc fails -Werror=attributes on it. */
#define OSP_PARSE_STEP __attribute__((noinline, optnone))
#else
#define OSP_PARSE_STEP __attribute__((noinline))
#endif
/* The symbolic path consults the hunt input-capacity registry; the concrete
   path below stays freestanding. Quoted lookup finds klee/hunt_mock.h beside
   this header, exactly as memory_gc_symbolic_roots.h does. */
#include "klee/hunt_mock.h"

static OSP_PARSE_STEP int osp_decimal_bounded(const char *s, unsigned last, size_t capacity, uint64_t *out) {
    uint64_t value = 0;
    unsigned active = 1, invalid = 0, overflow = 0;
    for (size_t i = 0; (i < capacity) | active; i++) {
        unsigned c = (unsigned char)s[i], digit = c - (unsigned)'0';
        unsigned take = active & (c != 0);
        invalid |= take & (digit > 9U);
        unsigned append = take & (digit <= 9U) & !overflow;
        overflow |= append & ((value > UINT64_C(922337203685477580)) |
                    ((value == UINT64_C(922337203685477580)) & (digit > last)));
        volatile uint64_t mask = UINT64_C(0) - (uint64_t)(append & !overflow);
        value = ((value * 10U + digit) & mask) | (value & ~mask);
        active = take & (digit <= 9U);
    }
    if (invalid | overflow) return 1;
    *out = value;
    return 0;
}
#else
#define OSP_PARSE_STEP
#endif

static OSP_PARSE_STEP size_t osp_decimal_size(const char *s) {
    size_t n = 0;
    while (s[n]) {
        if ((unsigned)(unsigned char)s[n] - (unsigned)'0' > 9U) return SIZE_MAX;
        n++;
    }
    return n;
}
static OSP_PARSE_STEP int osp_decimal_limit(const char *s, unsigned last) {
    const char *limit = "922337203685477580";
    unsigned greater = (unsigned)(s[18] - '0') > last;
    for (unsigned i = 18; i > 0; i--) {
        unsigned index = i - 1;
        greater = (s[index] > limit[index]) | ((s[index] == limit[index]) & greater);
    }
    return greater;
}
static OSP_PARSE_STEP uint32_t osp_decimal_block(const char *s, unsigned n) {
    uint32_t value = 0;
    for (unsigned i = 0; i < n; i++) value = value * 10U + (unsigned)(s[i] - '0');
    return value;
}
static OSP_PARSE_STEP unsigned osp_decimal_prefix(const char *s, size_t n) {
    unsigned zero = 1;
    for (size_t i = 0; i < n; i++) zero &= s[i] == '0';
    return zero;
}
static OSP_PARSE_STEP int osp_decimal_magnitude(const char *s, unsigned last, uint64_t *out) {
    size_t extent = osp_decimal_size(s);
    if (extent == SIZE_MAX) return 1;
    if (extent > 19 && !osp_decimal_prefix(s, extent - 19)) return 1;
    if (extent > 19) s += extent - 19;
    unsigned n = extent > 19 ? 19 : (unsigned)extent;
    if (n == 19 && osp_decimal_limit(s, last)) return 1;
    uint64_t value = osp_decimal_block(s, n > 9 ? n - (n > 18 ? 18 : 9) : n);
    if (n > 18) value = value * 1000000000ULL + osp_decimal_block(s + n - 18, 9);
    if (n > 9) value = value * 1000000000ULL + osp_decimal_block(s + n - 9, 9);
    *out = value;
    return 0;
}
static OSP_PARSE_STEP int osp_decimal_parse(const char *s, const char *p, unsigned last, uint64_t *out) {
#ifdef OSP_SYMBOLIC
    size_t capacity = osp_hunt_input_capacity(s), offset = (size_t)(p - s);
    if (capacity > offset) return osp_decimal_bounded(p, last, capacity - offset, out);
#else
    (void)s;
#endif
    return osp_decimal_magnitude(p, last, out);
}

OSP_PARSE_STEP int64_t osp_parse_int_strict(const char *s, int64_t *out) {
    if (!s || s[0] == '\0' || !out) return 1;
    int neg = 0;
    const char *p = s;
    if (*s == '-' || *s == '+') { neg = *s == '-'; p++; }
    if (*p == '\0') return 1;
    uint64_t acc = 0;
    if (osp_decimal_parse(s, p, neg ? 8U : 7U, &acc)) return 1;
    // int64_t has an exact two's-complement representation, including INT64_MIN.
    uint64_t bits = neg ? 0ULL - acc : acc;
    memcpy(out, &bits, sizeof(bits));
    return 0;
}

#endif
