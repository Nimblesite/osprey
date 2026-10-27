"""CPython oracle for @osprey/flash_attn: scaled-dot-product attention reference values.

The real flash-attn package needs CUDA, so this oracle uses torch.nn.functional
.scaled_dot_product_attention in float64 where it applies (square sequences, no window,
no ALiBi) and explicit math for everything else: the window/causal mask, ALiBi bias,
GQA head mapping and the log-sum-exp. Causal masking is bottom-right aligned exactly
like flash-attn 2.1+ when seqlen_q != seqlen_k. Writes packages/flash_attn/tests/
expected.py, which the suite compares the online-softmax tiled pass against
([PORT-TEST-ORACLE], spec 0043). Run with the oracle virtualenv:
`python scripts/oracles/flash_attn.py`.
"""
import math
import os
import sys

from expected_module import write

import numpy
import torch
import torch.nn.functional as F


def scaled(hd):
    """The default softmax scale, 1/sqrt(headdim)."""
    return 1.0 / math.sqrt(hd)


def attention(q, k, v, sq, sk, hq, hk, hd, scale, causal, wl, wr, alibi):
    """out, lse, probs tensors for [batch, seq, heads, dim] inputs: explicit float64
    math with GQA, window, causal and ALiBi. Fully-masked rows give out 0 and lse -inf."""
    B = q.shape[0]
    g_idx = torch.arange(hk).repeat_interleave(hq // hk)
    kk = k[:, :, g_idx, :]  # [B, sk, hq, hd]
    vv = v[:, :, g_idx, :]  # [B, sk, hq, hd]

    scores = torch.einsum("bqgd,bkgd->bgqk", q, kk) * scale  # [B, hq, sq, sk]
    if alibi:
        slope = torch.tensor(alibi, dtype=torch.float64).view(1, hq, 1, 1)
        ii = torch.arange(sq, dtype=torch.float64).view(1, 1, sq, 1)
        jj = torch.arange(sk, dtype=torch.float64).view(1, 1, 1, sk)
        scores = scores - slope * (ii + (sk - sq) - jj).abs()

    mask = torch.ones(sq, sk, dtype=torch.bool)
    for i in range(sq):
        lo = 0
        hi = sk
        if wl >= 0:
            lo = max(0, i - wl)
        if causal:
            hi = min(hi, i + (sk - sq) + 1)
        if wr >= 0:
            hi = min(hi, i + wr + 1)
        if lo >= hi:
            lo = hi
        mask[i, :lo] = False
        mask[i, hi:] = False

    scores = scores.masked_fill(~mask[None, None], float("-inf"))
    m = scores.max(dim=-1, keepdim=True).values
    p = torch.exp(scores - m)
    p = p.masked_fill(~mask[None, None], 0.0)
    l = p.sum(dim=-1, keepdim=True)
    safe_l = l.clone()
    safe_l[safe_l == 0.0] = 1.0
    probs = p / safe_l
    lse = (m + torch.log(safe_l)).squeeze(-1)  # [B, hq, sq]
    out = torch.einsum("bgqk,bkgd->bqgd", probs, vv)  # [B, sq, hq, hd]
    return out, lse, probs


def reference(B, sq, sk, hq, hk, hd, scale, causal, wl, wr, alibi, mag, seed):
    """out, lse, probs flat lists for one config, cross-checked against SDPA."""
    g = torch.Generator().manual_seed(seed)
    q = torch.randn(B, sq, hq, hd, generator=g, dtype=torch.float64) * mag
    k = torch.randn(B, sk, hk, hd, generator=g, dtype=torch.float64) * mag
    v = torch.randn(B, sk, hk, hd, generator=g, dtype=torch.float64) * mag
    out, lse, probs = attention(q, k, v, sq, sk, hq, hk, hd, scale, causal, wl, wr, alibi)

    # Cross-check against SDPA where it applies. SDPA takes [batch, heads, seq, dim],
    # flash-attn takes [batch, seq, heads, dim], so permute on the way in and back.
    if sq == sk and wl == -1 and wr == -1 and not alibi:
        sdpa = F.scaled_dot_product_attention(q.permute(0, 2, 1, 3), k.permute(0, 2, 1, 3), v.permute(0, 2, 1, 3), is_causal=causal, scale=scale, enable_gqa=True)
        torch.testing.assert_close(sdpa.permute(0, 2, 1, 3), out, rtol=1e-5, atol=1e-8)

    return q.flatten().tolist(), k.flatten().tolist(), v.flatten().tolist(), out.flatten().tolist(), lse.flatten().tolist(), probs.flatten().tolist()


def configs():
    """The case grid: (B, sq, sk, hq, hk, hd, scale, causal, wl, wr, alibi, ret, mag)."""
    cs = []

    def add(B, sq, sk, hq, hk, hd, scale, causal, wl=-1, wr=-1, alibi=None, ret=False, mag=1.0):
        cs.append((B, sq, sk, hq, hk, hd, scale, causal, wl, wr, alibi or [], ret, mag))

    # Square self-attention grid: seqlen, causal, head split, headdim.
    for sq in (1, 2, 3, 5, 8):
        for causal in (False, True):
            for hq, hk in ((1, 1), (2, 1), (4, 2), (2, 2)):
                for hd in (1, 4):
                    add(1, sq, sq, hq, hk, hd, scaled(hd), causal)
    # Batch sizes 2 and 3.
    for B, sq, hq, hk, hd in ((2, 3, 2, 1, 4), (2, 5, 2, 2, 2), (3, 4, 2, 1, 3), (2, 6, 4, 2, 1)):
        for causal in (False, True):
            add(B, sq, sq, hq, hk, hd, scaled(hd), causal)
    # Odd headdim, singleton GQA.
    add(1, 1, 1, 3, 1, 1, scaled(1), False)
    add(1, 2, 2, 1, 1, 3, scaled(3), True)
    # Decoding: seqlen_q < seqlen_k, causal, bottom-right aligned.
    for sq, sk, hq, hk, hd in ((1, 4, 1, 1, 4), (2, 6, 2, 1, 2), (3, 5, 1, 1, 4), (1, 8, 2, 2, 4), (2, 7, 4, 2, 2)):
        add(1, sq, sk, hq, hk, hd, scaled(hd), True)
    # Non-causal, non-square.
    add(1, 2, 5, 1, 1, 4, scaled(4), False)
    add(1, 5, 2, 1, 1, 4, scaled(4), False)
    # Causal with seqlen_q > seqlen_k: fully-masked early rows (non-finite lse).
    add(1, 3, 2, 1, 1, 4, scaled(4), True)
    add(1, 5, 3, 1, 1, 2, scaled(2), True)
    add(1, 4, 1, 2, 1, 2, scaled(2), True)
    # Local attention windows.
    for wl, wr, causal in ((2, 2, False), (1, -1, False), (-1, 1, False), (0, 0, False), (2, 1, True), (1, 1, True)):
        add(1, 6, 6, 1, 1, 4, scaled(4), causal, wl, wr)
    # ALiBi slopes, one per query head.
    add(1, 5, 5, 2, 2, 4, scaled(4), False, alibi=[0.5, 1.0])
    add(2, 4, 4, 2, 1, 2, scaled(2), False, alibi=[-0.3, 0.7])
    add(1, 6, 6, 1, 1, 4, scaled(4), True, alibi=[0.5])
    add(1, 5, 5, 3, 3, 2, scaled(2), False, alibi=[0.0, 0.25, -0.5])
    add(1, 2, 4, 1, 1, 4, scaled(4), True, alibi=[-0.2])
    # Multi-block: seqlen_k above the port's 32-key block.
    add(1, 40, 40, 1, 1, 2, scaled(2), False)
    add(1, 40, 40, 1, 1, 2, scaled(2), True)
    add(1, 1, 48, 1, 1, 2, scaled(2), True)
    add(1, 34, 34, 2, 1, 1, scaled(1), False)
    # return_attn_probs.
    add(1, 2, 2, 1, 1, 4, scaled(4), True, ret=True)
    add(2, 3, 3, 2, 1, 2, scaled(2), False, ret=True)
    add(1, 2, 4, 1, 1, 4, scaled(4), True, ret=True)
    add(1, 5, 5, 2, 2, 4, scaled(4), False, alibi=[0.5, 1.0], ret=True)
    add(1, 4, 4, 1, 1, 2, scaled(2), False, wl=1, wr=1, ret=True)
    # Extreme magnitudes and non-default scales.
    add(1, 4, 4, 1, 1, 4, scaled(4), True, mag=10.0)
    add(1, 4, 4, 1, 1, 4, scaled(4), False, mag=0.1)
    add(1, 5, 5, 2, 1, 4, scaled(4), False, alibi=[1.5, -0.5], mag=3.0)
    add(1, 2, 2, 1, 1, 1, 1.0, False)
    add(1, 2, 4, 1, 1, 1, 0.5, True)
    return cs


def varlen_reference(cu_q, cu_k, hq, hk, hd, scale, causal, wl, wr, alibi, ret, mag, seed):
    """flat q/k/v and concatenated out/lse/probs for one varlen config."""
    total_q, total_k = cu_q[-1], cu_k[-1]
    g = torch.Generator().manual_seed(seed)
    q = torch.randn(total_q * hq * hd, generator=g, dtype=torch.float64) * mag
    k = torch.randn(total_k * hk * hd, generator=g, dtype=torch.float64) * mag
    v = torch.randn(total_k * hk * hd, generator=g, dtype=torch.float64) * mag
    outs, lses, probses = [], [], []
    for b in range(len(cu_q) - 1):
        sq = cu_q[b + 1] - cu_q[b]
        sk = cu_k[b + 1] - cu_k[b]
        qb = q[cu_q[b] * hq * hd:cu_q[b + 1] * hq * hd].reshape(1, sq, hq, hd)
        kb = k[cu_k[b] * hk * hd:cu_k[b + 1] * hk * hd].reshape(1, sk, hk, hd)
        vb = v[cu_k[b] * hk * hd:cu_k[b + 1] * hk * hd].reshape(1, sk, hk, hd)
        out, lse, probs = attention(qb, kb, vb, sq, sk, hq, hk, hd, scale, causal, wl, wr, alibi)
        outs.append(out.flatten().tolist())
        lses.append(lse.flatten().tolist())
        probses.append(probs.flatten().tolist())
    return q.flatten().tolist(), k.flatten().tolist(), v.flatten().tolist(), sum(outs, []), sum(lses, []), sum(probses, [])


def varlen_configs():
    """(cu_q, cu_k, hq, hk, hd, scale, causal, wl, wr, alibi, ret, mag) lists."""
    return [
        ([0, 2, 5], [0, 4, 7], 1, 1, 4, scaled(4), True, -1, -1, [], False, 1.0),
        ([0, 1, 3, 6], [0, 2, 4, 7], 2, 1, 2, scaled(2), True, -1, -1, [], False, 1.0),
        ([0, 3, 3], [0, 3, 6], 1, 1, 2, scaled(2), False, -1, -1, [], False, 1.0),
        ([0, 4], [0, 6], 2, 2, 4, scaled(4), True, -1, -1, [], False, 1.0),
        ([0, 2, 2, 5], [0, 2, 5, 8], 1, 1, 4, scaled(4), True, -1, -1, [0.5], False, 1.0),
    ]


def split_by_finite(cases):
    """Finite-lse cases apart from the fully-masked (non-finite lse) ones."""
    finite, nonfinite = [], []
    for idx, (B, sq, sk, hq, hk, hd, scale, causal, wl, wr, alibi, ret, mag) in enumerate(cases):
        lse = reference(B, sq, sk, hq, hk, hd, scale, causal, wl, wr, alibi, mag, 1000 + idx)[4]
        (nonfinite if any(not math.isfinite(x) for x in lse) else finite).append((B, sq, sk, hq, hk, hd, scale, causal, wl, wr, alibi, ret, mag))
    return finite, nonfinite


def columns(names, rows):
    """Parallel constant lists: `rows` yields one tuple per case in `names` order."""
    cols = {name: [] for name in names}
    for row in rows:
        for name, value in zip(names, row):
            cols[name].append(value)
    return cols


def emit(cases):
    """Parallel constant lists for one group of cases."""
    names = ("SHAPES", "SCALES", "CAUSAL", "WINDOWS", "ALIBI", "RETURN", "Q", "K", "V", "OUT", "LSE", "PROBS")
    return columns(names, (
        ([B, sq, sk, hq, hk, hd], scale, causal, [wl, wr], alibi, ret, *reference(B, sq, sk, hq, hk, hd, scale, causal, wl, wr, alibi, mag, 1000 + idx))
        for idx, (B, sq, sk, hq, hk, hd, scale, causal, wl, wr, alibi, ret, mag) in enumerate(cases)))


def emit_varlen(cfgs):
    """Parallel constant lists for the varlen cases."""
    names = ("CUQ", "CUK", "HQ", "HK", "HD", "SCALES", "CAUSAL", "WINDOWS", "ALIBI", "RETURN", "Q", "K", "V", "OUT", "LSE", "PROBS")
    return columns(names, (
        (cu_q, cu_k, hq, hk, hd, scale, causal, [wl, wr], alibi, ret, *varlen_reference(cu_q, cu_k, hq, hk, hd, scale, causal, wl, wr, alibi, ret, mag, 2000 + idx))
        for idx, (cu_q, cu_k, hq, hk, hd, scale, causal, wl, wr, alibi, ret, mag) in enumerate(cfgs)))


def main():
    finite, nonfinite = split_by_finite(configs())
    constants = {f"CASE_{name}": values for name, values in emit(finite).items()}
    constants.update({f"NF_{name}": values for name, values in emit(nonfinite).items()})
    constants["CASE_COUNT"] = len(finite)
    constants["NF_COUNT"] = len(nonfinite)
    constants.update({f"VARLEN_{name}": values for name, values in emit_varlen(varlen_configs()).items()})
    constants["VARLEN_COUNT"] = len(varlen_configs())
    write(
        "flash_attn",
        "torch.nn.functional.scaled_dot_product_attention (float64) plus explicit mask/ALiBi/log-sum-exp math: flash_attn_func reference values.",
        {"torch": torch.__version__, "numpy": numpy.__version__},
        constants,
    )


if __name__ == "__main__":
    main()
