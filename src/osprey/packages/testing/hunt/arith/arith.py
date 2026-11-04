# osprey: flavor=python
"""Hunt driver for `Check.strictly`: the strict `Arith` policy must turn every
fault class into an `Error`, never a crash. Two symbolic integers drive `//`
(remainder-by-zero), `%` (remainder-by-zero), `/` (float divide-by-zero) and
`+` (integer overflow), each in its own handled region so every class is
explored independently."""
from testing import Check


def run(name, work):
    """Run one handled region and print ok or the policy's error message."""
    match Check.strictly(work):
        case Success(value=value):
            return print(f"{name}: ok")
        case Error(message=message):
            return print(f"{name}: {message}")


def main():
    a = unwrap_or(parseInt(input()), 0)
    b = unwrap_or(parseInt(input()), 0)
    run("div", lambda: print(a // b))
    run("rem", lambda: print(a % b))
    run("fdiv", lambda: print(a / b))
    run("ovf", lambda: print(a + b))
