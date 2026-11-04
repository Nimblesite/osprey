"""Strict JSON is shared by scalar and object-call requests."""

import json
import math


def _reject_constant(value):
    raise ValueError(f"Non-finite JSON number: {value}")


def finite_float(value):
    result = float(value)
    if not math.isfinite(result):
        # Overflow must fail here rather than silently passing infinity to Python.
        raise ValueError("Non-finite JSON number")
    return result


def decode(value):
    return json.loads(value, parse_constant=_reject_constant, parse_float=finite_float)
