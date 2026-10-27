"""CPython oracle for @osprey/testing: numpy.isclose verdicts.

Writes packages/testing/tests/expected.py, which the suite compares `Check.near`
against ([PORT-TEST-ORACLE], spec 0043). Run with the oracle virtualenv:
`python scripts/oracles/testing.py`.
"""
import numpy

from expected_module import write

RTOL, ATOL = 1e-05, 1e-08
PAIRS = [
    (1.0 + 1e-06, 1.0), (1.001, 1.0), (0.0, 1e-09), (0.0, 1e-07), (-1.0, -1.0000001),
    (1e300, 1.0000001e300), (1e300, 1.0001e300), (5e-324, 0.0), (-0.0, 0.0), (100.0, 100.001),
    (100.0, 100.002), (1e-08, 2e-08), (-3.5, 3.5), (2.0, 1.99998), (2.0, 1.99997),
]
INF = float("inf")
SPECIAL = [(INF, INF), (INF, -INF), (float("nan"), float("nan")), (float("nan"), 1.0), (-INF, -INF), (INF, 1e308)]


def verdicts(pairs):
    return [bool(numpy.isclose(a, b, rtol=RTOL, atol=ATOL, equal_nan=True)) for a, b in pairs]


def main():
    write("testing", f"numpy.isclose(a, b, rtol={RTOL}, atol={ATOL}, equal_nan=True) verdicts.", {"numpy": numpy.__version__}, {
        "PAIRS": PAIRS, "VERDICTS": verdicts(PAIRS), "SPECIAL": SPECIAL, "SPECIAL_VERDICTS": verdicts(SPECIAL),
    }, {
        "PAIRS": "Finite (got, want) pairs, as floats.",
        "VERDICTS": "numpy.isclose's verdict for each finite pair.",
        "SPECIAL": "Pairs holding infinities or NaN, each float as its repr text.",
        "SPECIAL_VERDICTS": "numpy.isclose's verdict for each special pair.",
    })


if __name__ == "__main__":
    main()
