#!/usr/bin/env python3
"""Attribute a macOS `sample` report of an Osprey program to Osprey functions ([DAW-PERF]).

usage: sample-attribution.py <report> [rows] [closure ...]

Prints leaf time, each Osprey function's own time plus the runtime calls it makes directly, inclusive
time, and the (function, runtime call) pairs. Naming closures also prints what each runs under and calls,
which is how an anonymous `__closure_fn_N` is traced back to its source.
"""
import re
import sys


def demangle(name):
    found = re.match(r"_?_?_osp_((?:\d+x[0-9a-f]+_?)+)(\$mono\d+)?(.*)$", name)
    if not found:
        return name
    parts = [bytes.fromhex(p.split("x", 1)[1]).decode("utf8", "replace") for p in found.group(1).strip("_").split("_")]
    return "::".join(parts) + (found.group(3) or "")


def rows_of(path):
    rows, reading = [], False
    for line in open(path, errors="replace"):
        if line.startswith("Call graph:"):
            reading = True
        elif reading and line.startswith("Total number in stack"):
            break
        elif reading:
            found = re.match(r"^([ +!:|]*?)(\d+) (.+?)  \(in ([^)]*)\)", line)
            if found:
                rows.append((len(found.group(1)), int(found.group(2)), demangle(found.group(3))))
    return rows


def osprey(name):
    return ("::" in name and not name.startswith("_R")) or name.startswith("__closure_fn")


def own_samples(rows, index):
    depth, count, _ = rows[index]
    below, child = None, 0
    for other, samples, _ in rows[index + 1:]:
        if other <= depth:
            break
        below = other if below is None else below
        child += samples if other == below else 0
    return count - child


def table(title, counts, total, limit):
    print(f"-- {title}")
    for key, value in sorted(counts.items(), key=lambda item: -item[1])[:limit]:
        print(f"{value:7d} {100 * value / total:5.1f}%  {key if isinstance(key, str) else ' -> '.join(key)}")


def attribute(path, limit, closures):
    rows, stack, total = rows_of(path), [], 0
    leaf, owned, inclusive, pairs, under, calls = {}, {}, {}, {}, {}, {}
    for index, (depth, count, name) in enumerate(rows):
        while stack and stack[-1][0] >= depth:
            stack.pop()
        if stack and stack[-1][1] in closures and name != stack[-1][1]:
            calls.setdefault(stack[-1][1], {}).setdefault(name, 0)
            calls[stack[-1][1]][name] += count
        if name in closures:
            chain = " > ".join([n for _, n in stack if osprey(n)][-3:])
            under.setdefault(name, {}).setdefault(chain, 0)
            under[name][chain] += count
        stack.append((depth, name))
        names = [n for _, n in stack]
        own = own_samples(rows, index)
        if own <= 0 or not any(osprey(n) or "osp_native" in n or "7Library5event" in n for n in names):
            continue
        total += own
        leaf[name] = leaf.get(name, 0) + own
        for seen in {n for n in names if osprey(n)}:
            inclusive[seen] = inclusive.get(seen, 0) + own
        owner = next((n for n in reversed(names) if osprey(n)), "(host)")
        owned[owner] = owned.get(owner, 0) + own
        if not osprey(name):
            pairs[(owner, name)] = pairs.get((owner, name), 0) + own
    print(f"## {path}: {total} samples in the application")
    if total == 0:
        return
    table("leaf", leaf, total, limit)
    table("Osprey function: its own code plus the runtime it calls directly", owned, total, limit)
    table("inclusive", inclusive, total, limit)
    table("Osprey function -> runtime call", pairs, total, limit)
    for closure in closures:
        print(f"-- {closure}")
        for chain, value in sorted(under.get(closure, {}).items(), key=lambda item: -item[1])[:3]:
            print(f"{value:7d}  under {chain}")
        print("         calls " + ", ".join(f"{k} x{v}" for k, v in sorted(calls.get(closure, {}).items(), key=lambda item: -item[1])[:8]))


attribute(sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 25, sys.argv[3:])
