# osprey: flavor=python
"""The @osprey/flash_attn suite: every config the oracle computed (torch SDPA in
float64 plus explicit mask/ALiBi/lse math) is run through both the online-softmax
tiled `flash_attn_func` and the naive reference, and the two are compared against
each other and the oracle. Fully-masked causal rows (seqlen_q > seqlen_k) yield out 0
and lse -inf, stored as strings and decoded with `Check.float_of`."""
from flash_attn import Attention, Core, Naive
from flash_attn_expected import Expected
from testing import Check

RTOL = 0.000000001
ATOL = 0.000000000001


class Opts:
    """The sampling options every attention config shares."""
    scale: float
    causal: bool
    window_left: int
    window_right: int
    alibi: list[float]
    ret: bool


class Case:
    """One oracle config unpacked from the parallel constants."""
    shapes: Core.Shapes
    q: list[float]
    k: list[float]
    v: list[float]
    opts: Opts


def at(xs, i, d):
    """Element i of xs, or d past the end."""
    return unwrap_or(listGet(xs, i), d)


def row(xs, i):
    """Row `i` of a table of lists, [] past the end."""
    return at(xs, i, [])


def shapes(seqlen_q, seqlen_k, heads_q, heads_k, headdim):
    """A one-batch Shapes record from the five dimensions a Core case varies."""
    return Core.Shapes(batch=1, seqlen_q=seqlen_q, seqlen_k=seqlen_k, heads_q=heads_q, heads_k=heads_k, headdim=headdim)


def shapes_of(shapes_list, i):
    """The Shapes record for case i's [batch, seqlen_q, seqlen_k, heads_q, heads_k, headdim]."""
    s = row(shapes_list, i)
    return Core.Shapes(batch=at(s, 0, 0), seqlen_q=at(s, 1, 0), seqlen_k=at(s, 2, 0), heads_q=at(s, 3, 0), heads_k=at(s, 4, 0), headdim=at(s, 5, 0))


def opts_of(scales_list, causal_list, windows_list, alibi_list, ret_list, i):
    """Case i's shared options, unpacked from the parallel constant lists."""
    w = row(windows_list, i)
    return Opts(scale=at(scales_list, i, 0.0), causal=at(causal_list, i, False), window_left=at(w, 0, -1), window_right=at(w, 1, -1), alibi=row(alibi_list, i), ret=at(ret_list, i, False))


def case_of(shapes_list, scales_list, causal_list, windows_list, alibi_list, ret_list, q_list, k_list, v_list, i):
    """Unpack case i from the parallel constant lists into a Case."""
    return Case(
        shapes=shapes_of(shapes_list, i),
        q=row(q_list, i),
        k=row(k_list, i),
        v=row(v_list, i),
        opts=opts_of(scales_list, causal_list, windows_list, alibi_list, ret_list, i),
    )


def case_checks(c, want_out, want_lse, want_probs, name):
    """The assertions for one config: flash and naive against the oracle, and each other."""
    o = c.opts
    flash = Attention.flash_attn_func(c.q, c.k, c.v, c.shapes, o.scale, o.causal, o.window_left, o.window_right, o.alibi, o.ret)
    naive = Naive.naive_attention(c.q, c.k, c.v, c.shapes, o.scale, o.causal, o.window_left, o.window_right, o.alibi)
    core = [
        Check.close_all(name + ".flash.out", flash.out, want_out, RTOL, ATOL),
        Check.close_all(name + ".flash.lse", flash.lse, want_lse, RTOL, ATOL),
        Check.close_all(name + ".naive.out", naive.out, want_out, RTOL, ATOL),
        Check.close_all(name + ".naive.lse", naive.lse, want_lse, RTOL, ATOL),
        Check.close_all(name + ".naive.probs", naive.probs, want_probs, RTOL, ATOL),
        Check.close_all(name + ".out.flash_naive", flash.out, naive.out, RTOL, ATOL),
        Check.close_all(name + ".lse.flash_naive", flash.lse, naive.lse, RTOL, ATOL),
    ]
    if o.ret:
        return listConcat(core, [Check.close_all(name + ".flash.probs", flash.probs, want_probs, RTOL, ATOL)])
    else:
        return listAppend(core, Check.equal(name + ".flash.probs.empty", len(flash.probs), 0))


def finite_checks(i):
    """Case i from the finite-lse constants."""
    c = case_of(Expected.CASE_SHAPES, Expected.CASE_SCALES, Expected.CASE_CAUSAL, Expected.CASE_WINDOWS, Expected.CASE_ALIBI, Expected.CASE_RETURN, Expected.CASE_Q, Expected.CASE_K, Expected.CASE_V, i)
    return case_checks(c, row(Expected.CASE_OUT, i), row(Expected.CASE_LSE, i), row(Expected.CASE_PROBS, i), f"case{i}")


def decoded_lse(raw):
    """A non-finite lse constant's repr strings decoded to floats."""
    return toList(map(range(len(raw)), lambda j: Check.float_of(at(raw, j, ""))))


def nonfinite_checks(i):
    """Case i from the non-finite constants (lse holds -inf, written as strings)."""
    c = case_of(Expected.NF_SHAPES, Expected.NF_SCALES, Expected.NF_CAUSAL, Expected.NF_WINDOWS, Expected.NF_ALIBI, Expected.NF_RETURN, Expected.NF_Q, Expected.NF_K, Expected.NF_V, i)
    return case_checks(c, row(Expected.NF_OUT, i), decoded_lse(row(Expected.NF_LSE, i)), row(Expected.NF_PROBS, i), f"nf{i}")


def flatten(xss):
    """One list of outcomes from a list of per-case outcome lists."""
    return foldList(xss, [], lambda acc, xs: listConcat(acc, xs))


