# osprey: flavor=python
# osprey: namespace=flash_attn
# osprey: module=Core
"""Shared geometry and masking for @osprey/flash_attn.

The port works on flat float lists, so the shapes flash-attn reads off its tensors
are carried by a `Shapes` record. Causal masking is bottom-right aligned exactly like
flash-attn 2.1+ when seqlen_q != seqlen_k (query i attends key j when
`j <= i + seqlen_k - seqlen_q`), window_size is local attention (`-1` means unlimited),
and GQA maps a query head to key/value head `h // (heads_q // heads_k)`. ALiBi adds
`-slope * |i + seqlen_k - seqlen_q - j|` to each score, flash-attn's documented bias.
Implements [PORT-DEVICE], [PORT-SCOPE] from spec 0043.
"""

__all__ = [
    "Shapes", "FlashResult", "Bounds", "Row", "Dim4",
    "max_f", "min_i", "kv_head", "q_index", "kv_index", "v_at", "dot",
    "mask_bounds", "alibi_bias", "dim4", "slice_flat",
]


class Shapes:
    """The shape of the flat q, k, v lists: batch, seqlen_q, seqlen_k, heads_q,
    heads_k and headdim. flash-attn reads these off the tensors; flat lists carry
    no shape, so the caller supplies it."""
    batch: int
    seqlen_q: int
    seqlen_k: int
    heads_q: int
    heads_k: int
    headdim: int


class FlashResult:
    """`flash_attn_func`'s outputs: the attended output as
    [batch, seqlen_q, heads_q, headdim], the per-row softmax log-sum-exp as
    [batch, heads_q, seqlen_q], and the attention probabilities as
    [batch, heads_q, seqlen_q, seqlen_k] when return_attn_probs asked for them,
    else `[]`."""
    out: list[float]
    lse: list[float]
    probs: list[float]


class Bounds:
    """The half-open key range [lo, hi) one query row attends to."""
    lo: int
    hi: int


class Row:
    """One query row's online-softmax running state after the key pass: the running
    max `m`, the running sum `l` and the rescaled value accumulator `acc`."""
    m: float
    l: float
    acc: list[float]


class Dim4:
    """A flat index decomposed over a 4-D [a, b, c, d] shape, last dimension fastest."""
    a: int
    b: int
    c: int
    d: int


def max_f(a, b):
    """The larger of two floats, for the online softmax's running maximum.

    # Examples
    ```osprey
    from flash_attn import Core
    print(f"[{Core.max_f(1.5, 2.5)}] [{Core.max_f(-0.5, -1.5)}] [{Core.max_f(2.0, 2.0)}]")
    ```
    ```output
    [2.5] [-0.5] [2.0]
    ```
    """
    return b if b > a else a


def min_i(a, b):
    """The smaller of two ints, for block-end clamps.

    # Examples
    ```osprey
    from flash_attn import Core
    print(f"[{Core.min_i(10, 7)}] [{Core.min_i(3, 7)}]")
    ```
    ```output
    [7] [3]
    ```
    """
    return a if a < b else b


def kv_head(shapes, h):
    """The key/value head a query head reads under GQA/MQA: heads_k divides heads_q.

    # Examples
    ```osprey
    from flash_attn import Core

    @handler(Arith)
    class wrapped:
        def overflow(op, l, r, wrapped_value):
            return wrapped_value

        def remainderByZero(l):
            return 0

    def show():
        with wrapped:
            gqa = Core.Shapes(batch=1, seqlen_q=2, seqlen_k=2, heads_q=4, heads_k=2, headdim=2)
            mqa = Core.Shapes(batch=1, seqlen_q=2, seqlen_k=2, heads_q=1, heads_k=1, headdim=2)
            print(f"[{Core.kv_head(gqa, 0)}] [{Core.kv_head(gqa, 3)}] [{Core.kv_head(mqa, 0)}]")

    show()
    ```
    ```output
    [0] [1] [0]
    ```
    """
    return h // (shapes.heads_q // shapes.heads_k)


def flat_index(seqlen, heads, headdim, b, i, h, d):
    """Flat index of x[b, i, h, d] in a [batch, seqlen, heads, headdim] list."""
    return ((b * seqlen + i) * heads + h) * headdim + d


