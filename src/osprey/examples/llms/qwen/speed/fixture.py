"""Random Q4_K blocks for the speed probe: 17408 rows x 5120 columns, 144 bytes per 256 weights.

Run with `uv run --with numpy python3 fixture.py`; writes q4k.bin (50 MB) and x.bin (5120 float32).
"""
import numpy as np

rows, inner = 17408, 5120
blocks = inner // 256
rng = np.random.default_rng(1)
b = rng.integers(0, 256, size=(rows * blocks, 144), dtype=np.uint8)
d = (rng.random(rows * blocks) * 0.01).astype(np.float16).view(np.uint16)
m = (rng.random(rows * blocks) * 0.001).astype(np.float16).view(np.uint16)
b[:, 0], b[:, 1], b[:, 2], b[:, 3] = d & 255, d >> 8, m & 255, m >> 8
b.tofile("q4k.bin")
rng.standard_normal(inner).astype(np.float32).tofile("x.bin")