def oracle_cases():
    """Every oracle config through flash and naive."""
    finite = flatten(toList(map(range(Expected.CASE_COUNT), finite_checks)))
    nonfinite = flatten(toList(map(range(Expected.NF_COUNT), nonfinite_checks)))
    return listConcat(finite, nonfinite)


def max_diff(xs):
    """The largest gap between consecutive cumulative lengths, for the varlen API."""
    return fold(range(len(xs) - 1), 0, lambda m, b: max_gap(at(xs, b, 0), at(xs, b + 1, 0), m))


def max_gap(a, b, m):
    """The running maximum of gap (b - a) and m."""
    gap = b - a
    return gap if gap > m else m


def varlen_checks(i):
    """Varlen case i: flash_attn_varlen_func against the concatenated oracle values."""
    cu_q = row(Expected.VARLEN_CUQ, i)
    cu_k = row(Expected.VARLEN_CUK, i)
    o = opts_of(Expected.VARLEN_SCALES, Expected.VARLEN_CAUSAL, Expected.VARLEN_WINDOWS, Expected.VARLEN_ALIBI, Expected.VARLEN_RETURN, i)
    r = Attention.flash_attn_varlen_func(row(Expected.VARLEN_Q, i), row(Expected.VARLEN_K, i), row(Expected.VARLEN_V, i), cu_q, cu_k, max_diff(cu_q), max_diff(cu_k), at(Expected.VARLEN_HQ, i, 0), at(Expected.VARLEN_HK, i, 0), at(Expected.VARLEN_HD, i, 0), o.scale, o.causal, o.window_left, o.window_right, o.alibi, o.ret)
    checks = [
        Check.close_all(f"varlen{i}.out", r.out, row(Expected.VARLEN_OUT, i), RTOL, ATOL),
        Check.close_all(f"varlen{i}.lse", r.lse, row(Expected.VARLEN_LSE, i), RTOL, ATOL),
    ]
    if o.ret:
        return listAppend(checks, Check.close_all(f"varlen{i}.probs", r.probs, row(Expected.VARLEN_PROBS, i), RTOL, ATOL))
    else:
        return listAppend(checks, Check.equal(f"varlen{i}.probs.empty", len(r.probs), 0))


def varlen_cases():
    """All the varlen configs."""
    return flatten(toList(map(range(Expected.VARLEN_COUNT), varlen_checks)))


def geometry_checks():
    """One assertion per record and scalar Core export."""
    s = shapes(2, 3, 4, 2, 2)
    return [
        Check.equal("core.Shapes", s.headdim, 2),
        Check.close_all("core.FlashResult", Core.FlashResult(out=[1.0], lse=[0.0], probs=[]).out, [1.0], RTOL, ATOL),
        Check.equal("core.Bounds", Core.Bounds(lo=1, hi=4).hi, 4),
        Check.equal("core.Row", Core.Row(m=1.0, l=2.0, acc=[3.0]).l, 2.0),
        Check.equal("core.Dim4", Core.Dim4(a=1, b=2, c=3, d=4).c, 3),
        Check.close("core.max_f", Core.max_f(1.5, 2.5), 2.5, RTOL, ATOL),
        Check.equal("core.min_i", Core.min_i(3, 5), 3),
    ]


def index_checks():
    """One assertion per index, GQA and slicing Core export."""
    s = shapes(2, 3, 4, 2, 2)
    hd2 = shapes(1, 1, 1, 1, 2)
    gqa = shapes(1, 1, 2, 1, 2)
    return [
        Check.equal("core.kv_head.0", Core.kv_head(s, 0), 0),
        Check.equal("core.kv_head.2", Core.kv_head(s, 2), 1),
        Check.equal("core.q_index", Core.q_index(s, 0, 1, 2, 1), 13),
        Check.equal("core.kv_index", Core.kv_index(s, 0, 2, 1, 1), 11),
        Check.equal("core.dim4", Core.dim4(2, 3, 4, 5, 0).a, 0),
        Check.close("core.dot", Core.dot([1.0, 2.0], [5.0, 6.0], hd2, 0, 0, 0, 0), 17.0, RTOL, ATOL),
        Check.close("core.v_at", Core.v_at([0.0, 1.0, 2.0, 3.0], gqa, 0, 0, 0, 1), 1.0, RTOL, ATOL),
        Check.close_all("core.slice_flat", Core.slice_flat([1.0, 2.0, 3.0, 4.0], 1, 2), [2.0, 3.0], RTOL, ATOL),
    ]


def masking_checks():
    """One assertion per mask and ALiBi Core export, including the empty row."""
    sq_sk = shapes(3, 3, 1, 1, 1)
    long_q = shapes(4, 2, 1, 1, 1)
    return [
        Check.equal("core.mask_bounds.causal", Core.mask_bounds(sq_sk, True, -1, -1, 1).hi, 2),
        Check.equal("core.mask_bounds.window.lo", Core.mask_bounds(sq_sk, False, 1, -1, 2).lo, 1),
        Check.equal("core.mask_bounds.empty", Core.mask_bounds(long_q, True, -1, -1, 0).hi, 0),
        Check.close("core.alibi_bias", Core.alibi_bias([0.5], sq_sk, 0, 0, 2), -1.0, RTOL, ATOL),
        Check.equal("core.alibi_bias.none", Core.alibi_bias([], sq_sk, 0, 0, 2), 0.0),
    ]


def all_checks():
    """Every assertion in the suite, in a stable order."""
    return listConcat(oracle_cases(), listConcat(varlen_cases(), listConcat(geometry_checks(), listConcat(index_checks(), masking_checks()))))


def main():
    return Check.run(all_checks)
