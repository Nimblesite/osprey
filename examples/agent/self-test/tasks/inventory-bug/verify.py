import ast
import math
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ORIGINAL_TESTS = ("test_receipt_total", "test_zero_discount", "test_receipt_lines")


def fail(message):
    raise SystemExit(f"inventory-bug: {message}")


def test_ids(suite):
    for test in suite:
        if isinstance(test, unittest.TestSuite):
            yield from test_ids(test)
        else:
            yield test.id()


def existing_tests(project):
    tree = ast.parse((project / "test_inventory.py").read_text(encoding="utf-8"))
    return {
        node.name: ast.dump(node) for node in ast.walk(tree)
        if isinstance(node, ast.FunctionDef) and node.name in ORIGINAL_TESTS
    }


def main():
    project = Path(sys.argv[1]).resolve()
    result = subprocess.run(
        [sys.executable, "-m", "unittest"], cwd=project, text=True,
        capture_output=True,
    )
    if result.returncode:
        fail("visible unittest suite fails\n" + result.stdout + result.stderr)
    sys.path.insert(0, str(project))
    suite = unittest.defaultTestLoader.discover(str(project))
    required = {f"test_inventory.ReportTests.{name}" for name in ORIGINAL_TESTS}
    ids = set(test_ids(suite))
    missing = required - ids
    if missing:
        fail(f"required visible tests missing: {sorted(missing)}")
    new_tests = ids - required
    if not new_tests:
        fail("no regression test was added")
    new_names = {test_id.rsplit(".", 1)[1] for test_id in new_tests}
    new_methods = [
        node for path in project.glob("test*.py")
        for node in ast.walk(ast.parse(path.read_text(encoding="utf-8")))
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
        and node.name in new_names
    ]
    if not any(
        isinstance(call.func, ast.Name) and call.func.id == "total_with_tax"
        for method in new_methods
        for call in ast.walk(method)
        if isinstance(call, ast.Call)
    ):
        fail("a new regression test must call total_with_tax")
    original = Path(__file__).resolve().parent / "project"
    if existing_tests(project) != existing_tests(original):
        fail("the existing tests were changed")
    with tempfile.TemporaryDirectory() as scratch:
        old_code = Path(scratch) / "old"
        shutil.copytree(project, old_code, ignore=shutil.ignore_patterns("__pycache__"))
        shutil.copy(original / "pricing.py", old_code / "pricing.py")
        caught = subprocess.run(
            [sys.executable, "-m", "unittest", *sorted(new_tests)], cwd=old_code,
            text=True, capture_output=True,
        )
    if not caught.returncode:
        fail("the regression tests pass on the original bugs, so they prove nothing")
    from models import Product, StockItem
    from pricing import discounted_subtotal, total_with_tax
    from report import receipt

    items = [StockItem(Product("M", "Mug", 2.50), 4)]
    if not math.isclose(discounted_subtotal(items, 10), 9.0):
        fail("discount subtotal does not include every item quantity")
    if total_with_tax(items, discount=10, tax_rate=20) != 10.80:
        fail("total_with_tax must apply the discount once and round at the end")
    rounding_items = [StockItem(Product("P", "Pen", 0.05), 3)]
    if total_with_tax(rounding_items, discount=10, tax_rate=20) != 0.16:
        fail("total_with_tax must not round the discounted subtotal before tax")
    if total_with_tax(items, discount=100, tax_rate=20) != 0:
        fail("a 100 percent discount must produce zero")
    if "TOTAL 10.80" not in receipt(items, discount=10, tax_rate=20):
        fail("receipt total is incorrect for multiple quantities and tax")
    print("verified: inventory-bug")


if __name__ == "__main__":
    main()
