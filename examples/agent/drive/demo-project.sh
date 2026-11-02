#!/usr/bin/env bash
# A small real project with two bugs its tests catch, for driving the app against.
set -euo pipefail
folder=${1:-/tmp/osprey-agent-demo}
rm -rf "$folder"
mkdir -p "$folder/calc"
printf '%s\n' 'def add(a, b):' '    return a - b' '' '' 'def mean(values):' '    return sum(values) / len(values) + 1' > "$folder/calc/__init__.py"
printf '%s\n' 'import unittest' 'from calc import add, mean' '' '' 'class T(unittest.TestCase):' '    def test_add(self):' '        self.assertEqual(add(2, 3), 5)' '' '    def test_mean(self):' '        self.assertEqual(mean([1, 2, 3]), 2)' '' '' 'unittest.main()' > "$folder/test_calc.py"
printf '%s\n' '# calc' '' 'A tiny calculator library. Run the tests with `python3 test_calc.py`.' > "$folder/README.md"
echo "$folder"
