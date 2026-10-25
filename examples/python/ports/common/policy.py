# osprey: flavor=python
# osprey: namespace=port_common
# osprey: module=Policy
"""The arithmetic policy the example applications run under.

The ports perform plain integer and floating-point arithmetic, so the compiler
requires a handler for the Arith effect. Every application here uses small,
bounded indices and counts, so a fault would be a bug in the application; the
handler substitutes the wrapped or zero value and the application's assertions
then catch any wrong result instead of the program stopping mid-report.
"""

__all__ = ["arithmetic"]


@handler(Arith)
class arithmetic:
    """Value-mode arithmetic: wrap an overflow, answer zero for zero divisors."""

    def overflow(_op, _left, _right, wrapped):
        return wrapped

    def divideByZero(_op, _left):
        return 0.0

    def remainderByZero(_left):
        return 0
