"""Checks the calc task by behaviour: the project's own tests pass, and median is right.

    verify_calc.py <project> [--tests]    --tests also requires tests of median in test_calc.py
"""
import pathlib
import subprocess
import sys


def main(project, tests_required):
    run = subprocess.run([sys.executable, "test_calc.py"], cwd=project, text=True, capture_output=True)
    print(run.stderr)
    if run.returncode:
        sys.exit("calc: the project's tests do not pass")
    tests = pathlib.Path(project, "test_calc.py").read_text()
    for kept in ["test_add", "test_mean"]:
        if kept not in tests:
            sys.exit(f"calc: {kept} was removed")
    if tests_required and "median" not in tests:
        sys.exit("calc: median has no tests")
    sys.path.insert(0, project)
    from calc import add, mean, median
    cases = [(add(2, 3), 5), (mean([1, 2, 3]), 2), (median([3, 1, 2]), 2), (median([4, 1, 3, 2]), 2.5), (median([7]), 7), (median([1.5, 2.5]), 2.0)]
    for got, want in cases:
        if got != want:
            sys.exit(f"calc: got {got}, wanted {want}")
    values = [3, 1, 2]
    median(values)
    if values != [3, 1, 2]:
        sys.exit("calc: median changed its input")
    print("verified: calc")


if __name__ == "__main__":
    main(sys.argv[1], "--tests" in sys.argv[2:])
