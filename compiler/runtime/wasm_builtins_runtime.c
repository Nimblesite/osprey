// Compiler helpers required by LLVM's wasm32 lowering. [WASM-TARGET-RUNTIME]
//
// `llvm.smul.with.overflow.i64` widens its operands while checking signed
// overflow. WebAssembly has no native i128 multiply, so LLVM lowers that
// widened operation to the standard compiler-runtime `__multi3` ABI. The
// Homebrew LLVM package and the sysroot-only WASI SDK used in CI do not ship a
// wasm compiler-rt archive, so the portable Osprey runtime provides the one
// helper its generated IR requires.

#include <stdint.h>

typedef __int128 osp_i128;

typedef union {
    osp_i128 whole;
    struct {
        uint64_t low;
        int64_t high;
    } words;
} osp_i128_words;

// Compute the low 128 bits of an unsigned 64-bit product from 32-bit partial
// products. Keeping every intermediate at 64 bits prevents this helper from
// recursively requiring `__multi3` when clang lowers it for wasm32.
static osp_i128 multiply_u64(uint64_t left, uint64_t right) {
    const unsigned half_bits = 32;
    const uint64_t half_mask = UINT32_MAX;
    osp_i128_words result;
    uint64_t carry;
    result.words.low = (left & half_mask) * (right & half_mask);
    carry = result.words.low >> half_bits;
    result.words.low &= half_mask;
    carry += (left >> half_bits) * (right & half_mask);
    result.words.low += (carry & half_mask) << half_bits;
    result.words.high = (int64_t)(carry >> half_bits);
    carry = result.words.low >> half_bits;
    result.words.low &= half_mask;
    carry += (right >> half_bits) * (left & half_mask);
    result.words.low += (carry & half_mask) << half_bits;
    result.words.high = (int64_t)((uint64_t)result.words.high + (carry >> half_bits)
                                  + (left >> half_bits) * (right >> half_bits));
    return result.whole;
}

osp_i128 __multi3(osp_i128 left, osp_i128 right);

osp_i128 __multi3(osp_i128 left, osp_i128 right) {
    osp_i128_words left_words = {.whole = left};
    osp_i128_words right_words = {.whole = right};
    osp_i128_words result = {.whole = multiply_u64(left_words.words.low,
                                                   right_words.words.low)};
    uint64_t cross = (uint64_t)left_words.words.high * right_words.words.low
                     + left_words.words.low * (uint64_t)right_words.words.high;
    result.words.high = (int64_t)((uint64_t)result.words.high + cross);
    return result.whole;
}
