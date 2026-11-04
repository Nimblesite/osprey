import unittest

from models import Product, StockItem
from report import receipt


class ReportTests(unittest.TestCase):
    def setUp(self):
        self.items = [
            StockItem(Product("CF", "Coffee", 3.50), 2),
            StockItem(Product("TE", "Tea", 2.00), 1),
        ]

    def test_receipt_total(self):
        self.assertIn("TOTAL 8.91", receipt(self.items, discount=10, tax_rate=10))

    def test_zero_discount(self):
        item = StockItem(Product("PN", "Pen", 1.25), 1)
        self.assertIn("TOTAL 1.25", receipt([item]))

    def test_receipt_lines(self):
        text = receipt(self.items)
        self.assertIn("2 x Coffee @ 3.50", text)
        self.assertIn("1 x Tea @ 2.00", text)


if __name__ == "__main__":
    unittest.main()
