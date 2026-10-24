# osprey: flavor=python
# osprey: namespace=safetensors
# osprey: module=Bytes
"""Little-endian byte readers and exact float-from-bits decoding shared by the
safetensors and gguf ports.

A file is a `list[int]` of byte values 0..255. The four decoders rebuild the
binary64 a binary16/binary32/bfloat16/binary64 word holds using exact integer
and power-of-two arithmetic (sign, exponent, mantissa times a power of two by
repeated doubling/halving), including subnormals, +-0, +-inf and NaN. They are
named so they can be swapped for the `floatFromBits`/`float32FromBits` builtins
once those land in the compiler. Implements [BUILTIN-FLOAT-BITS].
"""

__all__ = ["pow2i", "pow2", "le_bytes", "le16", "le32", "le64", "read_bytes", "read_u64", "f16_from_bits", "bf16_from_bits", "f32_from_bits", "f64_from_parts", "decode_f16", "decode_bf16", "decode_f32", "decode_f64", "bytes_to_string", "int_bytes", "f64_bytes", "f32_bytes", "f16_bytes", "bf16_bytes"]


def pow2i(e):
    """2^e as an int, for e in [0, 56], by repeated doubling."""
    return fold(range(e), 1, lambda acc, i: acc * 2)


def le_bytes(data, off, n):
    """The n little-endian bytes at `off` as one int; n <= 7 keeps it in range."""
    return fold(range(n), 0, lambda acc, i: acc + unwrap_or(listGet(data, off + i), 0) * pow2i(8 * i))


def le16(data, off):
    """Two little-endian bytes at `off` as one int."""
    return le_bytes(data, off, 2)


def le32(data, off):
    """Four little-endian bytes at `off` as one int."""
    return le_bytes(data, off, 4)


def le64(data, off):
    """Eight little-endian bytes at `off` as one int; callers ensure bytes 6 and 7
    are zero so the value stays in range."""
    return le_bytes(data, off, 8)


def read_bytes(data, off, n):
    """n bytes starting at `off`, or Error when the range leaves `data`."""
    if off < 0 or n < 0 or off > len(data) or n > len(data) - off:
        return Error(message="read_bytes: out of range")
    else:
        return Success(value=toList(map(range(n), lambda i: unwrap_or(listGet(data, off + i), 0))))


def read_u64(data, off):
    """Eight little-endian bytes at `off` as one int, or Error when out of range or
    the value would exceed 2^48 (bytes 6 or 7 nonzero, impossible in a real file)."""
    if off < 0 or off + 8 > len(data):
        return Error(message="read_u64: out of range")
    else:
        hi = le16(data, off + 6)
        if hi != 0:
            return Error(message="read_u64: value too large")
        else:
            return Success(value=le64(data, off))


def pow2(e):
    """2^e exactly, by repeated doubling for e >= 0 and halving for e < 0. Exact
    for the float64 exponent range [-1074, 1024] this module decodes."""
    if e >= 0:
        return fold(range(e), 1.0, lambda acc, i: acc * 2.0)
    else:
        return fold(range(0 - e), 1.0, lambda acc, i: acc / 2.0)


def f16_from_bits(bits):
    """The binary64 the binary16 word `bits` holds, reconstructed exactly."""
    sign = 1.0 if intDiv(bits, 32768) == 0 else -1.0
    exp = intDiv(bits, 1024) % 32
    frac = bits % 1024
    if exp == 31:
        return sign * pow2(1024) if frac == 0 else pow2(1024) - pow2(1024)
    elif exp == 0:
        return sign * toFloat(frac) * pow2(-24)
    else:
        return sign * toFloat(1024 + frac) * pow2(exp - 25)


def bf16_from_bits(bits):
    """The binary64 the bfloat16 word `bits` holds, reconstructed exactly."""
    sign = 1.0 if intDiv(bits, 32768) == 0 else -1.0
    exp = intDiv(bits, 128) % 256
    frac = bits % 128
    if exp == 255:
        return sign * pow2(1024) if frac == 0 else pow2(1024) - pow2(1024)
    elif exp == 0:
        return sign * toFloat(frac) * pow2(-133)
    else:
        return sign * toFloat(128 + frac) * pow2(exp - 134)


