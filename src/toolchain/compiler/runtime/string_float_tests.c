// Float spelling: toString of float, float32 and float16, round trips, and
// agreement with a reference spelling. Part of the string_runtime suite.

#include "string_tests_shared.h"

void test_float_to_string(void) {
    struct { double in; const char *want; } cases[] = {
        {3.0, "3.0"},       {0.0, "0.0"},        {-2.0, "-2.0"},
        {3.5, "3.5"},       {-0.25, "-0.25"},    {100000.0, "100000.0"},
        {1e20, "1e+20"},    {2.5e-8, "2.5e-08"}, {1234567890.5, "1234567890.5"},
    };
    for (size_t i = 0; i < sizeof(cases) / sizeof(cases[0]); i++) {
        char *s = osp_float_to_string(cases[i].in);
        assert(s != NULL);
        assert(strcmp(s, cases[i].want) == 0);
        release_test_buffer(s);
    }
    volatile double zero = 0.0;   /* volatile: no constant-fold warnings */
    char *nan_s = osp_float_to_string(zero / zero);
    /* NaN spelling is libc-defined ("nan"); the contract is no ".0" suffix */
    assert(nan_s != NULL && strstr(nan_s, "nan") != NULL);
    assert(strstr(nan_s, ".0") == NULL);
    release_test_buffer(nan_s);
    volatile double huge_val = 1e308;
    char *inf_s = osp_float_to_string(huge_val * 10.0);
    assert(inf_s != NULL && strstr(inf_s, "inf") != NULL);
    release_test_buffer(inf_s);
}

/* [TYPE-NUMERIC-SIZED] Exact binary64 -> binary16 rounding boundaries. */
void test_float16_conversion(void) {
    struct { double in; uint16_t want; } cases[] = {
        {0.0, 0x0000}, {-0.0, 0x8000}, {1.0, 0x3C00}, {-2.0, 0xC000},
        {65504.0, 0x7BFF}, {65519.99, 0x7BFF}, {65520.0, 0x7C00},
        {-65520.0, 0xFC00}, {INFINITY, 0x7C00}, {-INFINITY, 0xFC00},
        {6.103515625e-05, 0x0400}, {5.960464477539063e-08, 0x0001},
        {2.98023223876953125e-08, 0x0000}, {8.940696716308594e-08, 0x0002},
        {0x1.0000000000001p-25, 0x0001}, {1.00048828125, 0x3C00},
        {1.00146484375, 0x3C02}, {6.1005353927612305e-05, 0x0400},
    };
    for (size_t i = 0; i < sizeof(cases) / sizeof(cases[0]); i++) {
        uint16_t got = osp_float_to_f16(cases[i].in);
        if (got != cases[i].want) {
            fprintf(stderr, "f16 case %zu (%a): got %04x want %04x\n",
                    i, cases[i].in, got, cases[i].want);
        }
        assert(got == cases[i].want);
    }
    uint16_t nan = osp_float_to_f16(NAN);
    assert((nan & 0x7C00) == 0x7C00 && (nan & 0x03FF) != 0);
    nan = osp_float_to_f16(-NAN);
    assert((nan & 0xFC00) == 0xFC00 && (nan & 0x03FF) != 0);
    for (uint32_t b = 0; b < 65536; b++) {
        uint16_t back = osp_float_to_f16((double)osp_f16_to_f32((uint16_t)b));
        if ((b & 0x7C00) == 0x7C00 && (b & 0x03FF)) {
            assert((back & 0x7C00) == 0x7C00 && (back & 0x03FF) != 0);
            assert((back & 0x8000) == (b & 0x8000));
        } else {
            assert(back == b);
        }
    }
    printf("  ok  float16_conversion (all 65536 bit patterns)\n");
}

/* [TYPE-NUMERIC-PRINT] Every non-NaN binary16 spelling reads back exactly. */
void test_float16_spelling_roundtrip(void) {
    char buf[32];
    for (uint32_t b = 0; b < 65536; b++) {
        if ((b & 0x7C00) == 0x7C00 && (b & 0x03FF)) continue;
        int n = osp_float16_spelling((uint16_t)b, buf, sizeof(buf));
        assert(n >= 0 && (size_t)n < sizeof(buf));
        assert(osp_float_to_f16(strtod(buf, NULL)) == b);
    }
    printf("  ok  float16_spelling_roundtrip (all non-NaN patterns)\n");
}

static void check_float32_spelling(float value) {
    uint32_t bits;
    memcpy(&bits, &value, sizeof(bits));
    char buf[32];
    int n = osp_float32_spelling(value, buf, sizeof(buf));
    assert(n >= 0 && (size_t)n < sizeof(buf));
    if ((size_t)n > 14 + (bits >> 31)) {
        fprintf(stderr, "f32 %08x spelling too long: %s (%d)\n", bits, buf, n);
    }
    assert((size_t)n <= 14 + (bits >> 31));
    float back = (float)strtod(buf, NULL);
    uint32_t back_bits;
    memcpy(&back_bits, &back, sizeof(back_bits));
    assert(back_bits == bits);
}

