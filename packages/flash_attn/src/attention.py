# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=flash_attn
# osprey: module=Attention
"""flash_attn_func and flash_attn_varlen_func, as FlashAttention's online-softmax
tiled pass.

Each output element is one build kernel: it tiles the key range into blocks and
runs the online softmax (running max m, running sum l, rescaled accumulator), so the
[seqlen_q, seqlen_k] score matrix is never materialised. dropout_p is fixed at 0 and
binary64 arithmetic throughout. When return_attn_probs is true the probabilities come
from `Naive.naive_attention`, which does materialise them. Implements [PORT-DEVICE]
from spec 0043; the same body moves behind @osprey/numpy's Storage.Buffer when it lands.
"""
from flash_attn import Core, Naive
from flash_attn.Core import Shapes, FlashResult, Row, max_f, min_i, dot, mask_bounds, alibi_bias, dim4, slice_flat, v_at

__all__ = ["flash_attn_func", "flash_attn_varlen_func"]

# Key block size, as in flash-attn's tiled kernels; any seqlen_k above it
# exercises more than one block.
BLOCK_SIZE = 32


def block_count(lo, hi):
    """The number of key blocks in [lo, hi)."""
    span = hi - lo
    return span // BLOCK_SIZE + (1 if span % BLOCK_SIZE != 0 else 0)


def update_row(q, k, v, shapes, scale, alibi, b, i, h, j, r):
    """One online-softmax step: fold key j's score into the running state r."""
    score = scale * dot(q, k, shapes, b, i, h, j) + alibi_bias(alibi, shapes, h, i, j)
    new_m = max_f(r.m, score)
    rescale = exp(r.m - new_m)
    p = exp(score - new_m)
    acc = toList(map(range(shapes.headdim), lambda d: unwrap_or(listGet(r.acc, d), 0.0) * rescale + v_at(v, shapes, b, j, h, d) * p))
    return Row(m=new_m, l=r.l * rescale + p, acc=acc)


def block_pass(q, k, v, shapes, scale, alibi, b, i, h, j_start, j_end, row):
    """The online-softmax steps over one key block [j_start, j_end)."""
    return fold(range(j_start, j_end), row, lambda r, j: update_row(q, k, v, shapes, scale, alibi, b, i, h, j, r))



def empty_row(shapes):
    """The online-softmax state before any key block: max -inf, sum 0, zero accumulator."""
    return Row(m=0.0 - exp(1000.0), l=0.0, acc=toList(map(range(shapes.headdim), lambda _: 0.0)))


def attend_row(q, k, v, shapes, scale, causal, wl, wr, alibi, b, i, h):
    """The online-softmax pass for one query row, tiled over key blocks."""
    bounds = mask_bounds(shapes, causal, wl, wr, i)
    if bounds.hi <= bounds.lo:
        return empty_row(shapes)
    else:
        seed = empty_row(shapes)
        return fold(range(block_count(bounds.lo, bounds.hi)), seed, lambda row, bi: block_pass(q, k, v, shapes, scale, alibi, b, i, h, bounds.lo + bi * BLOCK_SIZE, min_i(bounds.hi, bounds.lo + (bi + 1) * BLOCK_SIZE), row))


def out_element(q, k, v, shapes, scale, causal, wl, wr, alibi, o):
    """Output element o of [batch, seqlen_q, heads_q, headdim]: one kernel, the full
    online-softmax pass for its (batch, query, head)."""
    d4 = dim4(shapes.batch, shapes.seqlen_q, shapes.heads_q, shapes.headdim, o)
    row = attend_row(q, k, v, shapes, scale, causal, wl, wr, alibi, d4.a, d4.b, d4.c)
    return 0.0 if row.l == 0.0 else unwrap_or(listGet(row.acc, d4.d), 0.0) / row.l


def lse_element(q, k, v, shapes, scale, causal, wl, wr, alibi, e):
    """Softmax log-sum-exp element e of [batch, heads_q, seqlen_q]: running max plus
    the log of the running sum, -inf for a fully-masked row."""
    d4 = dim4(shapes.batch, shapes.heads_q, shapes.seqlen_q, 1, e)
    row = attend_row(q, k, v, shapes, scale, causal, wl, wr, alibi, d4.a, d4.c, d4.b)
    return 0.0 - exp(1000.0) if row.l == 0.0 else row.m + ln(row.l)


def flash_attn_func(q, k, v, shapes, softmax_scale, causal, window_left, window_right, alibi_slopes, return_attn_probs):
    """flash-attn's flash_attn_func over flat [batch, seqlen, heads, headdim] lists.

    softmax_scale is qk^T's scale; causal is bottom-right aligned; window_left and
    window_right are local-attention distances (-1 unlimited); alibi_slopes holds one
    slope per query head ([] for none); dropout_p is always 0. Returns out, the
    softmax log-sum-exp, and the probabilities (when return_attn_probs)."""
    n_out = shapes.batch * shapes.seqlen_q * shapes.heads_q * shapes.headdim
    out = toList(map(range(n_out), lambda o: out_element(q, k, v, shapes, softmax_scale, causal, window_left, window_right, alibi_slopes, o)))
    n_lse = shapes.batch * shapes.heads_q * shapes.seqlen_q
    lse = toList(map(range(n_lse), lambda e: lse_element(q, k, v, shapes, softmax_scale, causal, window_left, window_right, alibi_slopes, e)))
    probs = Naive.naive_attention(q, k, v, shapes, softmax_scale, causal, window_left, window_right, alibi_slopes).probs if return_attn_probs else []
    return FlashResult(out=out, lse=lse, probs=probs)


def append_seq(acc, q, k, v, cu_q, cu_k, hq, hk, hd, scale, causal, wl, wr, alibi, ret, b):
    """One varlen sequence's flash_attn_func result appended to the running total."""
    sq = unwrap_or(listGet(cu_q, b + 1), 0) - unwrap_or(listGet(cu_q, b), 0)
    sk = unwrap_or(listGet(cu_k, b + 1), 0) - unwrap_or(listGet(cu_k, b), 0)
    q_off = unwrap_or(listGet(cu_q, b), 0) * hq * hd
    k_off = unwrap_or(listGet(cu_k, b), 0) * hk * hd
    shapes = Shapes(batch=1, seqlen_q=sq, seqlen_k=sk, heads_q=hq, heads_k=hk, headdim=hd)
    r = flash_attn_func(slice_flat(q, q_off, sq * hq * hd), slice_flat(k, k_off, sk * hk * hd), slice_flat(v, k_off, sk * hk * hd), shapes, scale, causal, wl, wr, alibi, ret)
    return FlashResult(out=listConcat(acc.out, r.out), lse=listConcat(acc.lse, r.lse), probs=listConcat(acc.probs, r.probs))


def flash_attn_varlen_func(q, k, v, cu_seqlens_q, cu_seqlens_k, _max_seqlen_q, _max_seqlen_k, heads_q, heads_k, headdim, softmax_scale, causal, window_left, window_right, alibi_slopes, return_attn_probs):
    """flash-attn's flash_attn_varlen_func: independent flash_attn_func over each
    sequence delimited by the cumulative lengths, outputs concatenated. max_seqlen_q
    and max_seqlen_k are accepted for API parity and unused (the shapes come off the
    cumulative lengths)."""
    return fold(range(len(cu_seqlens_q) - 1), FlashResult(out=[], lse=[], probs=[]), lambda acc, b: append_seq(acc, q, k, v, cu_seqlens_q, cu_seqlens_k, heads_q, heads_k, headdim, softmax_scale, causal, window_left, window_right, alibi_slopes, return_attn_probs, b))