def f32_from_bits(bits):
    """The binary64 the binary32 word `bits` holds, reconstructed exactly."""
    sign = 1.0 if intDiv(bits, 2147483648) == 0 else -1.0
    exp = intDiv(bits, 8388608) % 256
    frac = bits % 8388608
    if exp == 255:
        return sign * pow2(1024) if frac == 0 else pow2(1024) - pow2(1024)
    elif exp == 0:
        return sign * toFloat(frac) * pow2(-149)
    else:
        return sign * toFloat(8388608 + frac) * pow2(exp - 150)


def f64_from_parts(sign, exp, frac):
    """The binary64 with sign bit `sign`, 11-bit exponent `exp` and 52-bit
    fraction `frac`, reconstructed exactly."""
    s = 1.0 if sign == 0 else -1.0
    if exp == 2047:
        return s * pow2(1024) if frac == 0 else pow2(1024) - pow2(1024)
    elif exp == 0:
        return s * toFloat(frac) * pow2(-1074)
    else:
        return s * toFloat(4503599627370496 + frac) * pow2(exp - 1075)


def decode_f16(data, off):
    """The binary16 stored little-endian at `off` as a binary64."""
    return f16_from_bits(le16(data, off))


def decode_bf16(data, off):
    """The bfloat16 stored little-endian at `off` as a binary64."""
    return bf16_from_bits(le16(data, off))


def decode_f32(data, off):
    """The binary32 stored little-endian at `off` as a binary64."""
    return f32_from_bits(le32(data, off))


def decode_f64(data, off):
    """The binary64 stored little-endian at `off` as a binary64."""
    b7 = unwrap_or(listGet(data, off + 7), 0)
    b6 = unwrap_or(listGet(data, off + 6), 0)
    sign = intDiv(b7, 128)
    exp = (b7 % 128) * 16 + intDiv(b6, 16)
    frac = (b6 % 16) * 281474976710656 + le_bytes(data, off, 6)
    return f64_from_parts(sign, exp, frac)


def bytes_to_string(data):
    """The UTF-8 text `data` holds, or Error when the bytes are not valid UTF-8."""
    match utf8_decode(data, 0, []):
        case Success(value=value):
            return Success(value=join(value, ""))
        case Error(message=message):
            return Error(message=message)


def u8_of(v):
    """The low byte of v as an unsigned 0..255."""
    return (v % 256 + 256) % 256


def floor_div_256(v):
    """Floor division of v by 256 (arithmetic shift right)."""
    if v >= 0:
        return intDiv(v, 256)
    else:
        return intDiv(v - 255, 256)


def int_bytes_from(v, n, acc):
    """The LE bytes of v (n of them), least significant first."""
    if n == 0:
        return acc
    else:
        return int_bytes_from(floor_div_256(v), n - 1, listAppend(acc, u8_of(v)))


def int_bytes(v, n):
    """n little-endian two's-complement bytes of the int v."""
    return int_bytes_from(v, n, [])


def is_nan(x):
    """Whether the float x is a NaN."""
    return x != x


def is_inf(x):
    """Whether the float x is an infinity."""
    return abs(x) == exp(1000.0)


def is_neg_zero(x):
    """Whether the float x is negative zero."""
    return x == 0.0 and toString(x) == "-0.0"


def scale_up_by(v, n):
    """v doubled exactly n times."""
    return fold(range(n), v, lambda acc, i: acc * 2.0)


def to_unit(m):
    """(e, u) with u = m / 2^e in [1, 2) for m > 0."""
    if m >= 1.0:
        return unit_up(m, 0)
    else:
        return unit_down(m, 0)


def unit_up(m, e):
    """Scale m down to [1, 2), counting doublings."""
    if m < 2.0:
        return (e, m)
    else:
        return unit_up(m / 2.0, e + 1)


def unit_down(m, e):
    """Scale m up to [1, 2), counting halvings."""
    if m >= 1.0:
        return (e, m)
    else:
        return unit_down(m * 2.0, e - 1)


