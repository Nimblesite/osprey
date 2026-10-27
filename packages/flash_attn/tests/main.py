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


class Case:
    """One oracle config unpacked from the parallel constants."""
    shapes: Core.Shapes
    q: list[float]
    k: list[float]
    v: list[float]
    scale: float
    causal: bool
    window_left: int
    window_right: int
    alibi: list[float]
    ret: bool


class Varlen:
    """One varlen config unpacked from the parallel constants."""
    cu_q: list[int]
    cu_k: list[int]
    hq: int
    hk: int
    hd: int
    scale: float
    causal: bool
    window_left: int
    window_right: int
    alibi: list[float]
    ret: bool
    q: list[float]
    k: list[float]
    v: list[float]


def shapes_of(shapes_list, i):
    """The Shapes record for case i's [batch, seqlen_q, seqlen_k, heads_q, heads_k, headdim]."""
    s = unwrap_or(listGet(shapes_list, i), [0, 0, 0, 0, 0, 0])
    return Core.Shapes(
        batch=unwrap_or(listGet(s, 0), 0),
        seqlen_q=unwrap_or(listGet(s, 1), 0),
        seqlen_k=unwrap_or(listGet(s, 2), 0),
        heads_q=unwrap_or(listGet(s, 3), 0),
        heads_k=unwrap_or(listGet(s, 4), 0),
        headdim=unwrap_or(listGet(s, 5), 0),
    )


def case_of(shapes_list, scales_list, causal_list, windows_list, alibi_list, ret_list, q_list, k_list, v_list, i):
    """Unpack case i from the parallel constant lists into a Case."""
    w = unwrap_or(listGet(windows_list, i), [-1, -1])
    return Case(
        shapes=shapes_of(shapes_list, i),
        q=unwrap_or(listGet(q_list, i), []),
        k=unwrap_or(listGet(k_list, i), []),
        v=unwrap_or(listGet(v_list, i), []),
        scale=unwrap_or(listGet(scales_list, i), 0.0),
        causal=unwrap_or(listGet(causal_list, i), False),
        window_left=unwrap_or(listGet(w, 0), -1),
        window_right=unwrap_or(listGet(w, 1), -1),
        alibi=unwrap_or(listGet(alibi_list, i), []),
        ret=unwrap_or(listGet(ret_list, i), False),
    )


def case_checks(c, want_out, want_lse, want_probs, name):
    """The assertions for one config: flash and naive against the oracle, and each other."""
    flash = Attention.flash_attn_func(c.q, c.k, c.v, c.shapes, c.scale, c.causal, c.window_left, c.window_right, c.alibi, c.ret)
    naive = Naive.naive_attention(c.q, c.k, c.v, c.shapes, c.scale, c.causal, c.window_left, c.window_right, c.alibi)
    core = [
        Check.close_all(name + ".flash.out", flash.out, want_out, RTOL, ATOL),
        Check.close_all(name + ".flash.lse", flash.lse, want_lse, RTOL, ATOL),
        Check.close_all(name + ".naive.out", naive.out, want_out, RTOL, ATOL),
        Check.close_all(name + ".naive.lse", naive.lse, want_lse, RTOL, ATOL),
        Check.close_all(name + ".naive.probs", naive.probs, want_probs, RTOL, ATOL),
        Check.close_all(name + ".out.flash_naive", flash.out, naive.out, RTOL, ATOL),
        Check.close_all(name + ".lse.flash_naive", flash.lse, naive.lse, RTOL, ATOL),
    ]
    if c.ret:
        return listConcat(core, [Check.close_all(name + ".flash.probs", flash.probs, want_probs, RTOL, ATOL)])
    else:
        return listAppend(core, Check.equal(name + ".flash.probs.empty", len(flash.probs), 0))


def finite_checks(i):
    """Case i from the finite-lse constants."""
    c = case_of(Expected.CASE_SHAPES, Expected.CASE_SCALES, Expected.CASE_CAUSAL, Expected.CASE_WINDOWS, Expected.CASE_ALIBI, Expected.CASE_RETURN, Expected.CASE_Q, Expected.CASE_K, Expected.CASE_V, i)
    want_out = unwrap_or(listGet(Expected.CASE_OUT, i), [])
    want_lse = unwrap_or(listGet(Expected.CASE_LSE, i), [])
    want_probs = unwrap_or(listGet(Expected.CASE_PROBS, i), [])
    return case_checks(c, want_out, want_lse, want_probs, f"case{i}")


def decoded_lse(raw):
    """A non-finite lse constant's repr strings decoded to floats."""
    return toList(map(range(len(raw)), lambda j: Check.float_of(unwrap_or(listGet(raw, j), ""))))


