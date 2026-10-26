# osprey: flavor=python
"""Fatal-channel fixture executed by the Rust package interoperability gate."""
from jinja2 import Value as V
from testing import Check


def main():
    match Check.strictly(lambda: print(V.repr_text(V.quarantine("Jinja quarantine must abort")))):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(message)