def split_double(x):
    """(sign, F, E) with x = sign * F * 2^E, F a positive int < 2^53."""
    sign = 0 if x > 0.0 else 1
    m = abs(x)
    if m == 0.0:
        return (sign, 0, 0)
    elif m * pow2(1022) >= 1.0:
        (e, mm) = to_unit(m)
        F = unwrap_or(floatToInt(mm * pow2(52)), 0)
        return (sign, F, e - 52)
    else:
        F = unwrap_or(floatToInt(scale_up_by(m, 1074)), 0)
        return (sign, F, -1074)


def round_shift(M, k):
    """M >> k with round-half-even, for M >= 0 and k >= 0."""
    if k >= 54:
        return 0
    elif k == 0:
        return M
    else:
        q = intDiv(M, pow2i(k))
        r = M % pow2i(k)
        half = pow2i(k - 1)
        if r > half or (r == half and q % 2 == 1):
            return q + 1
        else:
            return q


def narrow_normal(F, E, frac_bits, bias):
    """The (exp, frac) fields of a normal significand F * 2^E."""
    N = round_shift(F, 52 - frac_bits)
    sig = pow2i(frac_bits + 1)
    if N >= sig:
        return (E + 52 + bias + 1, 0)
    else:
        return (E + 52 + bias, N - pow2i(frac_bits))


def narrow_subnormal(F, E, frac_bits, bias):
    """The (exp, frac) fields of a subnormal significand F * 2^E."""
    shift = 1 - bias - frac_bits - E
    N = round_shift(F, shift) if shift > 0 else F
    if N >= pow2i(frac_bits):
        return (1, 0)
    else:
        return (0, N)


def narrow_fields(F, E, frac_bits, bias):
    """The (exp, frac) fields for F * 2^E in a float format, F > 0."""
    if E + 52 >= 1 - bias and F >= pow2i(52):
        return narrow_normal(F, E, frac_bits, bias)
    else:
        return narrow_subnormal(F, E, frac_bits, bias)


def pack_float(sign, exp, frac, exp_bits, frac_bits):
    """LE bytes of a float with the given fields (total bits at most 32)."""
    nbits = exp_bits + frac_bits + 1
    nbytes = intDiv(nbits, 8)
    word = sign * pow2i(nbits - 1) + exp * pow2i(frac_bits) + frac
    return toList(map(range(nbytes), lambda i: intDiv(word, pow2i(8 * i)) % 256))


def narrow_float_bytes(x, exp_bits, frac_bits, bias):
    """LE bytes of x rounded to a float format."""
    max_exp = pow2i(exp_bits) - 1
    if x == 0.0:
        return pack_float(1 if is_neg_zero(x) else 0, 0, 0, exp_bits, frac_bits)
    elif is_nan(x):
        return pack_float(0, max_exp, pow2i(frac_bits - 1), exp_bits, frac_bits)
    elif is_inf(x):
        return pack_float(1 if x < 0.0 else 0, max_exp, 0, exp_bits, frac_bits)
    else:
        (sign, F, E) = split_double(x)
        (exp, frac) = narrow_fields(F, E, frac_bits, bias)
        if exp >= max_exp:
            return pack_float(sign, max_exp, 0, exp_bits, frac_bits)
        else:
            return pack_float(sign, exp, frac, exp_bits, frac_bits)


def f64_fields_bytes(sign, exp, frac):
    """8 LE bytes of a float64 with the given sign, 11-bit exp, 52-bit frac."""
    b0 = frac % 256
    b1 = intDiv(frac, 256) % 256
    b2 = intDiv(frac, 65536) % 256
    b3 = intDiv(frac, 16777216) % 256
    b4 = intDiv(frac, 4294967296) % 256
    b5 = intDiv(frac, 1099511627776) % 256
    b6 = intDiv(frac, 281474976710656) % 16 + (exp % 16) * 16
    b7 = sign * 128 + intDiv(exp, 16)
    return [b0, b1, b2, b3, b4, b5, b6, b7]


