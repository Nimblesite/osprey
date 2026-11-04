"""Random Q4_K weights for one synthetic Qwen block: q, k, v, o (5120x5120), gate and up
(17408x5120) and down (5120x17408), 144 bytes per 256 weights, plus a 5120 float32 input.

Run with `uv run --with numpy python3 token_fixture.py`; writes block.bin (about 210 MB) and x.bin.
"""
import numpy as np

rng = np.random.default_rng(2)
shapes = [(5120, 5120)] * 4 + [(17408, 5120)] * 2 + [(5120, 17408)]
parts = []
for rows, inner in shapes:
    n = rows * inner // 256
    b = rng.integers(0, 256, size=(n, 144), dtype=np.uint8)
    d = (rng.random(n) * 0.002).astype(np.float16).view(np.uint16)
    m = (rng.random(n) * 0.0002).astype(np.float16).view(np.uint16)
    b[:, 0], b[:, 1], b[:, 2], b[:, 3] = d & 255, d >> 8, m & 255, m >> 8
    parts.append(b.reshape(-1))
np.concatenate(parts).tofile("block.bin")
rng.standard_normal(5120).astype(np.float32).tofile("x.bin")
print(sum(p.size for p in parts))