def q_index(shapes, b, i, h, d):
    """Flat index of q[b, i, h, d] in the [batch, seqlen_q, heads_q, headdim] list.

    # Examples
    ```osprey
    from flash_attn import Core

    @handler(Arith)
    class wrapped:
        def overflow(op, l, r, wrapped_value):
            return wrapped_value

    def show():
        with wrapped:
            shapes = Core.Shapes(batch=1, seqlen_q=2, seqlen_k=2, heads_q=2, heads_k=2, headdim=2)
            print(f"[{Core.q_index(shapes, 0, 0, 0, 0)}] [{Core.q_index(shapes, 0, 1, 1, 1)}]")

    show()
    ```
    ```output
    [0] [7]
    ```
    """
    return flat_index(shapes.seqlen_q, shapes.heads_q, shapes.headdim, b, i, h, d)


def kv_index(shapes, b, j, h, d):
    """Flat index of k or v[b, j, h, d] in the [batch, seqlen_k, heads_k, headdim] list.

    # Examples
    ```osprey
    from flash_attn import Core

    @handler(Arith)
    class wrapped:
        def overflow(op, l, r, wrapped_value):
            return wrapped_value

    def show():
        with wrapped:
            shapes = Core.Shapes(batch=1, seqlen_q=2, seqlen_k=3, heads_q=4, heads_k=2, headdim=2)
            print(f"[{Core.kv_index(shapes, 0, 0, 0, 0)}] [{Core.kv_index(shapes, 0, 2, 1, 1)}]")

    show()
    ```
    ```output
    [0] [11]
    ```
    """
    return flat_index(shapes.seqlen_k, shapes.heads_k, shapes.headdim, b, j, h, d)


def v_at(v, shapes, b, j, h, d):
    """v[b, j, kv_head(h), d] for a query head h, applying the GQA head mapping.

    # Examples
    ```osprey
    from flash_attn import Core

    @handler(Arith)
    class wrapped:
        def overflow(op, l, r, wrapped_value):
            return wrapped_value

        def remainderByZero(l):
            return 0

    def show():
        with wrapped:
            shapes = Core.Shapes(batch=1, seqlen_q=2, seqlen_k=2, heads_q=2, heads_k=1, headdim=2)
            v = [10.0, 11.0, 20.0, 21.0]
            print(f"[{Core.v_at(v, shapes, 0, 0, 0, 0)}] [{Core.v_at(v, shapes, 0, 1, 1, 0)}] [{Core.v_at(v, shapes, 0, 5, 0, 0)}]")

    show()
    ```
    ```output
    [10.0] [20.0] [0.0]
    ```
    """
    return unwrap_or(listGet(v, kv_index(shapes, b, j, kv_head(shapes, h), d)), 0.0)


def dot(q, k, shapes, b, i, h, j):
    """q_i . k_j over headdim for query head h, using the GQA key head.

    # Examples
    ```osprey
    from flash_attn import Core

    @handler(Arith)
    class wrapped:
        def overflow(op, l, r, wrapped_value):
            return wrapped_value

        def remainderByZero(l):
            return 0

    def show():
        with wrapped:
            shapes = Core.Shapes(batch=1, seqlen_q=1, seqlen_k=2, heads_q=1, heads_k=1, headdim=2)
            q = [1.0, 2.0]
            k = [3.0, 4.0, 0.0, 5.0]
            print(f"[{Core.dot(q, k, shapes, 0, 0, 0, 0)}] [{Core.dot(q, k, shapes, 0, 0, 0, 1)}] [{Core.dot(q, k, shapes, 0, 0, 0, 9)}]")

    show()
    ```
    ```output
    [11.0] [10.0] [0.0]
    ```
    """
    hk = kv_head(shapes, h)
    return fold(range(shapes.headdim), 0.0, lambda acc, d: acc + unwrap_or(listGet(q, q_index(shapes, b, i, h, d)), 0.0) * unwrap_or(listGet(k, kv_index(shapes, b, j, hk, d)), 0.0))