def f64_bytes(x):
    """8 little-endian bytes of the double x."""
    if x == 0.0:
        return [0, 0, 0, 0, 0, 0, 0, 128 if is_neg_zero(x) else 0]
    elif is_nan(x):
        return [0, 0, 0, 0, 0, 0, 248, 127]
    elif is_inf(x):
        sign = 1 if x < 0.0 else 0
        return [0, 0, 0, 0, 0, 0, 240, 127 + sign * 128]
    else:
        (sign, F, E) = split_double(x)
        if F >= pow2i(52):
            return f64_fields_bytes(sign, E + 1075, F - pow2i(52))
        else:
            return f64_fields_bytes(sign, 0, F)


def f32_bytes(x):
    """4 little-endian bytes of x rounded to binary32."""
    return narrow_float_bytes(x, 8, 23, 127)


def f16_bytes(x):
    """2 little-endian bytes of x rounded to binary16."""
    return narrow_float_bytes(x, 5, 10, 15)


def bf16_bytes(x):
    """2 little-endian bytes of x rounded to bfloat16."""
    return narrow_float_bytes(x, 8, 7, 127)


def utf8_decode(data, i, pieces) -> Result[list[str], str]:
    """Decode the UTF-8 bytes of `data` from `i`, collecting codepoints as strings."""
    if i >= len(data):
        return Success(value=pieces)
    else:
        return utf8_step(data, i, pieces)


def utf8_step(data, i, pieces):
    """Decode the one codepoint at `i` and continue with the rest."""
    match utf8_one(data, i):
        case Success(value=value):
            return utf8_decode(data, value.next, listAppend(pieces, unwrap_or(fromCodePoint(value.cp), "")))
        case Error(message=message):
            return Error(message="invalid utf-8 sequence of 1 bytes from index " + toString(i))


class Utf8Step:
    """One decoded UTF-8 codepoint and the index just past its bytes."""
    cp: int
    next: int


def is_cont(data, i):
    """Whether byte `i` of `data` is a UTF-8 continuation byte (0x80..0xBF)."""
    b = unwrap_or(listGet(data, i), -1)
    return b >= 128 and b <= 191


def utf8_one(data, i):
    """The codepoint and next index for the UTF-8 sequence at `i`, or Error when
    the bytes are not a valid sequence."""
    b0 = unwrap_or(listGet(data, i), -1)
    if b0 < 128:
        return Success(value=Utf8Step(cp=b0, next=i + 1))
    elif b0 >= 194 and b0 <= 223 and is_cont(data, i + 1):
        b1 = unwrap_or(listGet(data, i + 1), 0)
        return Success(value=Utf8Step(cp=(b0 - 192) * 64 + b1 - 128, next=i + 2))
    elif b0 >= 224 and b0 <= 239 and is_cont(data, i + 1) and is_cont(data, i + 2):
        return utf8_three(data, i, b0)
    elif b0 >= 240 and b0 <= 244 and is_cont(data, i + 1) and is_cont(data, i + 2) and is_cont(data, i + 3):
        return utf8_four(data, i, b0)
    else:
        return Error(message="bad utf-8 lead byte")


def utf8_three(data, i, b0):
    """Decode a 3-byte UTF-8 sequence with overlong and surrogate rejection."""
    b1 = unwrap_or(listGet(data, i + 1), 0)
    b2 = unwrap_or(listGet(data, i + 2), 0)
    cp = (b0 - 224) * 4096 + (b1 - 128) * 64 + b2 - 128
    if cp < 2048 or (b0 == 224 and b1 < 160) or (b0 == 237 and b1 >= 160):
        return Error(message="bad utf-8 3-byte sequence")
    else:
        return Success(value=Utf8Step(cp=cp, next=i + 3))


def utf8_four(data, i, b0):
    """Decode a 4-byte UTF-8 sequence with overlong and range rejection."""
    b1 = unwrap_or(listGet(data, i + 1), 0)
    b2 = unwrap_or(listGet(data, i + 2), 0)
    b3 = unwrap_or(listGet(data, i + 3), 0)
    cp = (b0 - 240) * 262144 + (b1 - 128) * 4096 + (b2 - 128) * 64 + b3 - 128
    if cp < 65536 or cp > 1114111 or (b0 == 240 and b1 < 144) or (b0 == 244 and b1 >= 144):
        return Error(message="bad utf-8 4-byte sequence")
    else:
        return Success(value=Utf8Step(cp=cp, next=i + 4))
