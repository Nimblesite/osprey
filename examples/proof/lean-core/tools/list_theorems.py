#!/usr/bin/env python3
"""List full theorem names declared in a Lean stdlib source file.

Tracks namespace nesting (sections do not affect names) and prints each
`theorem` declaration as its fully qualified, kernel-level name.
Private declarations are omitted (they get mangled export names).
"""
import re
import sys

DECL = re.compile(
    r"^(?:(?:@\[[^\]]*\]\s*)*)"
    r"(?:(?:protected|private|unsafe|noncomputable|partial|syntax)\s+)*"
    r"theorem\s+([A-Za-z_\u03b1-\u03ba][\w'.!]*)"
)
NS = re.compile(r"^namespace\s+([\w.]+)")
END_NAMED = re.compile(r"^end\s+\S")
END_BARE = re.compile(r"^end\s*$")


def scan(lines):
    stack = []  # entries: ('ns', [parts]) or ('blk', None)
    found = []
    for raw in lines:
        line = raw.rstrip("\n")
        ns = NS.match(line)
        if ns:
            stack.append(("ns", ns.group(1).split(".")))
            continue
        if line.startswith(("section", "mutual")):
            stack.append(("blk", None))
            continue
        if END_NAMED.match(line) or END_BARE.match(line):
            # Both `end` and `end Name` close exactly one innermost block.
            if stack:
                stack.pop()
            continue
        m = DECL.match(line)
        if m:
            if "private" in line.split("theorem")[0]:
                continue
            parts = []
            for kind, val in stack:
                if kind == "ns":
                    parts.extend(val)
            parts.extend(m.group(1).split("."))
            found.append(".".join(parts))
    return found


def main():
    if len(sys.argv) != 2:
        sys.exit("usage: list_theorems.py <file.lean>")
    with open(sys.argv[1], encoding="utf-8") as f:
        for name in scan(f.readlines()):
            print(name)


if __name__ == "__main__":
    main()
