# osprey: flavor=python
# osprey: namespace=qwen_server
# osprey: module=Numeric
"""The server's arithmetic policy: a fault ends the served region, never substitutes a value.

A served request computes in a region with no `Arith` policy of its own
([ARITH-EFFECT-DISCHARGE]), so the server installs one here. This is the strict
shape: an integer overflow or a division by zero ends the region with the
message that named it, because a serving process that answered with a wrapped
count or an invented zero would be telling a caller something false. That
differs from `@osprey/testing`'s `Check.strictly`, which fails a suite
assertion; a server is not a suite and must not depend on the test package.
"""

__all__ = ["Fault", "halting", "arithmetic"]


@effect
class Fault:
    """An arithmetic fault ends the served region with the message that named it."""
    @control
    def halt(message: str) -> int: ...


@handler(Fault)
class halting:
    """The fault policy: report the fault on the server's log and end the region."""

    def halt(message):
        print(f"arithmetic fault: {message}")
        return 1


@handler(Arith)
class arithmetic:
    """Every arithmetic fault forwards to `Fault`: no arm invents a number."""

    def overflow(op, left, right, _wrapped):
        return perform(Fault.halt(f"integer overflow: {left} {op} {right}"))

    def divideByZero(op, left):
        return toFloat(perform(Fault.halt(f"division by zero: {left} {op} 0")))

    def remainderByZero(left):
        return perform(Fault.halt(f"remainder by zero: {left} % 0"))
