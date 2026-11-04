# osprey: flavor=python
# osprey: namespace=gguf
# osprey: module=Quant
"""Block decoders from stored GGUF tensor bytes to binary32 values, matching ggml's own
`dequantize_row_*` bit for bit ([LLM-WEIGHTS-QUANT]).

A file stores a quantized tensor as whole blocks: a binary16 super-block scale,
sometimes a minimum, then packed scales, low bits and high-bit masks. `decode` walks
whole blocks only, so a byte count that is not a whole number of blocks is an `Error`,
and every value it returns is the binary32 value ggml's C decoder writes for the same
bytes, widened exactly to `float`.

The arithmetic is at `float32` because ggml's is: each scale and each packed value is
rounded to binary32 before the block's multiply, and multiply and subtract happen in
ggml's order. A wider type would round differently and disagree in the last bit.

`IQ3_S_GRID` and `IQ4_NL_VALUES` are ggml's own tables, copied from its
`ggml-common.h` (`iq3s_grid` and `kvalues_iq4nl`) at llama.cpp commit
`3de52ac38818978d6da8f386eda43a856e53714a`.
"""
from gguf import Constants
from safetensors import Bytes

__all__ = ["SUPPORTED", "decode"]


class ScaleMin:
    """A Q4_K or Q5_K sub-block's 6-bit scale and its 6-bit minimum."""
    d: int
    m: int


SUPPORTED = ["F32", "Q8_0", "Q3_K", "Q4_K", "Q5_K", "Q6_K", "IQ4_NL", "IQ4_XS", "IQ3_S"]
"""The tensor types this port decodes: the ones the chosen weights file uses."""

Q3_K_LOW_MASK = 252645135
"""0x0f0f0f0f: the low four bits of each of four packed Q3_K scale bytes."""

Q3_K_HIGH_MASK = 50529027
"""0x03030303: the two high bits of each of four packed Q3_K scale bytes."""


def decode(type_name, data):
    """Every value `data` holds, decoded from its whole blocks of `type_name`."""
    match layout(type_name):
        case Success(value=block):
            return values_of(block, decoder(type_name), data)
        case Error(message=problem):
            return Error(message=problem)


def layout(type_name):
    """The block format of a type this port decodes, or an `Error` naming the type."""
    if listContains(SUPPORTED, type_name):
        return declared(type_name)
    else:
        return Error(message="gguf: no block decoder for tensor type " + type_name)


def declared(type_name):
    """The quant type record `type_name` names, from the package's own table."""
    return foldList(Constants.QUANT_TYPES, Error(message="gguf: no tensor type named " + type_name), lambda found, quant: Success(value=quant) if quant.name == type_name else found)


def decoder(type_name):
    """The per-element decoder of a type `SUPPORTED` lists, read as (data, block offset, index)."""
    if type_name == "F32":
        return f32_at
    elif type_name == "Q8_0":
        return q8_0_at
    elif type_name == "Q3_K":
        return q3_k_at
    elif type_name == "Q4_K":
        return q4_k_at
    elif type_name == "Q5_K":
        return q5_k_at
    elif type_name == "Q6_K":
        return q6_k_at
    elif type_name == "IQ4_NL":
        return iq4_nl_at
    elif type_name == "IQ4_XS":
        return iq4_xs_at
    else:
        return iq3_s_at


def values_of(block, element_at, data):
    """Every element of `data`'s whole blocks, as binary32 widened exactly to `float`."""
    if len(data) % block.size != 0:
        return Error(message=size_problem(block, data))
    else:
        total = intDiv(len(data), block.size) * block.block
        return Success(value=toList(map(range(total), lambda i: toFloat(element_at(data, intDiv(i, block.block) * block.size, i % block.block)))))


def size_problem(block, data):
    """The `Error` for a byte count that is not a whole number of blocks."""
    return "gguf: " + block.name + " data is " + toString(len(data)) + " bytes, not a whole number of " + toString(block.size) + "-byte blocks"


