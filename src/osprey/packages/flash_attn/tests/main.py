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


def case_of(shapes_list, scales_list, causal_list, windows_list, alibi_list, ret_list, q_list, k_list, v_list, i):
    """Unpack case i from the parallel constant lists into a Case."""
    s = row(shapes_list, i)
    return Case(
        shapes=Core.Shapes(batch=at(s, 0, 0), seqlen_q=at(s, 1, 0), seqlen_k=at(s, 2, 0), heads_q=at(s, 3, 0), heads_k=at(s, 4, 0), headdim=at(s, 5, 0)),
        q=row(q_list, i),
        k=row(k_list, i),
        v=row(v_list, i),
        opts=opts_of(scales_list, causal_list, windows_list, alibi_list, ret_list, i),
    )


def opts_of(scales_list, causal_list, windows_list, alibi_list, ret_list, i):
    """Case i's shared options, unpacked from the parallel constant lists."""
    w = row(windows_list, i)
    return Opts(scale=at(scales_list, i, 0.0), causal=at(causal_list, i, False), window_left=at(w, 0, -1), window_right=at(w, 1, -1), alibi=row(alibi_list, i), ret=at(ret_list, i, False))


def close_all_each(base, suffixes, actuals, wants):
    """One close_all assertion per parallel (suffix, actual, want) entry."""
    return toList(map(range(len(suffixes)), lambda k: Check.close_all(base + at(suffixes, k, ""), row(actuals, k), row(wants, k), RTOL, ATOL)))


def probs_checks(base, ret, probs, want_probs):
    """The probs assertions: the oracle's when `ret` asks for them, else an empty list."""
    if ret:
        return [Check.close_all(base + ".probs", probs, want_probs, RTOL, ATOL)]
    else:
        return [Check.equal(base + ".probs.empty", len(probs), 0)]


def case_checks(c, want_out, want_lse, want_probs, name):
    """The assertions for one config: flash and naive against the oracle, and each other."""
    o = c.opts
    flash = Attention.flash_attn_func(c.q, c.k, c.v, c.shapes, o.scale, o.causal, o.window_left, o.window_right, o.alibi, o.ret)
    naive = Naive.naive_attention(c.q, c.k, c.v, c.shapes, o.scale, o.causal, o.window_left, o.window_right, o.alibi)
    core = flatten([
        close_all_each(name + ".flash", [".out", ".lse"], [flash.out, flash.lse], [want_out, want_lse]),
        close_all_each(name + ".naive", [".out", ".lse", ".probs"], [naive.out, naive.lse, naive.probs], [want_out, want_lse, want_probs]),
        close_all_each(name, [".out.flash_naive", ".lse.flash_naive"], [flash.out, flash.lse], [naive.out, naive.lse]),
    ])
    return listConcat(core, probs_checks(name + ".flash", o.ret, flash.probs, want_probs))


def family_checks(shapes_l, scales_l, causal_l, windows_l, alibi_l, ret_l, q_l, k_l, v_l, out_l, lse_l, probs_l, prefix, i):
    """Case i of one family of parallel Expected constants, named prefix+i."""
    c = case_of(shapes_l, scales_l, causal_l, windows_l, alibi_l, ret_l, q_l, k_l, v_l, i)
    return case_checks(c, row(out_l, i), row(lse_l, i), row(probs_l, i), f"{prefix}{i}")


def decoded_lse(raws):
    """A non-finite lse constant's repr strings, every row decoded to floats."""
    return toList(map(range(len(raws)), lambda j: toList(map(range(len(row(raws, j))), lambda k: Check.float_of(at(row(raws, j), k, ""))))))


def flatten(xss):
    """One list of outcomes from a list of per-case outcome lists."""
    return foldList(xss, [], lambda acc, xs: listConcat(acc, xs))


def flatten_cases(count, work):
    """Every case 0..count-1 through work, as one flat list of outcomes."""
    return flatten(toList(map(range(count), work)))


