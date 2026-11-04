# osprey: flavor=python
# osprey: namespace=flash_attn
# osprey: module=Naive
"""A naive reference attention that materialises the [seqlen_q, seqlen_k] score
matrix, for the suite to compare the online-softmax tiled pass against.

flash-attn ships no Python reference; this port adds one so the flash path is proven
against an independent implementation of the same masking, GQA, window and ALiBi
conventions. Implements [PORT-TEST-ORACLE] from spec 0043.
"""
from flash_attn.Core import FlashResult, max_f, v_at, dot, mask_bounds, alibi_bias, dim4

__all__ = ["naive_attention"]


def naive_score(q, k, shapes, scale, causal, wl, wr, alibi, s):
    """Score element s of [batch, heads_q, seqlen_q, seqlen_k]: -inf when masked."""
    d4 = dim4(shapes.batch, shapes.heads_q, shapes.seqlen_q, shapes.seqlen_k, s)
    bounds = mask_bounds(shapes, causal, wl, wr, d4.c)
    if d4.d < bounds.lo or d4.d >= bounds.hi:
        return 0.0 - exp(1000.0)
    else:
        return scale * dot(q, k, shapes, d4.a, d4.c, d4.b, d4.d) + alibi_bias(alibi, shapes, d4.b, d4.c, d4.d)


def row_peak(scores, shapes, r):
    """The maximum score of row r, or -inf when the row is fully masked."""
    return fold(range(shapes.seqlen_k), unwrap_or(listGet(scores, r * shapes.seqlen_k), 0.0), lambda a, j: max_f(a, unwrap_or(listGet(scores, r * shapes.seqlen_k + j), 0.0)))


def naive_weight(score, peak):
    """exp(score - peak) for a key, 0.0 for a masked key."""
    return 0.0 if score == 0.0 - exp(1000.0) else exp(score - peak)


def probs_at(probs, shapes, b, h, i, j):
    """probs[b, h, i, j] in the [batch, heads_q, seqlen_q, seqlen_k] list."""
    return unwrap_or(listGet(probs, ((b * shapes.heads_q + h) * shapes.seqlen_q + i) * shapes.seqlen_k + j), 0.0)


def naive_out(probs, v, shapes, o):
    """Output element o of [batch, seqlen_q, heads_q, headdim] as a weighted sum of v."""
    d4 = dim4(shapes.batch, shapes.seqlen_q, shapes.heads_q, shapes.headdim, o)
    return fold(range(shapes.seqlen_k), 0.0, lambda acc, j: acc + probs_at(probs, shapes, d4.a, d4.c, d4.b, j) * v_at(v, shapes, d4.a, j, d4.c, d4.d))


def naive_attention(q, k, v, shapes, softmax_scale, causal, window_left, window_right, alibi_slopes):
    """Attention computed the explicit way: scores, softmax with per-row max and sum,
    then the probability-weighted sum over v. Fully-masked rows yield out 0.0 and lse
    -inf."""
    n_scores = shapes.batch * shapes.heads_q * shapes.seqlen_q * shapes.seqlen_k
    scores = toList(map(range(n_scores), lambda s: naive_score(q, k, shapes, softmax_scale, causal, window_left, window_right, alibi_slopes, s)))
    n_rows = shapes.batch * shapes.heads_q * shapes.seqlen_q
    peaks = toList(map(range(n_rows), lambda r: row_peak(scores, shapes, r)))
    weights = toList(map(range(n_scores), lambda s: naive_weight(unwrap_or(listGet(scores, s), 0.0), unwrap_or(listGet(peaks, s // shapes.seqlen_k), 0.0))))
    sums = toList(map(range(n_rows), lambda r: fold(range(shapes.seqlen_k), 0.0, lambda a, j: a + unwrap_or(listGet(weights, r * shapes.seqlen_k + j), 0.0))))
    probs = toList(map(range(n_scores), lambda s: 0.0 if unwrap_or(listGet(sums, s // shapes.seqlen_k), 0.0) == 0.0 else unwrap_or(listGet(weights, s), 0.0) / unwrap_or(listGet(sums, s // shapes.seqlen_k), 0.0)))
    n_out = shapes.batch * shapes.seqlen_q * shapes.heads_q * shapes.headdim
    out = toList(map(range(n_out), lambda o: naive_out(probs, v, shapes, o)))
    lse = toList(map(range(n_rows), lambda r: 0.0 - exp(1000.0) if unwrap_or(listGet(sums, r), 0.0) == 0.0 else unwrap_or(listGet(peaks, r), 0.0) + ln(unwrap_or(listGet(sums, r), 0.0))))
    return FlashResult(out=out, lse=lse, probs=probs)
