# osprey: flavor=python
# osprey: namespace=testing
# osprey: module=Check
"""Assertions every ported package's suite reports through, and the arithmetic
policy suites run under.

A check is a value, not a side effect: each assertion returns an `Outcome`, a
suite returns the list of its outcomes, and `report` prints one line per
outcome and the totals. Float comparisons use numpy's `allclose` rule,
`abs(got - want) <= atol + rtol * abs(want)`, with equal infinities equal and
NaN equal only to NaN, so a reference value computed by CPython's numpy or
torch under a different libm still compares. Implements [PORT-TEST-ORACLE].
"""

__all__ = ["Outcome", "passed", "failed", "equal", "close", "close_all", "ints_equal", "truth", "near", "report", "summary", "ints_text", "float_of", "Fault", "strictly", "lenient", "specials"]


class Outcome:
    """One assertion: its name, whether it held, and what was seen when not."""
    name: str
    ok: bool
    detail: str


def passed(name):
    """An assertion that held."""
    return Outcome(name=name, ok=True, detail="")


def failed(name, detail):
    """An assertion that did not hold, with what was seen."""
    return Outcome(name=name, ok=False, detail=detail)


def truth(name, condition, detail):
    """Passes when `condition` holds; `detail` explains a failure."""
    return passed(name) if condition else failed(name, detail)


def equal(name, got, want):
    """Structural equality of any two values of one printable type."""
    return truth(name, structuralEquals(got, want), f"got {got}, want {want}")


def is_nan(x):
    return x != x


def is_infinite(x):
    return abs(x) == exp(1000.0)


def near(got: float, want: float, rtol: float, atol: float) -> bool:
    """numpy.isclose(got, want, rtol, atol, equal_nan=True) for one pair."""
    if is_nan(got) or is_nan(want):
        return is_nan(got) and is_nan(want)
    elif got == want:
        return True
    elif is_infinite(got) or is_infinite(want):
        return False
    else:
        return abs(got - want) <= atol + rtol * abs(want)


def close(name, got, want, rtol, atol):
    """numpy.isclose for one float pair."""
    return truth(name, near(got, want, rtol, atol), f"got {got}, want {want}")


def first_far(got, want, rtol, atol):
    """The first index whose pair is not near, or the length when all are."""
    return fold(range(len(want)), len(want), lambda found, i: found if found < len(want) or near(unwrap_or(listGet(got, i), 0.0), unwrap_or(listGet(want, i), 0.0), rtol, atol) else i)


def close_all(name, got, want, rtol, atol):
    """numpy.allclose over two flat float lists of the same length."""
    if len(got) != len(want):
        return failed(name, f"length {len(got)}, want {len(want)}")
    else:
        i = first_far(got, want, rtol, atol)
        return truth(name, i == len(want), f"index {i}: got {unwrap_or(listGet(got, i), 0.0)}, want {unwrap_or(listGet(want, i), 0.0)}")


def ints_text(values):
    """`[1, 2, 3]`: an int list as Python prints it."""
    return "[" + join(toList(map(range(len(values)), lambda i: toString(unwrap_or(listGet(values, i), 0)))), ", ") + "]"


def ints_equal(name, got, want):
    """Exact equality of two int lists, naming the first difference."""
    i = fold(range(len(want)), len(want), lambda found, k: found if found < len(want) or unwrap_or(listGet(got, k), 0) == unwrap_or(listGet(want, k), 0) else k)
    if len(got) != len(want):
        return failed(name, f"length {len(got)}, want {len(want)}: got {ints_text(got)}")
    else:
        return truth(name, i == len(want), f"index {i}: got {ints_text(got)}, want {ints_text(want)}")


def float_of(text):
    """A float an oracle wrote as text: Python's repr of a number, or "inf", "-inf",
    "nan" for the values the flavor has no literal for
    (scripts/oracles/expected_module.py). Unparseable text is NaN, so it never
    compares equal to a real expectation by accident."""
    match text:
        case "inf":
            return exp(1000.0)
        case "-inf":
            return 0.0 - exp(1000.0)
        case _:
            return unwrap_or(parseFloat(text), exp(1000.0) - exp(1000.0))


def line(outcome):
    return f"ok {outcome.name}" if outcome.ok else f"FAIL {outcome.name}: {outcome.detail}"


def summary(outcomes):
    """`N passed, M failed` over a suite's outcomes."""
    good = fold(range(len(outcomes)), 0, lambda n, i: n + 1 if unwrap_or(listGet(outcomes, i), failed("", "")).ok else n)
    return f"{good} passed, {len(outcomes) - good} failed"


def report(outcomes):
    """Print every outcome, then the totals line a golden pins."""
    forEachList(outcomes, lambda outcome: print(line(outcome)))
    return print(summary(outcomes))


@effect
class Fault:
    """An arithmetic fault ends the suite: its arm answers the whole region."""
    @control
    def halt(message: str) -> int: ...


@handler(Fault)
class halting:
    def halt(fault):
        return Error(message=fault)


@handler(Arith)
class arithmetic:
    def overflow(op, l, r, _):
        return perform(Fault.halt(f"integer overflow: {l} {op} {r}"))

    def divideByZero(op, l):
        return toFloat(perform(Fault.halt(f"division by zero: {l} {op} 0")))

    def remainderByZero(l):
        return perform(Fault.halt(f"remainder by zero: {l} % 0"))


def strictly(work) -> Result[None, str]:
    """Runs a suite under the strict arithmetic policy: Success when it finishes,
    Error naming the first overflow or zero division. A fault in a port is a
    bug, so it stops the suite rather than substitute a value."""
    with halting:
        with arithmetic:
            return Success(value=work())


def specials():
    """The non-finite and extreme binary64 values a hunt driver mixes into its
    symbolic inputs ([PORT-HUNT]): +inf, -inf, NaN, exp(700) and the subnormal
    exp(-745), so every non-finite path is explored on every run."""
    return [exp(1000.0), 0.0 - exp(1000.0), exp(1000.0) - exp(1000.0), exp(700.0), exp(-745.0)]


@handler(Arith)
class lenient:
    """A hunt driver's arithmetic policy ([PORT-HUNT]): every fault answers with a
    value — the wrapped result for an overflow, zero for a division or remainder
    by zero — so symbolic execution explores past it instead of stopping."""
    def overflow(_op, _l, _r, wrapped):
        return wrapped

    def divideByZero(_op, _l):
        return 0.0

    def remainderByZero(_l):
        return 0
