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

__all__ = ["Outcome", "passed", "failed", "equal", "close", "close_all", "ints_equal", "truth", "near", "report", "summary", "ints_text", "float_of", "Fault", "strictly", "run", "run_or_print", "lenient", "specials"]


class Outcome:
    """One assertion: its name, whether it held, and what was seen when not."""
    name: str
    ok: bool
    detail: str


def passed(name):
    """An assertion that held.

    # Examples
    ```osprey
    from testing import Check

    held = Check.passed("close.pair")
    missed = Check.passed("near.special")
    print(f"[{held.name}] [{held.ok}] [{held.detail}] [{missed.ok}]")
    ```
    ```output
    [close.pair] [true] [] [true]
    ```
    """
    return Outcome(name=name, ok=True, detail="")


def failed(name, detail):
    """An assertion that did not hold, with what was seen.

    # Examples
    ```osprey
    from testing import Check

    missed = Check.failed("lenient.division", "got 0.0, want inf")
    print(f"[{missed.name}] [{missed.ok}] [{missed.detail}]")
    ```
    ```output
    [lenient.division] [false] [got 0.0, want inf]
    ```
    """
    return Outcome(name=name, ok=False, detail=detail)


def truth(name, condition, detail):
    """Passes when `condition` holds; `detail` explains a failure.

    # Examples
    ```osprey
    from testing import Check

    held = Check.truth("positive", 3 > 0, "got a negative")
    missed = Check.truth("positive", -1 > 0, "got -1, want 0 or more")
    print(f"[{held.ok}] [{missed.ok}] [{missed.detail}]")
    ```
    ```output
    [true] [false] [got -1, want 0 or more]
    ```
    """
    return passed(name) if condition else failed(name, detail)


def equal(name, got, want):
    """Structural equality of any two values of one printable type.

    # Examples
    ```osprey
    from testing import Check

    same = Check.equal("double", 2 + 2, 4)
    differs = Check.equal("greeting", "hi", "hello")
    print(f"[{same.ok}] [{differs.detail}]")
    ```
    ```output
    [true] [got hi, want hello]
    ```
    """
    return truth(name, structuralEquals(got, want), f"got {got}, want {want}")


def is_nan(x):
    return x != x


def is_infinite(x):
    return abs(x) == exp(1000.0)


def near(got: float, want: float, rtol: float, atol: float) -> bool:
    """numpy.isclose(got, want, rtol, atol, equal_nan=True) for one pair.

    # Examples
    ```osprey
    from testing import Check

    nan = exp(1000.0) - exp(1000.0)
    close_pair = Check.near(1.0, 1.0000001, 0.00001, 0.00000001)
    far_pair = Check.near(1.0, 2.0, 0.00001, 0.00000001)
    print(f"[{close_pair}] [{far_pair}] [{Check.near(nan, nan, 0.0, 0.0)}] [{Check.near(exp(1000.0), 0.0 - exp(1000.0), 0.0, 0.0)}]")
    ```
    ```output
    [true] [false] [true] [false]
    ```
    """
    if is_nan(got) or is_nan(want):
        return is_nan(got) and is_nan(want)
    elif got == want:
        return True
    elif is_infinite(got) or is_infinite(want):
        return False
    else:
        return abs(got - want) <= atol + rtol * abs(want)


def close(name, got, want, rtol, atol):
    """numpy.isclose for one float pair.

    # Examples
    ```osprey
    from testing import Check

    held = Check.close("close.pair", 1.0, 1.0000001, 0.00001, 0.00000001)
    missed = Check.close("close.far", 1.0, 2.0, 0.00001, 0.00000001)
    print(f"[{held.ok}] [{missed.detail}]")
    ```
    ```output
    [true] [got 1.0, want 2.0]
    ```
    """
    return truth(name, near(got, want, rtol, atol), f"got {got}, want {want}")


def first_far(got, want, rtol, atol):
    """The first index whose pair is not near, or the length when all are."""
    return fold(range(len(want)), len(want), lambda found, i: found if found < len(want) or near(unwrap_or(listGet(got, i), 0.0), unwrap_or(listGet(want, i), 0.0), rtol, atol) else i)


