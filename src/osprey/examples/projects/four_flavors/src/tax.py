# osprey: flavor=python
# osprey: namespace=shop
# osprey: module=Tax
"""Sales tax."""


@export
def with_tax(amount):
    """`amount` with ten percent tax added."""
    return satAdd(amount, amount // 10)
