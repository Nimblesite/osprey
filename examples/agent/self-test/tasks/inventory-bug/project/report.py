from pricing import discounted_subtotal


def receipt(items, discount=0, tax_rate=0):
    lines = [
        f"{item.quantity} x {item.product.name} @ {item.product.price:.2f}"
        for item in items
    ]
    subtotal = discounted_subtotal(items, discount)
    total = round(subtotal * (1 + tax_rate / 100), 2)
    return "\n".join(lines + [f"TOTAL {total:.2f}"])
