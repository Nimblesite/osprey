#!/usr/bin/env python3
"""Merges LCOV reports into one on stdout: a line's hits add up across reports, so a line any shard ran counts as hit."""
import sys
from collections import defaultdict


def merged(paths):
    """Hits per line per source file, over every report."""
    files = defaultdict(lambda: defaultdict(int))
    for path in paths:
        source = None
        with open(path, encoding="utf-8") as report:
            for line in report:
                if line.startswith("SF:"):
                    source = line[3:].strip()
                elif line.startswith("DA:"):
                    number, hits = line[3:].split(",")[:2]
                    files[source][int(number)] += int(hits)
    return files


def main():
    for source, lines in sorted(merged(sys.argv[1:]).items()):
        print(f"SF:{source}")
        for number, hits in sorted(lines.items()):
            print(f"DA:{number},{hits}")
        print(f"LH:{sum(1 for hits in lines.values() if hits)}")
        print(f"LF:{len(lines)}")
        print("end_of_record")


if __name__ == "__main__":
    main()
