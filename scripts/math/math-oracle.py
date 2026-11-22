#!/usr/bin/env python3
"""[MATH-CODE-ORACLE]: every tracked math source is accepted by the pinned Lean, and Lean
computes the same bits as the Osprey module generated from it, literal by literal and
definition by definition, under GC and ARC. A control that must diverge proves the
comparison can fail."""
import argparse
import itertools
import json
import os
import re
import struct
import subprocess
import sys
import tempfile
from decimal import Decimal
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOOLCHAIN = ROOT / "src/toolchain/benchmarks/proof/tools/lean-toolchain"
PROOF_CORPORA = ("src/osprey/examples/proof/", "src/toolchain/benchmarks/proof/")
FLOATS = [0.0, -0.0, 0.005, 0.01, 0.25, 0.5, 0.75, 0.99, 0.995, 1.0, 2.0, 3.5, -1.0, -2.5, 1e-9, 1e9]
POINTS = 160
"""The most points one definition is evaluated at; a larger grid is sampled by stride."""
CHUNK = 16
EXPORT = re.compile(r"^    export (\w+)( =)?", re.M)
LITERAL = re.compile(r"(?<![\w.])\d+\.\d+")
SHOW = 'def shown (x : Float) : String := if x.isNaN then "nan" else toString x.toBits\n'


def math_sources() -> list[Path]:
    listed = subprocess.run(["git", "ls-files", "*.lean"], cwd=ROOT, check=True, capture_output=True, text=True)
    return [ROOT / line for line in listed.stdout.splitlines() if not line.startswith(PROOF_CORPORA)]


def run(command: list[str], cwd: Path) -> str:
    env = {**os.environ, "LEAN_PATH": str(cwd)}
    done = subprocess.run(command, cwd=cwd, capture_output=True, text=True, timeout=300, env=env)
    if done.returncode != 0 or "error" in done.stderr:
        raise SystemExit(f"FAIL {' '.join(command)}\n{done.stdout}{done.stderr}")
    return done.stdout


def decimal(value: float) -> str:
    """The same plain decimal for both languages; Osprey reads no exponent."""
    text = format(Decimal(repr(abs(value))), "f")
    text = text if "." in text else text + ".0"
    return f"-{text}" if str(value).startswith("-") else text


def bits(text: str) -> str:
    """A printed Osprey value as the token Lean prints: bits for a float, `nan`, or a bool."""
    if text in ("true", "false"):
        return text
    value = float(text)
    return "nan" if value != value else str(struct.unpack(">Q", struct.pack(">d", value))[0])


def kind(inferred: str) -> str:
    """The oracle's axis for an inferred type: a flag, or a float (an unconstrained `number`
    reads as the Lean `Float` the source declares)."""
    return "bool" if inferred == "bool" else "float"


def definitions(compiler: Path, work: Path, stem: str, module: str) -> list[tuple[str, list[str], str]]:
    """`(name, parameter types, result type)` of every exported definition, in source order. The
    generated module carries no type headers ([MATH-CODE-EMIT]), so the types are the ones the
    compiler infers for it; an export with no parameters is a float constant."""
    path = work / f"{stem}.ospml"
    path.write_text(module)
    symbols = json.loads(run([str(compiler), str(path), "--symbols"], work))
    inferred = {
        entry["name"].split("::")[-1]: entry
        for entry in symbols
        if entry.get("kind") == "function"
    }
    found = []
    for name, constant in EXPORT.findall(module):
        if constant:
            found.append((name, [], "float"))
            continue
        entry = inferred.get(name)
        if entry is None:
            raise SystemExit(f"FAIL {stem}: the compiler reports no signature for export `{name}`")
        types = [kind(parameter["type"]) for parameter in entry["parameters"]]
        found.append((name, types, kind(entry["returnType"])))
    if not found:
        raise SystemExit(f"FAIL {stem}: the generated module exports no definition to compare")
    return found


