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
from faiss.Metrics import map_indexed

__all__ = ["ScalarQuantizer", "QT_8bit", "QT_4bit", "QT_8bit_uniform", "train", "compute_codes", "decode", "IndexScalarQuantizer", "sq_add", "sq_search"]

QT_8bit = 0
"""faiss's `ScalarQuantizer.QT_8bit`: one byte per component over the trained [min, max] range.

# Examples
```osprey
from faiss import ScalarQuant
print(f"[{ScalarQuant.QT_8bit}]")
```
```output
[0]
```
"""
QT_4bit = 1
"""faiss's `ScalarQuantizer.QT_4bit`: a nibble per component over the trained [min, max] range.

# Examples
```osprey
from faiss import ScalarQuant
print(f"[{ScalarQuant.QT_4bit}]")
```
```output
[1]
```
"""
QT_8bit_uniform = 2
"""faiss's `ScalarQuantizer.QT_8bit_uniform`: one byte per component over one range shared by every dimension.

# Examples
```osprey
from faiss import ScalarQuant
print(f"[{ScalarQuant.QT_8bit_uniform}]")
```
```output
[2]
```
"""


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
    """faiss's `ScalarQuantizer(d, qtype)` constructor.

    # Examples
    ```osprey
    from faiss import ScalarQuant
    sq = ScalarQuant.ScalarQuantizer(2, ScalarQuant.QT_8bit)
    print(f"[{sq.d}] [{sq.qtype}] {toString(sq.trained)}")
    ```
    ```output
    [2] [0] []
    ```
    """
    return ScalarQuantizerState(d=d, qtype=qtype, trained=[])


def train(sq, x):
    """faiss's `sq.train(x)` with RS_minmax: per-dimension min/range (8/4-bit)
    or global min/range (uniform).

    # Examples
    ```osprey
    from faiss import ScalarQuant

    @handler(Arith)
    class wrapped:
        def overflow(op, l, r, wrapped_value):
            return wrapped_value

        def divideByZero(op, l):
            return 0.0

    def show():
        with wrapped:
            per_dim = ScalarQuant.train(ScalarQuant.ScalarQuantizer(2, ScalarQuant.QT_8bit), [[0.0, 2.0], [4.0, 6.0]])
            shared = ScalarQuant.train(ScalarQuant.ScalarQuantizer(2, ScalarQuant.QT_8bit_uniform), [[0.0, 2.0], [4.0, 6.0]])
            print(f"{toString(per_dim.trained)} {toString(shared.trained)}")

    show()
    ```
    ```output
    [0.0, 2.0, 4.0, 4.0] [0.0, 6.0]
    ```
    """
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
    return fold(range(len(x)), exp(1000.0), lambda acc, i: min(acc, row_min(unwrap_or(listGet(x, i), []))))


def matrix_max(x):
    """The largest value anywhere in `x`."""
    return fold(range(len(x)), 0.0 - exp(1000.0), lambda acc, i: max(acc, row_max(unwrap_or(listGet(x, i), []))))


def column_min(x, j):
    """The smallest value in column `j`."""
    return fold(range(len(x)), exp(1000.0), lambda acc, i: min(acc, unwrap_or(listGet(unwrap_or(listGet(x, i), []), j), 0.0)))


def column_max(x, j):
    """The largest value in column `j`."""
    return fold(range(len(x)), 0.0 - exp(1000.0), lambda acc, i: max(acc, unwrap_or(listGet(unwrap_or(listGet(x, i), []), j), 0.0)))


def row_min(row):
    return fold(range(len(row)), exp(1000.0), lambda acc, i: min(acc, unwrap_or(listGet(row, i), 0.0)))


def row_max(row):
    return fold(range(len(row)), 0.0 - exp(1000.0), lambda acc, i: max(acc, unwrap_or(listGet(row, i), 0.0)))


def per_qtype(sq, rows, exact, four_bit, uniform):
    """Applies the code function this quantiser's `qtype` selects to every row."""
    match sq.qtype:
        case 0:
            return mapList(rows, lambda row: exact(sq, row))
        case 1:
            return mapList(rows, lambda row: four_bit(sq, row))
        case 2:
            return mapList(rows, lambda row: uniform(sq, row))
        case _:
            return []


def compute_codes(sq, x):
    """faiss's `sq.compute_codes(x)`: the exact byte codes, one list per row.

    # Examples
    ```osprey
    from faiss import ScalarQuant

    @handler(Arith)
    class wrapped:
        def overflow(op, l, r, wrapped_value):
            return wrapped_value

        def divideByZero(op, l):
            return 0.0

    def show():
        with wrapped:
            x = [[0.0, 2.0], [2.0, 4.0], [4.0, 6.0]]
            sq8 = ScalarQuant.train(ScalarQuant.ScalarQuantizer(2, ScalarQuant.QT_8bit), x)
            sq4 = ScalarQuant.train(ScalarQuant.ScalarQuantizer(2, ScalarQuant.QT_4bit), x)
            print(f"{toString(ScalarQuant.compute_codes(sq8, x))} {toString(ScalarQuant.compute_codes(sq4, x))}")

    show()
    ```
    ```output
    [[0, 0], [127, 127], [255, 255]] [[0], [119], [255]]
    ```
    """
    return per_qtype(sq, x, codes_8bit, codes_4bit, codes_uniform)