def nonfinite_checks(i):
    """Case i from the non-finite constants (lse holds -inf, written as strings)."""
    c = case_of(Expected.NF_SHAPES, Expected.NF_SCALES, Expected.NF_CAUSAL, Expected.NF_WINDOWS, Expected.NF_ALIBI, Expected.NF_RETURN, Expected.NF_Q, Expected.NF_K, Expected.NF_V, i)
    want_out = unwrap_or(listGet(Expected.NF_OUT, i), [])
    want_lse = decoded_lse(unwrap_or(listGet(Expected.NF_LSE, i), []))
    want_probs = unwrap_or(listGet(Expected.NF_PROBS, i), [])
    return case_checks(c, want_out, want_lse, want_probs, f"nf{i}")


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
    return fold(range(len(xs) - 1), 0, lambda m, b: max_gap(unwrap_or(listGet(xs, b), 0), unwrap_or(listGet(xs, b + 1), 0), m))


def max_gap(a, b, m):
    """The running maximum of gap (b - a) and m."""
    gap = b - a
    return gap if gap > m else m


def varlen_case_of(i):
    """Unpack varlen case i into a Varlen."""
    w = unwrap_or(listGet(Expected.VARLEN_WINDOWS, i), [-1, -1])
    return Varlen(
        cu_q=unwrap_or(listGet(Expected.VARLEN_CUQ, i), []),
        cu_k=unwrap_or(listGet(Expected.VARLEN_CUK, i), []),
        hq=unwrap_or(listGet(Expected.VARLEN_HQ, i), 0),
        hk=unwrap_or(listGet(Expected.VARLEN_HK, i), 0),
        hd=unwrap_or(listGet(Expected.VARLEN_HD, i), 0),
        scale=unwrap_or(listGet(Expected.VARLEN_SCALES, i), 0.0),
        causal=unwrap_or(listGet(Expected.VARLEN_CAUSAL, i), False),
        window_left=unwrap_or(listGet(w, 0), -1),
        window_right=unwrap_or(listGet(w, 1), -1),
        alibi=unwrap_or(listGet(Expected.VARLEN_ALIBI, i), []),
        ret=unwrap_or(listGet(Expected.VARLEN_RETURN, i), False),
        q=unwrap_or(listGet(Expected.VARLEN_Q, i), []),
        k=unwrap_or(listGet(Expected.VARLEN_K, i), []),
        v=unwrap_or(listGet(Expected.VARLEN_V, i), []),
    )


def varlen_checks(i):
    """Varlen case i: flash_attn_varlen_func against the concatenated oracle values."""
    c = varlen_case_of(i)
    r = Attention.flash_attn_varlen_func(c.q, c.k, c.v, c.cu_q, c.cu_k, max_diff(c.cu_q), max_diff(c.cu_k), c.hq, c.hk, c.hd, c.scale, c.causal, c.window_left, c.window_right, c.alibi, c.ret)
    checks = [
        Check.close_all(f"varlen{i}.out", r.out, unwrap_or(listGet(Expected.VARLEN_OUT, i), []), RTOL, ATOL),
        Check.close_all(f"varlen{i}.lse", r.lse, unwrap_or(listGet(Expected.VARLEN_LSE, i), []), RTOL, ATOL),
    ]
    if c.ret:
        return listAppend(checks, Check.close_all(f"varlen{i}.probs", r.probs, unwrap_or(listGet(Expected.VARLEN_PROBS, i), []), RTOL, ATOL))
    else:
        return listAppend(checks, Check.equal(f"varlen{i}.probs.empty", len(r.probs), 0))


def varlen_cases():
    """All the varlen configs."""
    return flatten(toList(map(range(Expected.VARLEN_COUNT), varlen_checks)))


def geometry_checks():
    """One assertion per record and scalar Core export."""
    s = Core.Shapes(batch=1, seqlen_q=2, seqlen_k=3, heads_q=4, heads_k=2, headdim=2)
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
    s = Core.Shapes(batch=1, seqlen_q=2, seqlen_k=3, heads_q=4, heads_k=2, headdim=2)
    hd2 = Core.Shapes(batch=1, seqlen_q=1, seqlen_k=1, heads_q=1, heads_k=1, headdim=2)
    gqa = Core.Shapes(batch=1, seqlen_q=1, seqlen_k=1, heads_q=2, heads_k=1, headdim=2)
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
    sq_sk = Core.Shapes(batch=1, seqlen_q=3, seqlen_k=3, heads_q=1, heads_k=1, headdim=1)
    long_q = Core.Shapes(batch=1, seqlen_q=4, seqlen_k=2, heads_q=1, heads_k=1, headdim=1)
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
    match Check.strictly(lambda: Check.report(all_checks())):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
