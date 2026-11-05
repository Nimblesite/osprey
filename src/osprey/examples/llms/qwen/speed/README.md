# Qwen speed probe — quantized matvec on the real shape

`q4k_matvec.osp` is a compiled Osprey kernel: one thread per output row decodes
Q4_K blocks (binary16 scale and minimum, 6-bit sub-scales, 4-bit weights) and
folds the row against a `float32` input. It runs 120 dependent-free products of
17408×5120 and reads one result at the end, so under Metal the dispatches batch
into one command buffer ([LLM-DISPATCH-BATCH]).

```
uv run --with numpy python3 fixture.py          # q4k.bin, x.bin
uv run --with numpy python3 reference.py        # rows 0, 1 and 17407 in numpy
OSPREY_GPU=metal osprey q4k_matvec.osp --run    # same three numbers, bit-exact
```

The host oracle takes about 17 s per product, so check it with the step count
lowered to 1 (`sed 's/range(0, 120)/range(0, 1)/'`) rather than all 120.

Receipts: `src/toolchain/benchmarks/gpu-metal-dispatch.csv`.

## Files

- `q4k_matvec.osp`: one thread per row, bytes read one at a time (about 2 ms per product).
- `q4k_words.osp`: the same product reading `uint32` words (no faster).
- `token.osp`, `token_split4.osp`, `token_fixture.py`: one synthetic decode step at the
  real shapes, 64 blocks, with and without four threads per row (0.72 s and 0.98 s per token).
- `quant_builtin.osp`, `token_builtin.osp`: the same product and token through the library kernel
  `gpuMatMulQuant` (0.42 ms per product, 0.21 s per token).
- `q4k_standalone.m`: the same product in plain Metal with 32-bit indices (0.37 ms), the
  target for the library kernel [LLM-MATMUL-QUANT]. Build with
  `clang -fobjc-arc -framework Metal -framework Foundation -O2 q4k_standalone.m -o q4k_standalone`.
- `real_token_gen.py`: writes a probe over the real GGUF's own tensors, in their real types and
  shapes, for chosen layers plus the head (`real_token_gen.py <gguf> <tokens> <layer>... > real_token.osp`).
  Sample a few layers and scale up: every layer pages in about 240 MB of the mapped file.