def f32_at(data, off, j):
    """F32 stores binary32 words, so a value is its own four bytes: one per block,
    which leaves `j` zero and the value at the block's own offset."""
    return toFloat32(Bytes.decode_f32(data, off + 4 * j))


def q8_0_at(data, off, j):
    """Q8_0: x = d * q, one signed byte per value."""
    return half_at(data, off) * toFloat32(signed_byte_at(data, off + 2 + j))


def q4_k_at(data, off, j):
    """Q4_K: x = d * scale * q - dmin * min, four bits per value in eight sub-blocks."""
    group = intDiv(j, 64)
    q = nibble(byte_at(data, off + 16 + 32 * group + j % 32), sub_block(j))
    return k4_value(data, off, 2 * group + sub_block(j), q)


def q5_k_at(data, off, j):
    """Q5_K: Q4_K's four low bits plus a high bit from the mask, one per value."""
    group = intDiv(j, 64)
    sub = sub_block(j)
    low = nibble(byte_at(data, off + 48 + 32 * group + j % 32), sub)
    return k4_value(data, off, 2 * group + sub, low + high_bit(byte_at(data, off + 16 + j % 32), 2 * group + sub))


def q6_k_at(data, off, j):
    """Q6_K: six bits per value from a low nibble and two mask bits, scaled per 16."""
    group = intDiv(j, 128)
    quarter = intDiv(j % 128, 32)
    l = j % 32
    low = nibble(byte_at(data, off + 64 * group + l + 32 * (quarter % 2)), intDiv(quarter, 2))
    high = bitAnd(shiftRight(byte_at(data, off + 128 + 32 * group + l), 2 * quarter), 3)
    scale = signed_byte_at(data, off + 192 + 8 * group + intDiv(l, 16) + 2 * quarter)
    return scaled(scaled(half_at(data, off + 208), scale), bitOr(low, shiftLeft(high, 4)) - 32)


def q3_k_at(data, off, j):
    """Q3_K: two low bits per value, a high bit from the mask and a 6-bit scale."""
    group = intDiv(j, 128)
    sub = intDiv(j % 128, 32)
    half = intDiv(j % 32, 16)
    l = j % 16
    low = bitAnd(shiftRight(byte_at(data, off + 32 + 32 * group + 16 * half + l), 2 * sub), 3)
    q = low - (0 if bit_at(byte_at(data, off + 16 * half + l), 4 * group + sub) else 4)
    scale = q3_k_scale(data, off + 96, 8 * group + 2 * sub + half)
    return scaled(half_at(data, off + 108), scale - 32) * toFloat32(q)


def iq4_nl_at(data, off, j):
    """IQ4_NL: one of 16 non-linear levels per value, scaled once per block."""
    q = nibble(byte_at(data, off + 2 + j % 16), intDiv(j, 16))
    return scaled(half_at(data, off), level_of(q))


def iq4_xs_at(data, off, j):
    """IQ4_XS: IQ4_NL's levels with a six-bit scale per 32 values."""
    ib = intDiv(j, 32)
    q = nibble(byte_at(data, off + 8 + 16 * ib + j % 16), intDiv(j % 32, 16))
    return scaled(half_at(data, off), xs_scale(data, off, ib) - 32) * toFloat32(level_of(q))


def iq3_s_at(data, off, j):
    """IQ3_S: a nine-bit grid index and a sign bit per value, scaled per 32."""
    group = intDiv(j, 64)
    half = intDiv(j % 64, 32)
    l = intDiv(j % 32, 8)
    w = j % 8
    value = scaled(scaled(half_at(data, off), 1 + 2 * nibble(byte_at(data, off + 106 + group), half)), grid_value(grid_index(data, off, group, half, l, w), w))
    return value * sign_at(data, off, group, half, l, w)


def xs_scale(data, off, ib):
    """IQ4_XS's six-bit scale for sub-block `ib`, four bits low and two from `scales_h`."""
    low = bitAnd(shiftRight(byte_at(data, off + 4 + intDiv(ib, 2)), 4 * (ib % 2)), 15)
    return bitOr(low, shiftLeft(bitAnd(shiftRight(Bytes.le16(data, off + 2), 2 * ib), 3), 4))


