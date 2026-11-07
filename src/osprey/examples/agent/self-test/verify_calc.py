"""Independent acceptance for the real calc project: preserved tests and behavior.

    verify_calc.py <project> [--tests] [--original <original test file>]
"""
import ast
import pathlib
import re
import statistics
import subprocess
import sys


def original_tests_remain(source, original):
    """Adding tests is allowed; changing existing test methods is not."""
    before, after = ast.parse(pathlib.Path(original).read_text()), ast.parse(source)
    methods = {node.name: ast.dump(node) for node in ast.walk(after)
               if isinstance(node, ast.FunctionDef) and node.name.startswith("test_")}
    for node in ast.walk(before):
        if isinstance(node, ast.FunctionDef) and node.name.startswith("test_"):
            if methods.get(node.name) != ast.dump(node):
                sys.exit(f"calc: original {node.name} was changed or removed")


def check_tests(project, tests_required, original):
    source = pathlib.Path(project, "test_calc.py").read_text()
    for kept in ["test_add", "test_mean"]:
        if kept not in source:
            sys.exit(f"calc: {kept} was removed")
    if tests_required and "median" not in source:
        sys.exit("calc: median has no tests")
    if original:
        original_tests_remain(source, original)
    run = subprocess.run([sys.executable, "test_calc.py"], cwd=project,
                         text=True, capture_output=True, timeout=30)
    print(run.stdout + run.stderr)
    count = re.search(r"Ran (\d+) tests?", run.stderr)
    if run.returncode or not count or int(count[1]) < (3 if tests_required else 2):
        sys.exit("calc: the project's tests do not pass or did not run enough cases")


def check_behaviors(project):
    sys.path.insert(0, project)
    from calc import add, mean, median
    cases = [(add(2, 3), 5), (mean([1, 2, 3]), 2), (median([3, 1, 2]), 2),
             (median([4, 1, 3, 2]), 2.5), (median([7]), 7), (median([1.5, 2.5]), 2.0),
             (add(-5, 2), -3), (add(0, 0), 0), (add(1.5, 2.5), 4.0)]
    for got, want in cases:
        if got != want:
            sys.exit(f"calc: got {got}, wanted {want}")
    for values in [[3, 1, 2], [4, 1, 3, 2], [7], [1.5, 2.5], [-9, -3, -5],
                   [0, 0, 0, 0], [8, 2, 2, 2, 9], [-4, 0, 7, 7, 20, 2]]:
        for fn, oracle in [(median, statistics.median), (mean, statistics.mean)]:
            original = values.copy()
            if fn(values) != oracle(original) or values != original:
                sys.exit(f"calc: {fn.__name__} failed {original} or changed its input")
    print("verified: calc (25 behavior cases, including input preservation)")


def main(project, tests_required, original):
    check_tests(project, tests_required, original)
    check_behaviors(project)


if __name__ == "__main__":
    original_path = sys.argv[sys.argv.index("--original") + 1] if "--original" in sys.argv else None
    main(sys.argv[1], "--tests" in sys.argv[2:], original_path)
