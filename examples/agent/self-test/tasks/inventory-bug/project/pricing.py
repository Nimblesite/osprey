def discounted_subtotal(items, discount=0):
    subtotal = sum(item.product.price for item in items)
    return subtotal * (1 - discount / 100)


def total_with_tax(items, discount=0, tax_rate=0):
    discounted = discounted_subtotal(items, discount)
    return round(discounted * (1 - discount / 100) * (1 + tax_rate / 100), 2)
