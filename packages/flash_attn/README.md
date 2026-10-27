# @osprey/flash-attn

A port of the [flash-attention](https://github.com/Dao-AILab/flash-attention) algorithm,
compared against flash-attn 2.1+ semantics. The real flash-attn package needs CUDA, so the
oracle is `torch.nn.functional.scaled_dot_product_attention` in float64 plus explicit math
for the window/causal mask, ALiBi bias, GQA head mapping and the log-sum-exp
(`scripts/oracles/flash_attn.py`); this is documented as a divergence from a CUDA oracle.

The port reimplements `flash_attn_func` and `flash_attn_varlen_func` as FlashAttention's
online-softmax tiled pass: the key range is tiled into 32-key blocks, and each output element
is one build kernel that runs the online softmax (running max `m`, running sum `l`, rescaled
accumulator). The `[seqlen_q, seqlen_k]` score matrix is never materialised on the flash path;
`return_attn_probs` gets the probabilities from the naive reference, which does materialise
them. `dropout_p` is fixed at 0 and arithmetic is binary64 throughout.

A naive reference attention (`Naive.naive_attention`) ships in the same package so the suite
proves the flash path against an independent implementation of the same conventions; the
original package ships no Python reference, so this is a port addition for testing.

## API map

| Ported function | Original | Notes |
| --- | --- | --- |
| `Attention.flash_attn_func` | `flash_attn.flash_attn_func` | q, k, v as flat `[batch, seqlen, heads, headdim]` lists with an explicit `Core.Shapes` record; `dropout_p=0` only, `deterministic` dropped (no bf16) |
| `Attention.flash_attn_varlen_func` | `flash_attn.flash_attn_varlen_func` | independent `flash_attn_func` per sequence, outputs concatenated |
| `Naive.naive_attention` | — | port-added reference (see above) |
| `Core.mask_bounds` | flash-attn's causal/window mask | bottom-right aligned causal (`j <= i + seqlen_k - seqlen_q`) |
| `Core.alibi_bias` | flash-attn ALiBi | `-slope * |i + seqlen_k - seqlen_q - j|`, one slope per query head |
| `Core.kv_head`, `Core.dot`, `Core.q_index`, `Core.kv_index`, `Core.v_at`, `Core.dim4`, `Core.slice_flat`, `Core.max_f`, `Core.min_i` | — | shared geometry/index helpers both modules import |

## Divergences

- **Flat lists, explicit shapes.** @osprey/numpy's `Storage.Buffer` effect has not landed, so
  tensors are flat float lists and every call supplies a `Core.Shapes` record
  (`batch, seqlen_q, seqlen_k, heads_q, heads_k, headdim`) instead of reading shapes off a
  tensor. The online-softmax body is written so it moves behind `Storage.Buffer` unchanged
  when numpy MILESTONE 1 lands.
- **Optional arguments are explicit.** No default/keyword-only parameters in the Python flavor,
  so `softmax_scale`, `causal`, `window_left`, `window_right`, `alibi_slopes` (empty list for
  none) and `return_attn_probs` are all positional; `window_size=(-1,-1)` splits into
  `window_left, window_right` with `-1` meaning unlimited; `dropout_p` and `deterministic`
  are dropped (0 and no bf16, respectively).
- **Returns a `FlashResult` record** `{out, lse, probs}` rather than `out` (or the
  `(out, q, k, v, out_padded, softmax_lse, S_dmask)` tuple of the original's
  `return_attn_probs` path): `out` is `[batch, seqlen_q, heads_q, headdim]`, `lse` the softmax
  log-sum-exp `[batch, heads_q, seqlen_q]`, and `probs` is `[]` unless `return_attn_probs`.
- **Fully-masked causal rows** (`seqlen_q > seqlen_k`) produce `out = 0.0` and `lse = -inf`,
  matching flash-attn's convention; the oracle's lse for those rows is stored as `"-inf"`
  strings and decoded by the suite with `testing.Check.float_of`.
- **Oracle is torch SDPA + explicit math**, not the CUDA flash-attn package (see above).
- **`Naive.naive_attention`** is a port addition; the original ships no reference.

## Running the suite

```sh
osprey packages/flash_attn/tests --run --quiet          # default memory backend
osprey packages/flash_attn/tests --run --quiet --memory=gc
OSPREY_ARC_DEBUG=1 osprey packages/flash_attn/tests --run --quiet --memory=arc
```
