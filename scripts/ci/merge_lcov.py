#!/usr/bin/env python3
"""Merges LCOV reports into one on stdout: a line's or a function's hits add up
across reports, so a line or function any shard ran counts as hit. Function
records are kept so the Rust gate can read functions as well as lines (#232)."""
import sys
from collections import defaultdict


def merged(paths):
    """Hits per line, and per function with its first line, per source file, over every report."""
    lines = defaultdict(lambda: defaultdict(int))
    functions = defaultdict(dict)
    calls = defaultdict(lambda: defaultdict(int))
    for path in paths:
        source = None
        with open(path, encoding="utf-8") as report:
            for line in report:
                if line.startswith("SF:"):
                    source = line[3:].strip()
                elif line.startswith("DA:"):
                    number, hits = line[3:].split(",")[:2]
                    lines[source][int(number)] += int(hits)
                elif line.startswith("FN:"):
                    number, name = line[3:].strip().split(",", 1)
                    functions[source].setdefault(name, int(number))
                elif line.startswith("FNDA:"):
                    hits, name = line[5:].strip().split(",", 1)
                    calls[source][name] += int(hits)
    return lines, functions, calls


def main():
    lines, functions, calls = merged(sys.argv[1:])
    for source in sorted(set(lines) | set(functions)):
        print(f"SF:{source}")
        for name, number in sorted(functions[source].items(), key=lambda item: (item[1], item[0])):
            print(f"FN:{number},{name}")
        for name in sorted(functions[source]):
            print(f"FNDA:{calls[source][name]},{name}")
        print(f"FNF:{len(functions[source])}")
        print(f"FNH:{sum(1 for name in functions[source] if calls[source][name])}")
        for number, hits in sorted(lines[source].items()):
            print(f"DA:{number},{hits}")
        print(f"LH:{sum(1 for hits in lines[source].values() if hits)}")
        print(f"LF:{len(lines[source])}")
        print("end_of_record")


if __name__ == "__main__":
    main()