def grid_index(data, off, group, half, l, w):
    """IQ3_S's grid index: eight bits from `qs`, the ninth from the group's mask byte."""
    mask = byte_at(data, off + 66 + 2 * group + half)
    low = byte_at(data, off + 2 + 16 * group + 8 * half + 2 * l + intDiv(w, 4))
    return bitOr(low, bitAnd(shiftLeft(mask, 8 - 2 * l - intDiv(w, 4)), 256))


def grid_value(index, w):
    """Byte `w % 4` of grid word `index`, which holds four 4-bit values little-endian."""
    return bitAnd(shiftRight(unwrap_or(listGet(IQ3_S_GRID, index), 0), 8 * (w % 4)), 255)


def sign_at(data, off, group, half, l, w):
    """-1 when bit `w` of the `l`th sign byte of the group is set, and +1 otherwise."""
    if bit_at(byte_at(data, off + 74 + 8 * group + 4 * half + l), w):
        return toFloat32(-1.0)
    else:
        return toFloat32(1.0)


def level_of(q):
    """The non-linear 4-bit level `q`, from ggml's `kvalues_iq4nl`."""
    return unwrap_or(listGet(IQ4_NL_VALUES, q), 0)


def k4_value(data, off, scale_index, q):
    """`d * scale * q - dmin * min` for one Q4_K or Q5_K sub-block."""
    pair = k4_scale_min(data, off + 4, scale_index)
    return scaled(half_at(data, off), pair.d) * toFloat32(q) - scaled(half_at(data, off + 2), pair.m)


def k4_scale_min(data, off, index):
    """`get_scale_min_k4`'s six-bit scale and minimum for sub-block `index`."""
    if index < 4:
        return ScaleMin(d=bitAnd(byte_at(data, off + index), 63), m=bitAnd(byte_at(data, off + 4 + index), 63))
    else:
        return ScaleMin(d=bitOr(bitAnd(byte_at(data, off + 4 + index), 15), shiftLeft(shiftRight(byte_at(data, off + index - 4), 6), 4)),
                        m=bitOr(shiftRight(byte_at(data, off + 4 + index), 4), shiftLeft(shiftRight(byte_at(data, off + index), 6), 4)))


def q3_k_scale(data, off, index):
    """One six-bit scale from Q3_K's twelve packed bytes, which hold sixteen of them.

    ggml reads the table as `int8_t`, which cannot go negative here: every descrambled
    byte is `(low four bits) | (two more << 4)`, so the value is 0..63 and its
    `- 32` never wraps.
    """
    return bitAnd(shiftRight(q3_k_scale_word(data, off, intDiv(index, 4)), 8 * (index % 4)), 255)


def q3_k_scale_word(data, off, word):
    """Word `word` of Q3_K's descrambled scale table, as ggml's `dequantize_row_q3_K` builds it."""
    low = Bytes.le32(data, off)
    high = Bytes.le32(data, off + 4)
    packed = Bytes.le32(data, off + 8)
    if word == 0:
        return bitOr(bitAnd(low, Q3_K_LOW_MASK), shiftLeft(bitAnd(packed, Q3_K_HIGH_MASK), 4))
    elif word == 1:
        return bitOr(bitAnd(high, Q3_K_LOW_MASK), shiftLeft(bitAnd(shiftRight(packed, 2), Q3_K_HIGH_MASK), 4))
    elif word == 2:
        return bitOr(bitAnd(shiftRight(low, 4), Q3_K_LOW_MASK), shiftLeft(bitAnd(shiftRight(packed, 4), Q3_K_HIGH_MASK), 4))
    else:
        return bitOr(bitAnd(shiftRight(high, 4), Q3_K_LOW_MASK), shiftLeft(bitAnd(shiftRight(packed, 6), Q3_K_HIGH_MASK), 4))


def sub_block(j):
    """Which 32-value sub-block of a 64-value group element `j` falls in: 0 for low bits."""
    return intDiv(j % 64, 32)