void test_float32_spelling_roundtrip(void) {
    const float awkward[] = {0.0f, -0.0f, 0.1f, 16777216.0f, 1e10f,
        1.17549435e-38f, 3.402823466e38f, 1.40129846e-45f, 1.0f / 3.0f,
        65504.0f, -65504.0f, 9999999.0f, 0.0001f, 0.00001f};
    for (size_t i = 0; i < sizeof(awkward) / sizeof(awkward[0]); i++) {
        check_float32_spelling(awkward[i]);
    }
    uint32_t state = 0x12345678u;
    for (int i = 0; i < 100000; i++) {
        state = state * 1664525u + 1013904223u;
        if ((state & 0x7F800000u) == 0x7F800000u && (state & 0x007FFFFFu)) continue;
        float value;
        memcpy(&value, &state, sizeof(value));
        check_float32_spelling(value);
    }
    printf("  ok  float32_spelling_roundtrip (awkward + 100000 LCG patterns)\n");
}

static void expect_float32_string(float value, const char *want) {
    char *s = osp_float32_to_string(value);
    assert(s != NULL && strcmp(s, want) == 0);
    release_test_buffer(s);
}

static void expect_float16_string(uint16_t bits, const char *want) {
    char *s = osp_float16_to_string(bits);
    assert(s != NULL && strcmp(s, want) == 0);
    release_test_buffer(s);
}

void test_sized_float_strings(void) {
    expect_float32_string(0.1f, "0.1");
    expect_float32_string(16777216.0f, "16777216.0");
    expect_float32_string(1e10f, "1e+10");
    expect_float32_string(1.17549435e-38f, "1.1754944e-38");
    expect_float32_string(-0.0f, "-0.0");
    expect_float32_string(1234567936.0f, "1234568000.0");
    expect_float16_string(osp_float_to_f16(1.0 / 3.0), "0.3333");
    expect_float16_string(0x7BFF, "65500.0"); /* 65504: the shortest that reads back */
    expect_float16_string(0x0001, "6e-08");
    expect_float16_string(0x8000, "-0.0");
    expect_float16_string(0x7C00, "inf");
    char *s = osp_float32_to_string(NAN);
    assert(s != NULL && strcmp(s, "nan") == 0);
    release_test_buffer(s);
    printf("  ok  sized_float_strings\n");
}

static void reference_float_spelling(double d, char *buf, size_t cap) {
    int n = -1;
    for (int precision = 10; precision <= 17; precision++) {
        n = snprintf(buf, cap, "%.*g", precision, d);
        if (strtod(buf, NULL) == d) break;
    }
    if (strpbrk(buf, ".eEni") == NULL) {
        memcpy(buf + n, ".0", 3);
    }
}

static void expect_reference_spelling(double d) {
    char want[64];
    reference_float_spelling(d, want, sizeof(want));
    char *got = osp_float_to_string(d);
    if (got == NULL || strcmp(got, want) != 0) {
        fprintf(stderr, "%a: got %s want %s\n", d, got ? got : "(null)", want);
    }
    assert(got != NULL && strcmp(got, want) == 0);
    release_test_buffer(got);
}

static uint64_t next_bits(uint64_t *state) {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    return *state;
}

static double from_bits(uint64_t bits) {
    double d;
    memcpy(&d, &bits, sizeof(d));
    return d;
}

void test_float_to_string_matches_reference(void) {
    for (uint64_t e = 0; e < 2047; e++) { /* every power of two, subnormal ones too */
        uint64_t bits = e == 0 ? 1 : e << 52;
        for (uint64_t shift = 0; e == 0 && shift < 52; shift++) {
            expect_reference_spelling(from_bits(bits << shift));
        }
        expect_reference_spelling(from_bits(bits));
        expect_reference_spelling(-from_bits(bits));
    }
    const double edges[] = {0.0, -0.0, 1e10, 1e10 - 1, -(1e10 - 1), 9999999999.5,
                            1e15, 0.1, 0.2, 0.3, 1.0 / 3.0, 2.5, -7.75, 5e-324,
                            2.2250738585072014e-308, 1.7976931348623157e308};
    for (size_t i = 0; i < sizeof(edges) / sizeof(edges[0]); i++) {
        expect_reference_spelling(edges[i]);
    }
    for (int k = -20000; k <= 20000; k++) {
        expect_reference_spelling(k / 1000.0);
        expect_reference_spelling((double)k * 1048576.0);
        /* exact decimal ties at 11 to 13 digits: round half to even */
        expect_reference_spelling(12345678901.5 + k);
        expect_reference_spelling(123456789012.25 + k);
        expect_reference_spelling(1234567890123.125 + k);
    }
    /* every decade and its neighbours: where the decimal exponent turns */
    for (double up = 1.0, down = 1.0; up < 1e308; up *= 10.0, down /= 10.0) {
        const double decades[] = {up, down};
        for (int i = 0; i < 2; i++) {
            uint64_t bits;
            memcpy(&bits, &decades[i], sizeof(bits));
            expect_reference_spelling(decades[i]);
            expect_reference_spelling(from_bits(bits - 1));
            expect_reference_spelling(from_bits(bits + 1));
            expect_reference_spelling(9.999999999999999 * decades[i]);
        }
    }
    uint64_t state = 0x9E3779B97F4A7C15u;
    for (int i = 0; i < 1000000; i++) {
        double d = from_bits(next_bits(&state));
        if (d - d == 0.0) { /* finite */
            expect_reference_spelling(d);
        }
    }
}