def oracle_cases():
    """Every oracle config through flash and naive."""
    finite = flatten_cases(Expected.CASE_COUNT, lambda i: family_checks(Expected.CASE_SHAPES, Expected.CASE_SCALES, Expected.CASE_CAUSAL, Expected.CASE_WINDOWS, Expected.CASE_ALIBI, Expected.CASE_RETURN, Expected.CASE_Q, Expected.CASE_K, Expected.CASE_V, Expected.CASE_OUT, Expected.CASE_LSE, Expected.CASE_PROBS, "case", i))
    nonfinite = flatten_cases(Expected.NF_COUNT, lambda i: family_checks(Expected.NF_SHAPES, Expected.NF_SCALES, Expected.NF_CAUSAL, Expected.NF_WINDOWS, Expected.NF_ALIBI, Expected.NF_RETURN, Expected.NF_Q, Expected.NF_K, Expected.NF_V, Expected.NF_OUT, decoded_lse(Expected.NF_LSE), Expected.NF_PROBS, "nf", i))
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
    checks = close_all_each(f"varlen{i}", [".out", ".lse"], [r.out, r.lse], [row(Expected.VARLEN_OUT, i), row(Expected.VARLEN_LSE, i)])
    return listConcat(checks, probs_checks(f"varlen{i}", o.ret, r.probs, row(Expected.VARLEN_PROBS, i)))


def varlen_cases():
    """All the varlen configs."""
    return flatten_cases(Expected.VARLEN_COUNT, varlen_checks)


def core_checks():
    """One assertion per Core export: records and scalars, indices, masks and ALiBi."""
    s = shapes(2, 3, 4, 2, 2)
    hd2 = shapes(1, 1, 1, 1, 2)
    gqa = shapes(1, 1, 2, 1, 2)
    sq_sk = shapes(3, 3, 1, 1, 1)
    long_q = shapes(4, 2, 1, 1, 1)
    return [
        Check.equal("core.Shapes", s.headdim, 2),
        Check.close_all("core.FlashResult", Core.FlashResult(out=[1.0], lse=[0.0], probs=[]).out, [1.0], RTOL, ATOL),
        Check.equal("core.Bounds", Core.Bounds(lo=1, hi=4).hi, 4),
        Check.equal("core.Row", Core.Row(m=1.0, l=2.0, acc=[3.0]).l, 2.0),
        Check.equal("core.Dim4", Core.Dim4(a=1, b=2, c=3, d=4).c, 3),
        Check.close("core.max_f", Core.max_f(1.5, 2.5), 2.5, RTOL, ATOL),
        Check.equal("core.min_i", Core.min_i(3, 5), 3),
        Check.equal("core.kv_head.0", Core.kv_head(s, 0), 0),
        Check.equal("core.kv_head.2", Core.kv_head(s, 2), 1),
        Check.equal("core.q_index", Core.q_index(s, 0, 1, 2, 1), 13),
        Check.equal("core.kv_index", Core.kv_index(s, 0, 2, 1, 1), 11),
        Check.equal("core.dim4", Core.dim4(2, 3, 4, 5, 0).a, 0),
        Check.close("core.dot", Core.dot([1.0, 2.0], [5.0, 6.0], hd2, 0, 0, 0, 0), 17.0, RTOL, ATOL),
        Check.close("core.v_at", Core.v_at([0.0, 1.0, 2.0, 3.0], gqa, 0, 0, 0, 1), 1.0, RTOL, ATOL),
        Check.close_all("core.slice_flat", Core.slice_flat([1.0, 2.0, 3.0, 4.0], 1, 2), [2.0, 3.0], RTOL, ATOL),
        Check.equal("core.mask_bounds.causal", Core.mask_bounds(sq_sk, True, -1, -1, 1).hi, 2),
        Check.equal("core.mask_bounds.window.lo", Core.mask_bounds(sq_sk, False, 1, -1, 2).lo, 1),
        Check.equal("core.mask_bounds.empty", Core.mask_bounds(long_q, True, -1, -1, 0).hi, 0),
        Check.close("core.alibi_bias", Core.alibi_bias([0.5], sq_sk, 0, 0, 2), -1.0, RTOL, ATOL),
        Check.equal("core.alibi_bias.none", Core.alibi_bias([], sq_sk, 0, 0, 2), 0.0),
    ]


def all_checks():
    """Every assertion in the suite, in a stable order."""
    return flatten([oracle_cases(), varlen_cases(), core_checks()])


def main():
    return Check.run(all_checks)