def close_all(name, got, want, rtol, atol):
    """numpy.allclose over two flat float lists of the same length.

    # Examples
    ```osprey
    from testing import Check

    held = Check.close_all("close_all.lists", [1.0, 2.0, 3.0], [1.0, 2.0, 3.0000001], 0.00001, 0.00000001)
    lengths = Check.close_all("close_all.lengths", [1.0], [1.0, 2.0], 0.00001, 0.00000001)
    print(f"[{held.ok}] [{lengths.detail}]")
    ```
    ```output
    [true] [length 1, want 2]
    ```
    """
    if len(got) != len(want):
        return failed(name, f"length {len(got)}, want {len(want)}")
    else:
        i = first_far(got, want, rtol, atol)
        return truth(name, i == len(want), f"index {i}: got {unwrap_or(listGet(got, i), 0.0)}, want {unwrap_or(listGet(want, i), 0.0)}")


def ints_text(values):
    """`[1, 2, 3]`: an int list as Python prints it.

    # Examples
    ```osprey
    from testing import Check

    print(f"{Check.ints_text([1, 2, 3])} {Check.ints_text([])}")
    ```
    ```output
    [1, 2, 3] []
    ```
    """
    return "[" + join(toList(map(range(len(values)), lambda i: toString(unwrap_or(listGet(values, i), 0)))), ", ") + "]"


def ints_equal(name, got, want):
    """Exact equality of two int lists, naming the first difference.

    # Examples
    ```osprey
    from testing import Check

    same = Check.ints_equal("ints_equal", [1, 2, 3], [1, 2, 3])
    differs = Check.ints_equal("ints_equal.differs", [1, 2, 4], [1, 2, 3])
    lengths = Check.ints_equal("ints_equal.lengths", [1, 2], [1, 2, 3])
    print(f"[{same.ok}] [{differs.detail}] [{lengths.detail}]")
    ```
    ```output
    [true] [index 2: got [1, 2, 4], want [1, 2, 3]] [length 2, want 3: got [1, 2]]
    ```
    """
    i = fold(range(len(want)), len(want), lambda found, k: found if found < len(want) or unwrap_or(listGet(got, k), 0) == unwrap_or(listGet(want, k), 0) else k)
    if len(got) != len(want):
        return failed(name, f"length {len(got)}, want {len(want)}: got {ints_text(got)}")
    else:
        return truth(name, i == len(want), f"index {i}: got {ints_text(got)}, want {ints_text(want)}")


def float_of(text):
    """A float an oracle wrote as text: Python's repr of a number, or "inf", "-inf",
    "nan" for the values the flavor has no literal for
    (src/osprey/packages/testing/oracle/expected_module.cpython). Unparseable text is NaN, so it never
    compares equal to a real expectation by accident.

    # Examples
    ```osprey
    from testing import Check

    number = Check.float_of("1.5")
    positive = Check.float_of("inf")
    negative = Check.float_of("-inf")
    unparseable = Check.float_of("junk")
    print(f"[{number}] [{positive}] [{negative}] [{unparseable} != {unparseable}]")
    ```
    ```output
    [1.5] [inf] [-inf] [nan != nan]
    ```
    """
    match text:
        case "inf":
            return exp(1000.0)
        case "-inf":
            return 0.0 - exp(1000.0)
        case _:
            return unwrap_or(parseFloat(text), abs(exp(1000.0) - exp(1000.0)))


def line(outcome):
    return f"ok {outcome.name}" if outcome.ok else f"FAIL {outcome.name}: {outcome.detail}"


def summary(outcomes):
    """`N passed, M failed` over a suite's outcomes.

    # Examples
    ```osprey
    from testing import Check

    suite = [Check.passed("a"), Check.failed("b", "got 1, want 2"), Check.passed("c")]
    print(f"[{Check.lenient(lambda: Check.summary(suite))}]")
    ```
    ```output
    [2 passed, 1 failed]
    ```
    """
    good = fold(range(len(outcomes)), 0, lambda n, i: n + 1 if unwrap_or(listGet(outcomes, i), failed("", "")).ok else n)
    return f"{good} passed, {len(outcomes) - good} failed"


def report(outcomes):
    """Print every outcome, then the totals line a golden pins.

    # Examples
    ```osprey
    from testing import Check

    Check.lenient(lambda: Check.report([Check.passed("equal.zero"), Check.failed("equal.one", "got 1, want 0")]))
    ```
    ```output
    ok equal.zero
    FAIL equal.one: got 1, want 0
    1 passed, 1 failed
    ```
    """
    forEachList(outcomes, lambda outcome: print(line(outcome)))
    return print(summary(outcomes))