def mask_bounds(shapes, causal, window_left, window_right, i):
    """The [lo, hi) key range query i attends to. Causal is bottom-right aligned
    (j <= i + seqlen_k - seqlen_q, flash-attn 2.1+); window_left/window_right are
    local-attention distances, -1 meaning unlimited. A fully-masked row is [lo, lo).

    # Examples
    ```osprey
    from flash_attn import Core

    @handler(Arith)
    class wrapped:
        def overflow(op, l, r, wrapped_value):
            return wrapped_value

    def show():
        with wrapped:
            causal = Core.Shapes(batch=1, seqlen_q=2, seqlen_k=3, heads_q=1, heads_k=1, headdim=2)
            local = Core.Shapes(batch=1, seqlen_q=2, seqlen_k=4, heads_q=1, heads_k=1, headdim=2)
            masked = Core.Shapes(batch=1, seqlen_q=3, seqlen_k=2, heads_q=1, heads_k=1, headdim=2)
            b0 = Core.mask_bounds(causal, True, -1, -1, 0)
            b1 = Core.mask_bounds(causal, True, -1, -1, 1)
            w = Core.mask_bounds(local, False, 1, 1, 2)
            m = Core.mask_bounds(masked, True, -1, -1, 0)
            print(f"[{b0.lo}, {b0.hi}] [{b1.lo}, {b1.hi}] [{w.lo}, {w.hi}] [{m.lo}, {m.hi}]")

    show()
    ```
    ```output
    [0, 2] [0, 3] [1, 4] [0, 0]
    ```
    """
    lo = 0 if window_left < 0 else (i - window_left if i - window_left > 0 else 0)
    causal_hi = min_i(shapes.seqlen_k, i + shapes.seqlen_k - shapes.seqlen_q + 1) if causal else shapes.seqlen_k
    hi = min_i(causal_hi, i + window_right + 1) if window_right >= 0 else causal_hi
    return Bounds(lo=lo, hi=(lo if lo > hi else hi))


def alibi_bias(alibi_slopes, shapes, h, i, j):
    """The ALiBi bias -slope * |i + seqlen_k - seqlen_q - j| for query head h,
    flash-attn's documented convention; an empty `alibi_slopes` means no bias.

    # Examples
    ```osprey
    from flash_attn import Core

    @handler(Arith)
    class wrapped:
        def overflow(op, l, r, wrapped_value):
            return wrapped_value

    def show():
        with wrapped:
            shapes = Core.Shapes(batch=1, seqlen_q=2, seqlen_k=2, heads_q=2, heads_k=2, headdim=2)
            print(f"[{Core.alibi_bias([], shapes, 0, 0, 1)}] [{Core.alibi_bias([0.5, 1.0], shapes, 0, 0, 1)}] [{Core.alibi_bias([0.5, 1.0], shapes, 1, 1, 0)}]")

    show()
    ```
    ```output
    [0.0] [-0.5] [-1.0]
    ```
    """
    if len(alibi_slopes) == 0:
        return 0.0
    else:
        dist = abs(toFloat(i + shapes.seqlen_k - shapes.seqlen_q - j))
        return 0.0 - unwrap_or(listGet(alibi_slopes, h), 0.0) * dist


def dim4(_shape_a, shape_b, shape_c, shape_d, s):
    """Decompose a flat index s over [shape_a, shape_b, shape_c, shape_d], last
    dimension fastest.

    # Examples
    ```osprey
    from flash_attn import Core

    @handler(Arith)
    class wrapped:
        def overflow(op, l, r, wrapped_value):
            return wrapped_value

        def remainderByZero(l):
            return 0

    def show():
        with wrapped:
            first = Core.dim4(2, 3, 2, 2, 0)
            last = Core.dim4(2, 3, 2, 2, 11)
            print(f"[{first.a}, {first.b}, {first.c}, {first.d}] [{last.a}, {last.b}, {last.c}, {last.d}]")

    show()
    ```
    ```output
    [0, 0, 0, 0] [0, 2, 1, 1]
    ```
    """
    d = s % shape_d
    r1 = s // shape_d
    c = r1 % shape_c
    r2 = r1 // shape_c
    b = r2 % shape_b
    a = r2 // shape_b
    return Dim4(a=a, b=b, c=c, d=d)


def slice_flat(xs, start, n):
    """A contiguous run xs[start .. start+n) as a new list, for varlen segments.

    # Examples
    ```osprey
    from flash_attn import Core

    xs = [1.0, 2.0, 3.0]
    print(f"{toString(Core.slice_flat(xs, 1, 2))} {toString(Core.slice_flat(xs, 0, 0))}")
    ```
    ```output
    [2.0, 3.0] []
    ```
    """
    return toList(map(range(n), lambda i: unwrap_or(listGet(xs, start + i), 0.0)))
