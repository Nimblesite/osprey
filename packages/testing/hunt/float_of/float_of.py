# osprey: flavor=python
"""Hunt driver: `Check.float_of` on one line of symbolic stdin, then a branch on
the decoded value so KLEE explores every parse outcome (a real number, "inf",
"-inf", and unparseable text decoding to NaN)."""
from testing import Check

RTOL = 0.00001
ATOL = 0.00000001


def main():
    text = input()
    value = Check.float_of(text)
    print("near-zero" if Check.near(value, 0.0, RTOL, ATOL) else "far")
