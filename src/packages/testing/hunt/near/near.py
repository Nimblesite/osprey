# osprey: flavor=python
"""Hunt driver for `Check.near` (the numpy.isclose port): two symbolic texts
decoded by `Check.float_of` exercise the NaN, infinity, equality and tolerance
paths — equal infinities close, opposite infinities far, NaN close only to NaN."""
from testing import Check

RTOL = 0.00001
ATOL = 0.00000001


def main():
    a = Check.float_of(input())
    b = Check.float_of(input())
    print("close" if Check.near(a, b, RTOL, ATOL) else "far")
