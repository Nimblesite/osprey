# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=numpy
# osprey: module=Math
"""Pure float helpers the flavor does not expose as builtins yet.

`Array` reuses numpy's names `exp`, `sqrt` and `tanh`, which shadow the
flavor's builtins of the same name throughout that module; the wrappers here
capture the builtins under distinct names. `ceil_int`, `floor_int` and
`trunc_int` build float->int conversion out of `toString`/`parseInt` because
the flavor has no floatToInt/ceil/floor builtin yet (the compiler pi is adding
them; this port works around the gap). Implements [BUILTIN-MATH],
[BUILTIN-MATH-ROUNDING].
"""

__all__ = [
    "exp_of", "sqrt_of", "tanh_of", "ln_of", "pow10", "trunc_int", "ceil_int",
    "floor_int",
]


def exp_of(x):
    """The flavor's `exp`, renamed so `Array.exp` keeps numpy's name."""
    return exp(x)


def sqrt_of(x):
    """The flavor's `sqrt`, renamed so `Array.sqrt` keeps numpy's name."""
    return sqrt(x)


def tanh_of(x):
    """The flavor's `tanh`, renamed so `Array.tanh` keeps numpy's name."""
    return tanh(x)


def ln_of(x):
    """The flavor's `ln` (natural log), which numpy calls `log`."""
    return ln(x)


def pow10(n):
    """10 raised to a non-negative integer power, by repeated multiplication."""
    return fold(range(n), 1, lambda acc, _: acc * 10)


def trunc_int(x):
    """Float -> int truncation toward zero, parsed from `toString` because the
    flavor has no floatToInt builtin yet. Handles the repr's optional sign,
    decimal point and `e` exponent."""
    s = toString(x)
    neg = startsWith(s, "-")
    body = unwrap_or(substring(s, 1, length(s)), "0") if neg else s
    pieces = unwrap_or(split(body, "e"), [body, "0"])
    mantissa = unwrap_or(listGet(pieces, 0), "0")
    exp = unwrap_or(parseInt(unwrap_or(listGet(pieces, 1), "0")), 0)
    mparts = unwrap_or(split(mantissa, "."), [mantissa, ""])
    whole = unwrap_or(listGet(mparts, 0), "0")
    frac = unwrap_or(listGet(mparts, 1), "")
    base = unwrap_or(parseInt(whole + frac), 0)
    shift = exp - length(frac)
    mag = base * pow10(shift) if shift >= 0 else intDiv(base, pow10(0 - shift))
    return 0 - mag if neg else mag


def ceil_int(x):
    """Smallest integer >= x, via truncation (no ceil builtin yet)."""
    t = trunc_int(x)
    return t + (1 if x > toFloat(t) else 0)


def floor_int(x):
    """Largest integer <= x, via truncation (no floor builtin yet)."""
    t = trunc_int(x)
    return t - (1 if x < toFloat(t) else 0)
