# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=faiss
# osprey: module=Normalize
"""faiss's `normalize_L2`: L2-normalize every row of a matrix in place.

faiss divides each row by its L2 norm, computed in float32 (`1.0f / sqrtf`),
and leaves a zero row untouched. The port returns the normalized matrix rather
than mutating its argument, and computes the norm in binary64; a norm is a
square root and the result is not exactly dyadic, so the suite compares at
rtol 1e-6 (see README).
"""

from faiss.Metrics import l2sqr

__all__ = ["normalize_L2"]


def normalize_L2(x):
    """faiss's `normalize_L2(x)`: each row divided by its L2 norm, zero rows
    unchanged. Returns the new matrix (faiss mutates `x` in place)."""
    return mapList(x, lambda row: normalize_row(row))


def normalize_row(row):
    """One row of `normalize_L2`: divide by the L2 norm when it is nonzero."""
    norm = sqrt(l2sqr(row, toList(map(range(len(row)), lambda _i: 0.0))))
    if norm > 0.0:
        return mapList(row, lambda v: v / norm)
    else:
        return row