def grid(types: list[str]) -> list[tuple]:
    axes = [FLOATS if kind == "float" else [True, False] for kind in types]
    points = list(itertools.product(*axes))
    return points[:: -(-len(points) // POINTS)]


def argument(value, lean: bool) -> str:
    if isinstance(value, bool):
        return str(value).lower()
    text = decimal(value)
    return f"({text})" if lean and text.startswith("-") else text


def lean_call(name: str, point: tuple, result: str) -> str:
    call = " ".join([name, *(argument(value, True) for value in point)])
    return f"shown ({call})" if result == "float" else f"toString ({call})"


def osprey_call(name: str, point: tuple) -> str:
    arguments = [argument(value, False) for value in point]
    if not arguments:
        return name
    return f"{name} ({', '.join(arguments)})"


def lean_values(work: Path, stem: str, prefix: str, defs, literals: list[str]) -> list[str]:
    lines = [f"import {stem}\n", SHOW]
    for name, types, result in defs:
        calls = ", ".join(lean_call(prefix + name, point, result) for point in grid(types))
        lines.append(f'#eval IO.println (" ".intercalate [{calls}])\n')
    shown = ", ".join(f"shown ({literal} : Float)" for literal in literals)
    lines.append(f'#eval IO.println (" ".intercalate [{shown}])\n')
    (work / "Driver.lean").write_text("".join(lines))
    return run(["lean", "Driver.lean"], work).split()


def osprey_values(compiler: Path, work: Path, source: Path, module: str, defs, memory: str) -> list[str]:
    project = work / "osprey"
    (project / "src").mkdir(parents=True, exist_ok=True)
    (project / "src" / source.name).write_text(source.read_text())
    namespace = re.search(r"^namespace (\w+)$", module, re.M)
    owner = re.search(r"^module (\w+)$", module, re.M).group(1)
    calls = [f"${{{osprey_call(f'{owner}::{name}', point)}}}" for name, types, _ in defs for point in grid(types)]
    prints = [f'    print "{" ".join(calls[at:at + CHUNK])}"' for at in range(0, len(calls), CHUNK)]
    space = namespace.group(1) if namespace else "oracle"
    (project / "src/main.ospml").write_text(f"namespace {space}\n\nmain () =\n" + "\n".join(prints) + "\n")
    manifest = f'[project]\nname = "oracle"\nsource_roots = ["src"]\ndefault_namespace = "{space}"\nentry = "src/main.ospml"\n'
    (project / "osprey.toml").write_text(manifest)
    out = run([str(compiler), str(project), "--run", "--quiet", "--no-warnings", f"--memory={memory}"], work)
    return [bits(token) for token in out.split()]


def check(compiler: Path, source: Path) -> int:
    """Lean accepts `source` and agrees with its generated module; the points compared."""
    with tempfile.TemporaryDirectory() as scratch:
        work = Path(scratch)
        (work / "lean-toolchain").write_text(TOOLCHAIN.read_text())
        (work / source.name).write_text(source.read_text())
        _ = run(["lean", "-o", f"{source.stem}.olean", source.name], work)
        module = run([str(compiler), "math", str(source)], work)
        defs = definitions(compiler, work, source.stem, module)
        literals = sorted(set(LITERAL.findall(re.sub(r"\(\*\*.*?\*\)|//.*", "", module))))
        named = re.search(r"^namespace (\S+)", source.read_text(), re.M)
        lean = lean_values(work, source.stem, f"{named.group(1)}." if named else "", defs, literals)
        expected = lean[: len(lean) - len(literals)]
        for literal, token in zip(literals, lean[len(expected):], strict=True):
            if bits(literal) != token:
                raise SystemExit(f"FAIL {source}: literal {literal} is {token} in Lean, {bits(literal)} when read as a decimal")
        for memory in ("gc", "arc"):
            actual = osprey_values(compiler, work, source, module, defs, memory)
            if actual != expected:
                at = next((i for i, pair in enumerate(zip(actual, expected)) if pair[0] != pair[1]), min(len(actual), len(expected)))
                raise SystemExit(f"FAIL {source} ({memory}): value {at} of {len(expected)}: Lean {expected[at:at + 1]}, Osprey {actual[at:at + 1]}")
        return len(expected)


CONTROL = "namespace Control\ndef tie (x : Float) : Float := Float.{function} x\nend Control\n"


def control(compiler: Path) -> None:
    """Lean evaluates `round` while the generated module computes `floor`: the comparison must
    catch two different functions, or this oracle proves nothing."""
    with tempfile.TemporaryDirectory() as scratch:
        work = Path(scratch)
        (work / "lean-toolchain").write_text(TOOLCHAIN.read_text())
        (work / "Control.lean").write_text(CONTROL.format(function="round"))
        _ = run(["lean", "-o", "Control.olean", "Control.lean"], work)
        defs = [("tie", ["float"], "float")]
        lean = lean_values(work, "Control", "Control.", defs, [])
        source = work / "floor" / "Control.lean"
        source.parent.mkdir()
        source.write_text(CONTROL.format(function="floor"))
        module = run([str(compiler), "math", str(source)], work)
        osprey = osprey_values(compiler, work, source, module, defs, "gc")
        if osprey == lean or len(osprey) != len(lean):
            raise SystemExit(f"FAIL control: Lean's round {lean} and Osprey's floor {osprey} must disagree")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, default=ROOT / "target/release/osprey")
    args = parser.parse_args()
    compiler = args.compiler.resolve()
    control(compiler)
    print("PASS control: two different functions are caught")
    sources = math_sources()
    for source in sources:
        points = check(compiler, source)
        print(f"PASS {source.relative_to(ROOT)}: Lean accepts it; {points} values bit-identical under gc and arc")
    print(f"math oracle: {len(sources)} sources")
    return 0 if sources else 1


if __name__ == "__main__":
    sys.exit(main())
