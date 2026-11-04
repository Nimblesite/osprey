# osprey: flavor=python
# osprey: namespace=faiss
# osprey: module=ScalarQuant
"""faiss's `ScalarQuantizer` (QT_8bit, QT_4bit, QT_8bit_uniform) with RS_minmax
training, and `IndexScalarQuantizer` search.

`train` records the per-dimension (or global, for the uniform variant) min and
range; `compute_codes` maps each value to its byte code exactly as faiss does
(`(int)(255 * clamp((x - vmin) / vdiff, 0, 1))` for 8-bit, nibble-packed for
4-bit), and `decode` reconstructs with the `(code + 0.5) / 255` centre. Codes
compare exactly (the suite trains on data whose ranges are powers of two, so
the quantizer input is dyadic and faiss's float32 result equals the binary64
the port computes); reconstruction is not dyadic, so decode and search compare
at rtol 1e-6. QT_fp16 needs the compiler's `floatFromBits` builtin, which has
not landed, so it is not ported yet (see README).
"""

from faiss.Flat import search_rows

__all__ = ["ScalarQuantizer", "QT_8bit", "QT_4bit", "QT_8bit_uniform", "train", "compute_codes", "decode", "IndexScalarQuantizer", "sq_add", "sq_search"]

QT_8bit = 0
"""faiss's `ScalarQuantizer.QT_8bit`: one byte per component over the trained [min, max] range."""
QT_4bit = 1
"""faiss's `ScalarQuantizer.QT_4bit`: a nibble per component over the trained [min, max] range."""
QT_8bit_uniform = 2
"""faiss's `ScalarQuantizer.QT_8bit_uniform`: one byte per component over one range shared by every dimension."""


class ScalarQuantizerState:
    """A trained scalar quantizer: `d`, `qtype` and the flat `trained` vector
    (per-dim vmin then vdiff for 8/4-bit, [vmin, vdiff] for uniform)."""
    d: int
    qtype: int
    trained: list[float]


class IndexSQ:
    """An exact-search index over scalar-quantized codes: `d`, `qtype`, `metric`
    ("L2" or "IP"), the trained `sq` and the stored `codes` (one byte list per
    vector)."""
    d: int
    qtype: int
    metric: str
    sq: ScalarQuantizerState
    codes: list[list[int]]


def ScalarQuantizer(d, qtype):
    """faiss's `ScalarQuantizer(d, qtype)` constructor."""
    return ScalarQuantizerState(d=d, qtype=qtype, trained=[])


def train(sq, x):
    """faiss's `sq.train(x)` with RS_minmax: per-dimension min/range (8/4-bit)
    or global min/range (uniform)."""
    if sq.qtype == QT_8bit_uniform:
        return ScalarQuantizerState(d=sq.d, qtype=sq.qtype, trained=train_uniform(x))
    else:
        return ScalarQuantizerState(d=sq.d, qtype=sq.qtype, trained=train_nonuniform(x, sq.d))


def train_uniform(x):
    """The global `[vmin, vdiff]` faiss's `train_Uniform(RS_minmax)` stores."""
    vmin = matrix_min(x)
    vmax = matrix_max(x)
    return [vmin, vmax - vmin]


def train_nonuniform(x, d):
    """Per-dimension `[vmin..., vdiff...]` from `train_NonUniform(RS_minmax)`."""
    vmin = toList(map(range(d), lambda j: column_min(x, j)))
    vdiff = toList(map(range(d), lambda j: column_max(x, j) - unwrap_or(listGet(vmin, j), 0.0)))
    return listConcat(vmin, vdiff)


def matrix_min(x):
    """The smallest value anywhere in `x`."""
    return fold(range(len(x)), exp(1000.0), lambda acc, i: min_f(acc, row_min(unwrap_or(listGet(x, i), []))))


def matrix_max(x):
    """The largest value anywhere in `x`."""
    return fold(range(len(x)), 0.0 - exp(1000.0), lambda acc, i: max_f(acc, row_max(unwrap_or(listGet(x, i), []))))


def column_min(x, j):
    """The smallest value in column `j`."""
    return fold(range(len(x)), exp(1000.0), lambda acc, i: min_f(acc, unwrap_or(listGet(unwrap_or(listGet(x, i), []), j), 0.0)))


def column_max(x, j):
    """The largest value in column `j`."""
    return fold(range(len(x)), 0.0 - exp(1000.0), lambda acc, i: max_f(acc, unwrap_or(listGet(unwrap_or(listGet(x, i), []), j), 0.0)))


def row_min(row):
    return fold(range(len(row)), exp(1000.0), lambda acc, i: min_f(acc, unwrap_or(listGet(row, i), 0.0)))


def row_max(row):
    return fold(range(len(row)), 0.0 - exp(1000.0), lambda acc, i: max_f(acc, unwrap_or(listGet(row, i), 0.0)))


def min_f(a, b):
    return a if a < b else b


def max_f(a, b):
    return a if a > b else b


def compute_codes(sq, x):
    """faiss's `sq.compute_codes(x)`: the exact byte codes, one list per row."""
    match sq.qtype:
        case 0:
            return mapList(x, lambda row: codes_8bit(sq, row))
        case 1:
            return mapList(x, lambda row: codes_4bit(sq, row))
        case 2:
            return mapList(x, lambda row: codes_uniform(sq, row))
        case _:
            return []