def high_bit(mask, bit):
    """16 when `bit` of `mask` is set: the high bit a Q5_K value gains."""
    if bit_at(mask, bit):
        return 16
    else:
        return 0


def bit_at(byte, bit):
    """Whether bit `bit` of `byte` is set."""
    return bitAnd(byte, shiftLeft(1, bit)) != 0


def byte_at(data, off):
    """Byte `off` of a block, which `decode` bounds-checked by the input's length."""
    return unwrap_or(listGet(data, off), 0)


def signed_byte_at(data, off):
    """The int8 whose bits are byte `off` of a block."""
    value = byte_at(data, off)
    if value >= 128:
        return value - 256
    else:
        return value


def nibble(byte, half):
    """Four bits of `byte`: the low half when `half` is 0, the high half when it is 1."""
    return bitAnd(shiftRight(byte, 4 * half), 15)


def half_at(data, off):
    """The binary16 super-block scale at `off`, at binary32."""
    return toFloat32(Bytes.decode_f16(data, off))


def scaled(d, n):
    """`d` times the integer `n`, at binary32."""
    return d * toFloat32(n)


IQ3_S_GRID = [
    16843009, 16843011, 16843013, 16843019, 16843023, 16843521, 16843523, 16843525,
    16843529, 16843533, 16844033, 16844035, 16844043, 16844551, 16845057, 16845061,
    16845067, 16845071, 16845571, 16845575, 16846081, 16846085, 16846595, 16846601,
    16846607, 16974081, 16974083, 16974085, 16974089, 16974593, 16974595, 16974603,
    16975105, 16975111, 16975119, 16975619, 16975627, 16976137, 16977155, 16977163,
    16977669, 17105153, 17105155, 17105163, 17105167, 17105665, 17105671, 17105677,
    17106179, 17106187, 17106689, 17106697, 17107205, 17107211, 17107215, 17107715,
    17107719, 17108737, 17108743, 17236231, 17236739, 17236747, 17237249, 17237253,
    17237763, 17237767, 17237773, 17238281, 17238785, 17238789, 17239311, 17239811,
    17239819, 17367297, 17367815, 17367823, 17368323, 17368329, 17368837, 17369345,
    17369351, 17369859, 17370881, 17498373, 17498377, 17499393, 17499397, 17499405,
    17499911, 17500419, 17500427, 17500431, 17501453, 17501959, 17629453, 17629955,
    17629959, 17630979, 17632005, 17633027, 17760513, 17760517, 17760521, 17761537,
    17761541, 17761549, 17762055, 17763073, 17763081, 50397441, 50397443, 50397445,
    50397449, 50397953, 50397955, 50397959, 50397963, 50397967, 50398465, 50398469,
    50398979, 50398985, 50398989, 50400009, 50400013, 50400515, 50401029, 50528513,
    50528515, 50528519, 50528525, 50529025, 50529033, 50529539, 50530049, 50530055,
    50530563, 50531073, 50531077, 50532097, 50532109, 50659585, 50660101, 50660107,
    50660111, 50660609, 50660617, 50661125, 50661633, 50661639, 50662155, 50662657,
    50663173, 50790659, 50790665, 50790671, 50791169, 50791175, 50791683, 50791695,
    50792193, 50792201, 50792707, 50793733, 50794241, 50921735, 50921739, 50922245,
    50922249, 50923267, 50923271, 50923781, 50923789, 50924289, 50924297, 51052803,
    51053313, 51053319, 51053827, 51054337, 51054341, 51055363, 51184897, 51184905,
    51184911, 51185929, 51185933, 51314947, 51314951, 51315457, 51315461, 51315971,
    51316491, 51316995, 51318021, 51318529, 83951873, 83951875, 83951879, 83951883,
    83951887, 83952385, 83952389, 83952393, 83952397, 83952899, 83952903, 83952911,
    83953409, 83953413, 83953923, 83953927, 83953931, 83954433, 83954437, 83954959,
    83955457, 83955463, 83955467, 84082945, 84082949, 84083457, 84083463, 84083471,
    84083973, 84083979, 84084483, 84084489, 84084997, 84085507, 84214019, 84214025,
    84214031, 84215043, 84215047, 84215553, 84215567, 84216067, 84216583, 84216591,
    84217603, 84217609, 84345089, 84345093, 84345099, 84345603, 84346117, 84346121,
    84346627, 84346631, 84347141, 84347649, 84348173, 84476163, 84476175, 84477185,
    84477191, 84477701, 84477707, 84478211, 84479749, 84479755, 84607241, 84607747,
    84608261, 84608783, 84609281, 84609799, 84610817, 84738305, 84738309, 84738319,
    84739331, 84740875, 84741379, 84869387, 84869891, 84870413, 84870913, 84871431,
    84871937, 117506309, 117506819, 117506823, 117506827, 117506831, 117507333, 117507843,
    117507847, 117507851, 117508357, 117508361, 117508367, 117508867, 117509383, 117509891,
    117637379, 117637383, 117637387, 117637897, 117638403, 117638407, 117639425, 117640449,
    117640965, 117640973, 117768449, 117768965, 117769473, 117769989, 117769993, 117771009,
    117899523, 117900033, 117900041, 117900547, 117900551, 117900559, 117901057, 117901571,
    117901575, 117901583, 117902091, 117903111, 118030599, 118031107, 118031117, 118031621,
    118032131, 118033157, 118033665, 118033673, 118161667, 118162177, 118162181, 118162699,
    118163205, 118163721, 118164237, 118165255, 118293261, 118294787, 118423811, 118423815,
    118424833, 118424837, 118425355, 151060737, 151060745, 151061253, 151061761, 151061769,
    151061775, 151062277, 151062787, 151063297, 151064321, 151191813, 151191823, 151192323,
    151192327, 151192837, 151193345, 151193355, 151193863, 151194371, 151194379, 151322883,
    151322887, 151323393, 151323403, 151323907, 151324423, 151324929, 151325455, 151325957,
    151326465, 151453961, 151454467, 151454471, 151454977, 151454981, 151455491, 151455499,
    151585025, 151585029, 151586057, 151586575, 151587073, 151588611, 151716107, 151716111,
    151717123, 151719173, 151847687, 151848713, 151850241, 151978753, 151978763, 151979777,
    151980295, 151980803, 184615173, 184615681, 184615689, 184616197, 184617217, 184617225,
    184617231, 184617733, 184618253, 184618761, 184746243, 184746247, 184746251, 184746757,
    184747267, 184747781, 184749829, 184877313, 184877827, 184878343, 184878849, 184878861,
    184879879, 185008389, 185008399, 185008897, 185009423, 185010441, 185010947, 185011467,
    185011975, 185139459, 185139465, 185140481, 185140997, 185141517, 185271045, 185271565,
    185273091, 185273095, 185403653, 185532677, 185532681, 185533701, 218170115, 218170119,
    218170123, 218171139, 218171143, 218172673, 218300673, 218301697, 218301711, 218303753,
    218432261, 218433289, 218433797, 218434315, 218434821, 218435329, 218562817, 218563337,
    218563843, 218564865, 218694923, 218695943, 218696965, 218824961, 218824967, 218826505,
    218828033, 218956043, 218958081, 219087619, 219087623, 251724033, 251724041, 251724047,
    251725057, 251725061, 251725581, 251726081, 251726601, 251727109, 251855109, 251855619,
    251856137, 251857159, 251857163, 251986179, 251986185, 251986689, 251986701, 251987203,
    251987713, 251988739, 252117253, 252118789, 252118795, 252119815, 252248323, 252248331,
    252248839, 252249345, 252250881, 252380421, 252381445, 252510469, 252512003, 252641537,
]
"""ggml's `iq3s_grid`: 512 words, each four 4-bit grid values packed little-endian."""

IQ4_NL_VALUES = [
-127, -104, -83, -65, -49, -35, -22, -10, 1, 13, 25, 38, 53, 69, 89, 113,
]
"""ggml's `kvalues_iq4nl`: the 16 non-linear levels IQ4_NL and IQ4_XS use."""
