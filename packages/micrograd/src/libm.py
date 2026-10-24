# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=micrograd
# osprey: module=Libm
"""The math builtins under names a micrograd module can call.

`Engine` exports operations named `exp` and `tanh`, after micrograd's own
`Value.exp` and `Value.tanh`; a module that defines those names shadows the
builtins for its whole body, so this module holds the arithmetic itself. It
defines neither name, so `exp` below is Osprey's libm-backed builtin
([BUILTIN-MATH]). `tanh_of` is micrograd's tanh, the lecture's
`(exp(2x) - 1) / (exp(2x) + 1)`, not the C `tanh` the builtin spells.
"""

__all__ = ["exp_of", "tanh_of"]


def exp_of(x: float) -> float:
    """e raised to x, the builtin `exp`."""
    return exp(x)


def tanh_of(x: float) -> float:
    """micrograd's tanh: `(exp(2x) - 1) / (exp(2x) + 1)`."""
    e = exp(2.0 * x)
    return (e - 1.0) / (e + 1.0)
