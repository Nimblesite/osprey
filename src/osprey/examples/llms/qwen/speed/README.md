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
