# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
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
    """The larger of two floats, for the online softmax's running maximum."""
    return b if b > a else a


def min_i(a, b):
    """The smaller of two ints, for block-end clamps."""
    return a if a < b else b


def kv_head(shapes, h):
    """The key/value head a query head reads under GQA/MQA: heads_k divides heads_q."""
    return h // (shapes.heads_q // shapes.heads_k)


def flat_index(seqlen, heads, headdim, b, i, h, d):
    """Flat index of x[b, i, h, d] in a [batch, seqlen, heads, headdim] list."""
    return ((b * seqlen + i) * heads + h) * headdim + d


def q_index(shapes, b, i, h, d):
    """Flat index of q[b, i, h, d] in the [batch, seqlen_q, heads_q, headdim] list."""
    return flat_index(shapes.seqlen_q, shapes.heads_q, shapes.headdim, b, i, h, d)


def kv_index(shapes, b, j, h, d):
    """Flat index of k or v[b, j, h, d] in the [batch, seqlen_k, heads_k, headdim] list."""
    return flat_index(shapes.seqlen_k, shapes.heads_k, shapes.headdim, b, j, h, d)


def v_at(v, shapes, b, j, h, d):
    """v[b, j, kv_head(h), d] for a query head h, applying the GQA head mapping."""
    return unwrap_or(listGet(v, kv_index(shapes, b, j, kv_head(shapes, h), d)), 0.0)


def dot(q, k, shapes, b, i, h, j):
    """q_i . k_j over headdim for query head h, using the GQA key head."""
    hk = kv_head(shapes, h)
    return fold(range(shapes.headdim), 0.0, lambda acc, d: acc + unwrap_or(listGet(q, q_index(shapes, b, i, h, d)), 0.0) * unwrap_or(listGet(k, kv_index(shapes, b, j, hk, d)), 0.0))


def mask_bounds(shapes, causal, window_left, window_right, i):
    """The [lo, hi) key range query i attends to. Causal is bottom-right aligned
    (j <= i + seqlen_k - seqlen_q, flash-attn 2.1+); window_left/window_right are
    local-attention distances, -1 meaning unlimited. A fully-masked row is [lo, lo)."""
    lo = 0 if window_left < 0 else (i - window_left if i - window_left > 0 else 0)
    causal_hi = min_i(shapes.seqlen_k, i + shapes.seqlen_k - shapes.seqlen_q + 1) if causal else shapes.seqlen_k
    hi = min_i(causal_hi, i + window_right + 1) if window_right >= 0 else causal_hi
    return Bounds(lo=lo, hi=(lo if lo > hi else hi))


def alibi_bias(alibi_slopes, shapes, h, i, j):
    """The ALiBi bias -slope * |i + seqlen_k - seqlen_q - j| for query head h,
    flash-attn's documented convention; an empty `alibi_slopes` means no bias."""
    if len(alibi_slopes) == 0:
        return 0.0
    else:
        dist = abs(toFloat(i + shapes.seqlen_k - shapes.seqlen_q - j))
        return 0.0 - unwrap_or(listGet(alibi_slopes, h), 0.0) * dist


def dim4(_shape_a, shape_b, shape_c, shape_d, s):
    """Decompose a flat index s over [shape_a, shape_b, shape_c, shape_d], last
    dimension fastest."""
    d = s % shape_d
    r1 = s // shape_d
    c = r1 % shape_c
    r2 = r1 // shape_c
    b = r2 % shape_b
    a = r2 // shape_b
    return Dim4(a=a, b=b, c=c, d=d)


def slice_flat(xs, start, n):
    """A contiguous run xs[start .. start+n) as a new list, for varlen segments."""
    return toList(map(range(n), lambda i: unwrap_or(listGet(xs, start + i), 0.0)))
