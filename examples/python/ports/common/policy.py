# osprey: flavor=python
# osprey: namespace=port_common
# osprey: module=Policy
"""Stop an application on arithmetic faults and register a failing test."""
from testing import Check

__all__ = ["execute"]


def execute(name, body):
    """Use the package suites' strict arithmetic policy for every application."""
    match Check.strictly(body):
        case Success():
            return None
        case Error(message=message):
            print(name + " failed: " + message)
            return test(name, lambda: checkAll("arithmetic fault", [False]))