def vmin_at(sq, j):
    return unwrap_or(listGet(sq.trained, j), 0.0)


def vdiff_at(sq, j):
    return unwrap_or(listGet(sq.trained, sq.d + j), 0.0)


def codes_8bit(sq, row):
    """One byte per value for QT_8bit."""
    return toList(map(range(sq.d), lambda j: code_of(vmin_at(sq, j), vdiff_at(sq, j), unwrap_or(listGet(row, j), 0.0), 255)))


def codes_uniform(sq, row):
    """One byte per value for QT_8bit_uniform (shared vmin/vdiff)."""
    return toList(map(range(sq.d), lambda j: code_of(unwrap_or(listGet(sq.trained, 0), 0.0), unwrap_or(listGet(sq.trained, 1), 0.0), unwrap_or(listGet(row, j), 0.0), 255)))


def codes_4bit(sq, row):
    """Two 4-bit codes per byte for QT_4bit: even dim low, odd dim high."""
    nibbles = toList(map(range(sq.d), lambda j: code_of(vmin_at(sq, j), vdiff_at(sq, j), unwrap_or(listGet(row, j), 0.0), 15)))
    return toList(map(range(intDiv(sq.d + 1, 2)), lambda t: nibble_of(nibbles, 2 * t) + 16 * nibble_of(nibbles, 2 * t + 1)))


def nibble_of(nibbles, j):
    if j >= len(nibbles):
        return 0
    else:
        return unwrap_or(listGet(nibbles, j), 0)


def code_of(vmin, vdiff, v, hi):
    """faiss's `(int)(hi * clamp((v - vmin) / vdiff, 0, 1))`, exactly, via a
    floor helper (the compiler has no float-to-int builtin yet)."""
    if vdiff == 0.0:
        return 0
    else:
        return floor_in(clamp01((v - vmin) / vdiff) * toFloat(hi), hi)


def clamp01(x):
    if x < 0.0:
        return 0.0
    else:
        return 1.0 if x > 1.0 else x


def floor_in(v, hi):
    """The largest int i in [0, hi] with i <= v (v in [0, hi]; exact for dyadic
    v, which is all the suite produces)."""
    return fold(range(hi + 1), 0, lambda acc, i: i if toFloat(i) <= v else acc)


def decode(sq, codes):
    """faiss's `sq.decode(codes)`: reconstruct each code at the cell centre."""
    match sq.qtype:
        case 0:
            return mapList(codes, lambda row: decode_8bit(sq, row))
        case 1:
            return mapList(codes, lambda row: decode_4bit(sq, row))
        case 2:
            return mapList(codes, lambda row: decode_uniform(sq, row))
        case _:
            return []


def decode_8bit(sq, row):
    return toList(map(range(sq.d), lambda j: vmin_at(sq, j) + (toFloat(unwrap_or(listGet(row, j), 0)) + 0.5) / 255.0 * vdiff_at(sq, j)))


def decode_uniform(sq, row):
    vmin = unwrap_or(listGet(sq.trained, 0), 0.0)
    vdiff = unwrap_or(listGet(sq.trained, 1), 0.0)
    return toList(map(range(sq.d), lambda j: vmin + (toFloat(unwrap_or(listGet(row, j), 0)) + 0.5) / 255.0 * vdiff))


def decode_4bit(sq, row):
    return toList(map(range(sq.d), lambda j: vmin_at(sq, j) + (toFloat(nibble_at(row, j)) + 0.5) / 15.0 * vdiff_at(sq, j)))


def nibble_at(row, j):
    byte = unwrap_or(listGet(row, intDiv(j, 2)), 0)
    return byte % 16 if j % 2 == 0 else intDiv(byte, 16)


def IndexScalarQuantizer(d, qtype, metric):
    """faiss's `IndexScalarQuantizer(d, qtype, metric)` constructor: an
    untrained index (train it with `train`, then `sq_add`)."""
    return IndexSQ(d=d, qtype=qtype, metric=metric, sq=ScalarQuantizer(d, qtype), codes=[])


def sq_add(index, sq, xb):
    """Append the codes of `xb` to the (trained) index."""
    return IndexSQ(d=index.d, qtype=index.qtype, metric=index.metric, sq=sq, codes=listConcat(index.codes, compute_codes(sq, xb)))


def sq_search(index, xq, k):
    """faiss's `IndexScalarQuantizer.search`: reconstruct every code, measure
    the metric to each query and heap-select. Reconstruction is float32, so
    distances compare at rtol 1e-6 (labels stay exact on well-separated data)."""
    rows = toList(map(range(len(index.codes)), lambda j: decode_one(index.sq, unwrap_or(listGet(index.codes, j), []))))
    return search_rows(index.metric, rows, xq, k)


def decode_one(sq, code):
    match sq.qtype:
        case 0:
            return decode_8bit(sq, code)
        case 1:
            return decode_4bit(sq, code)
        case 2:
            return decode_uniform(sq, code)
        case _:
            return []