@effect
class Fault:
    """An arithmetic fault ends the suite: its arm answers the whole region."""
    @control
    def halt(message: str) -> int:
        """The fault a suite's policy answers: under `strictly` it stops the
        region, `lenient` never performs it.

        # Examples
        ```osprey native
        from testing import Check

        def halt_message():
            match Check.strictly(lambda: print(toString(perform(Check.Fault.halt("integer overflow: 9223372036854775807 + 1"))))):
                case Error(message=message):
                    return message
                case Success():
                    return "no fault"

        print(f"[{halt_message()}]")
        ```
        ```output
        [integer overflow: 9223372036854775807 + 1]
        ```
        """
        ...


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
    bug, so it stops the suite rather than substitute a value.

    # Examples
    ```osprey native
    from testing import Check

    def plus(a, b):
        return a + b

    def verdict(work):
        match Check.strictly(work):
            case Error(message=message):
                return message
            case Success():
                return "the suite finished"

    print(f"[{verdict(lambda: print(toString(plus(9223372036854775807, 1))))}]")
    ```
    ```output
    [integer overflow: 9223372036854775807 + 1]
    ```
    """
    with halting:
        with arithmetic:
            return Success(value=work())


def run(cases):
    """A suite's entry point: reports `cases()` under `strictly`, or prints why
    the suite stopped.

    # Examples
    ```osprey native
    from testing import Check

    def plus(a, b):
        return a + b

    Check.run(lambda: [Check.equal("equal.overflow", plus(9223372036854775807, 1), 0)])
    ```
    ```output
    suite stopped: integer overflow: 9223372036854775807 + 1
    ```
    """
    return run_or_print(lambda: report(cases()), "suite")


def run_or_print(work, label):
    """`work` under `strictly`; when the policy abandons it, prints
    `<label> stopped: <message>` instead of returning.

    # Examples
    ```osprey native
    from testing import Check

    def plus(a, b):
        return a + b

    Check.run_or_print(lambda: print(f"sum {plus(2, 3)}"), "probe")
    Check.run_or_print(lambda: print(f"sum {plus(9223372036854775807, 1)}"), "probe")
    ```
    ```output
    sum 5
    probe stopped: integer overflow: 9223372036854775807 + 1
    ```
    """
    match strictly(work):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"{label} stopped: {message}")


def specials():
    """The non-finite and extreme binary64 values a hunt driver mixes into its
    symbolic inputs ([PORT-HUNT]): +inf, -inf, NaN, exp(700) and the subnormal
    exp(-745), so every non-finite path is explored on every run.

    # Examples
    ```osprey
    from testing import Check

    values = Check.specials()
    print(f"[{unwrap_or(listGet(values, 0), 0.0)}] [{unwrap_or(listGet(values, 1), 0.0)}] [{unwrap_or(listGet(values, 2), 0.0)}] [{unwrap_or(listGet(values, 3), 0.0)}] [{unwrap_or(listGet(values, 4), 0.0)}]")
    ```
    ```output
    [inf] [-inf] [nan] [1.0142320547350045e+304] [4.940656458e-324]
    ```
    """
    return [exp(1000.0), 0.0 - exp(1000.0), exp(1000.0) - exp(1000.0), exp(700.0), exp(-745.0)]


@handler(Arith)
class lenient:
    """A hunt driver's arithmetic policy ([PORT-HUNT]): every fault answers with a
    value — the wrapped result for an overflow, zero for a division or remainder
    by zero — so symbolic execution explores past it instead of stopping.

    # Examples
    ```osprey
    from testing import Check

    def plus(a, b):
        return a + b

    def quotient(a, b):
        return a / b

    def remainder(a, b):
        return a % b

    print(f"[{Check.lenient(lambda: plus(9223372036854775807, 1))}] [{Check.lenient(lambda: quotient(3.0, 0.0))}] [{Check.lenient(lambda: remainder(7, 0))}]")
    ```
    ```output
    [-9223372036854775808] [0.0] [0]
    ```
    """
    def overflow(_op, _l, _r, wrapped):
        return wrapped

    def divideByZero(_op, _l):
        return 0.0

    def remainderByZero(_l):
        return 0