def vmin_at(sq, j):
    return unwrap_or(listGet(sq.trained, j), 0.0)


def vdiff_at(sq, j):
    return unwrap_or(listGet(sq.trained, sq.d + j), 0.0)


def code_row(sq, row, hi):
    """One code per dimension, each in `hi` levels."""
    return toList(map(range(sq.d), lambda j: code_of(vmin_at(sq, j), vdiff_at(sq, j), unwrap_or(listGet(row, j), 0.0), hi)))


def codes_8bit(sq, row):
    """One byte per value for QT_8bit."""
    return code_row(sq, row, 255)


def codes_uniform(sq, row):
    """One byte per value for QT_8bit_uniform (shared vmin/vdiff)."""
    return toList(map(range(sq.d), lambda j: code_of(unwrap_or(listGet(sq.trained, 0), 0.0), unwrap_or(listGet(sq.trained, 1), 0.0), unwrap_or(listGet(row, j), 0.0), 255)))


def codes_4bit(sq, row):
    """Two 4-bit codes per byte for QT_4bit: even dim low, odd dim high."""
    nibbles = code_row(sq, row, 15)
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
    """faiss's `sq.decode(codes)`: reconstruct each code at the cell centre.

    # Examples
    ```osprey
    from faiss import ScalarQuant

    @handler(Arith)
    class wrapped:
        def overflow(op, l, r, wrapped_value):
            return wrapped_value

        def divideByZero(op, l):
            return 0.0

    def show():
        with wrapped:
            sq8 = ScalarQuant.train(ScalarQuant.ScalarQuantizer(2, ScalarQuant.QT_8bit), [[0.0, 2.0], [4.0, 6.0]])
            print(f"{toString(ScalarQuant.decode(sq8, [[0, 127]]))}")

    show()
    ```
    ```output
    [[0.00784313725490196, 4.0]]
    ```
    """
    return per_qtype(sq, codes, decode_8bit, decode_4bit, decode_uniform)


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
    untrained index (train it with `train`, then `sq_add`).

    # Examples
    ```osprey
    from faiss import ScalarQuant
    index = ScalarQuant.IndexScalarQuantizer(2, ScalarQuant.QT_8bit, "L2")
    print(f"[{index.d}] [{index.qtype}] [{index.metric}] {toString(index.codes)}")
    ```
    ```output
    [2] [0] [L2] []
    ```
    """
    return IndexSQ(d=d, qtype=qtype, metric=metric, sq=ScalarQuantizer(d, qtype), codes=[])


def sq_add(index, sq, xb):
    """Append the codes of `xb` to the (trained) index.

    # Examples
    ```osprey
    from faiss import ScalarQuant

    @handler(Arith)
    class wrapped:
        def overflow(op, l, r, wrapped_value):
            return wrapped_value

        def divideByZero(op, l):
            return 0.0

    def show():
        with wrapped:
            x = [[0.0, 2.0], [2.0, 4.0], [4.0, 6.0]]
            index = ScalarQuant.IndexScalarQuantizer(2, ScalarQuant.QT_8bit, "L2")
            sq = ScalarQuant.train(ScalarQuant.ScalarQuantizer(2, ScalarQuant.QT_8bit), x)
            loaded = ScalarQuant.sq_add(index, sq, x)
            print(f"{toString(loaded.codes)} {toString(index.codes)}")

    show()
    ```
    ```output
    [[0, 0], [127, 127], [255, 255]] []
    ```
    """
    return IndexSQ(d=index.d, qtype=index.qtype, metric=index.metric, sq=sq, codes=listConcat(index.codes, compute_codes(sq, xb)))


def sq_search(index, xq, k):
    """faiss's `IndexScalarQuantizer.search`: reconstruct every code, measure
    the metric to each query and heap-select. Reconstruction is float32, so
    distances compare at rtol 1e-6 (labels stay exact on well-separated data).

    # Examples
    ```osprey
    from faiss import ScalarQuant

    @handler(Arith)
    class wrapped:
        def overflow(op, l, r, wrapped_value):
            return wrapped_value

        def divideByZero(op, l):
            return 0.0

    def show():
        with wrapped:
            sq = ScalarQuant.train(ScalarQuant.ScalarQuantizer(2, ScalarQuant.QT_8bit), [[0.0, 2.0], [4.0, 6.0]])
            index = ScalarQuant.sq_add(ScalarQuant.IndexScalarQuantizer(2, ScalarQuant.QT_8bit, "L2"), sq, [[0.0, 2.0], [4.0, 6.0]])
            result = ScalarQuant.sq_search(index, [[2.0, 4.0]], 2)
            print(f"{toString(result.distances)} {toString(result.labels)}")

    show()
    ```
    ```output
    [[7.937377931564783, 8.062868127643213]] [[0, 1]]
    ```
    """
    rows = map_indexed(index.codes, lambda j: decode_one(index.sq, unwrap_or(listGet(index.codes, j), [])))
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

