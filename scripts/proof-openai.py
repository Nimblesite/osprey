#!/usr/bin/env python3
"""Check pinned OpenAI and Lean core proof exports [PROOF-RUNNABLE-LIBRARY]."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
from functools import reduce

ROOT = Path(__file__).resolve().parent.parent
CORPUS = ROOT / "examples/proof/openai"
TOOLS = ROOT / "benchmarks/proof/tools"
OUTPUT = ROOT / "target/proof/openai"
CORE = ROOT / "examples/proof/lean-core"
CORE_AXIOMS = {"nat-basic": ["propext"], "nat-lemmas": ["propext"], "bool": ["propext"],
               "option-basic": ["Quot.sound"], "list-basic": ["propext"],
               "list-lemmas": ["propext", "Quot.sound"], "prod": ["Quot.sound", "propext"]}


def core_inventory():
    return json.loads((CORE / "corpus.json").read_text()), json.loads((CORE / "frontier.json").read_text())


def core_selected(group, frontier):
    selected = group["selected"]
    if frontier is None:
        return selected, group
    accepted, blocked = frontier["accepted"], frontier["blocked"]
    if (frontier["selected"] != selected or len(set(selected)) != len(selected)
            or len(accepted) + len(blocked) != len(selected) or set(accepted) & set(blocked)
            or set(accepted) | set(blocked) != set(selected)):
        raise ValueError("core frontier is not a complete disjoint partition")
    exported = frontier["consolidation"]
    if not exported["retained"] or exported["names"] != accepted or exported["passing"] != accepted or exported["failing"]:
        raise ValueError("core consolidation differs from the accepted roots")
    return accepted, exported


def core_groups(corpus=None, frontier=None):
    corpus, frontier = core_inventory() if corpus is None else (corpus, frontier)
    if corpus["toolchain"] != "leanprover/lean4:v4.25.0" or frontier["toolchain"] != corpus["toolchain"]:
        raise ValueError("core toolchain differs from the pinned source root")
    extras = {group["id"]: group for group in frontier["groups"]}
    ledger = json.loads((CORE / "root-axioms.json").read_text())
    identifiers = [group["id"] for group in corpus["groups"]]
    if len(set(identifiers)) != len(identifiers) or set(ledger) != set(identifiers) or not set(extras) <= set(identifiers):
        raise ValueError("core corpus, frontier and ledger group inventories differ")
    groups = []
    for group in corpus["groups"]:
        names, exported = core_selected(group, extras.get(group["id"]))
        if set(ledger[group["id"]]) != set(names):
            raise ValueError("core root axiom ledger differs from the accepted roots")
        groups.append({**group, "theorems": names, "export_path": exported["export_path"],
                       "export_sha256": exported["sha256"], "axioms": CORE_AXIOMS[group["id"]],
                       "theorem_axioms": ledger[group["id"]], "core": True})
    return groups + compatible_groups(corpus, frontier)


def compatible_groups(corpus, frontier):
    ports = json.loads((CORE / "compatible.json").read_text())
    if ports["toolchain"] != corpus["toolchain"]:
        raise ValueError("compatible toolchain differs from the core corpus")
    originals = {group["id"]: group for group in corpus["groups"]}
    blocked = {group["id"]: set(group["blocked"]) for group in frontier["groups"]}
    groups = []
    for port in ports["groups"]:
        original = originals[port["original_id"]]
        if not set(port["theorems"]) <= blocked[original["id"]]:
            raise ValueError("compatible proof does not replace an inventoried refusal")
        groups.append({**port, "original": {**original, "core": True, "export_sha256": original["sha256"]}})
    return groups


def export_path(group):
    return ROOT / group["export_path"] if "export_path" in group else CORPUS / (group["id"] + ".ndjson")


def pinned_files(group):
    if "source_path" in group:
        return [(ROOT / group["source_path"], "source_sha256"), (export_path(group), "export_sha256")]
    if group.get("core"):
        stdlib = Path.home() / ".elan/toolchains/leanprover--lean4---v4.25.0/src/lean"
        return [(stdlib / group["stdlib_source"], "stdlib_source_sha256"), (export_path(group), "export_sha256")]
    return [(CORPUS / "upstream" / Path(group["upstream"]).name, "upstream_sha256"),
            (TOOLS / (group["module"] + ".lean"), "port_sha256"), (export_path(group), "export_sha256")]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def run(command, **kwargs):
    result = subprocess.run(command, text=True, capture_output=True, **kwargs)
    if result.returncode:
        raise RuntimeError(f"{command}:\n{result.stdout}\n{result.stderr}")
    return result.stdout


def names_and_theorems(rows):
    names, theorems = {0: ""}, {}
    for row in rows:
        if "in" in row:
            value = row.get("str", row.get("num"))
            prefix = names[value["pre"]]
            names[row["in"]] = prefix + ("." if prefix else "") + str(value.get("str", value.get("i")))
        if "thm" in row:
            theorems[names[row["thm"]["name"]]] = row["thm"]
    return names, theorems


def expression_shape(index, expressions, names):
    row = expressions[index]
    if "sort" in row:
        return ["sort", row["sort"]]
    if "bvar" in row:
        return ["bvar", row["bvar"]]
    if "const" in row:
        value = row["const"]
        return ["const", names[value["name"]], value["us"]]
    return compound_shape(row, expressions, names)


def compound_shape(row, expressions, names):
    if "app" in row:
        value = row["app"]
        return ["app", expression_shape(value["fn"], expressions, names), expression_shape(value["arg"], expressions, names)]
    if "natVal" in row:
        return ["nat", int(row["natVal"])]
    for key in ("forallE", "lam"):
        if key in row:
            value = row[key]
            return [key, expression_shape(value["type"], expressions, names), expression_shape(value["body"], expressions, names)]
    raise ValueError(f"unexpected axiom type expression: {row}")


def level_shape(index, levels, names):
    if index == 0:
        return ["zero"]
    row = levels[index]
    if "param" in row:
        return ["param", names[row["param"]]]
    if "succ" in row:
        return ["succ", level_shape(row["succ"], levels, names)]
    for key in ("max", "imax"):
        if key in row:
            return [key, *[level_shape(child, levels, names) for child in row[key]]]
    raise ValueError(f"unexpected universe: {row}")


def statement_expression(row, levels, names):
    if "sort" in row:
        return {**row, "sort": level_shape(row["sort"], levels, names)}
    if "const" in row:
        value = row["const"]
        return {**row, "const": {**value, "us": [level_shape(i, levels, names) for i in value["us"]]}}
    return row


def statement_shapes(rows, selected):
    names, declarations = names_and_theorems(rows)
    levels = {row["il"]: row for row in rows if "il" in row}
    expressions = {row["ie"]: statement_expression(row, levels, names) for row in rows if "ie" in row}
    return {name: expression_shape(declarations[name]["type"], expressions, names) for name in selected}


def check_statements(group, rows):
    if "original" in group:
        original = export_rows(group["original"])
        if statement_shapes(rows, group["theorems"]) != statement_shapes(original, group["theorems"]):
            raise ValueError("compatible proof differs from an original statement")


def propext_shape(one):
    prop, a, b = ["sort", 0], ["bvar", 1], ["bvar", 0]
    iff = ["app", ["app", ["const", "Iff", []], a], b]
    eq = ["app", ["app", ["app", ["const", "Eq", [one]], prop], ["bvar", 2]], ["bvar", 1]]
    return ["forallE", prop, ["forallE", prop, ["forallE", iff, eq]]]


def applied(head, *arguments):
    return reduce(lambda function, argument: ["app", function, argument], arguments, head)


def quot_sound_shape(universe):
    variable = lambda index: ["bvar", index]
    constant = lambda name: ["const", name, [universe]]
    relation = ["forallE", variable(0), ["forallE", variable(1), ["sort", 0]]]
    hypothesis = applied(variable(2), variable(1), variable(0))
    quotient = applied(constant("Quot"), variable(4), variable(3))
    representative = lambda index: applied(constant("Quot.mk"), variable(4), variable(3), variable(index))
    equality = applied(constant("Eq"), quotient, representative(2), representative(1))
    return ["forallE", ["sort", universe], ["forallE", relation,
            ["forallE", variable(1), ["forallE", variable(2), ["forallE", hypothesis, equality]]]]]


def approved_axiom(axiom, names, levels):
    name, parameters = names[axiom["name"]], axiom["levelParams"]
    if name == "propext" and not parameters:
        one = next((i for i, row in levels.items() if row == {"il": i, "succ": 0}), None)
        if one is not None:
            return propext_shape(one)
    if name == "Quot.sound" and len(parameters) == 1:
        universe = next((i for i, row in levels.items() if row == {"il": i, "param": parameters[0]}), None)
        if universe is not None:
            return quot_sound_shape(universe)
    raise ValueError("unapproved axiom or universe")


def check_axioms(rows, names, expected):
    expressions = {row["ie"]: row for row in rows if "ie" in row}
    levels = {row["il"]: row for row in rows if "il" in row}
    axioms = [row["axiom"] for row in rows if "axiom" in row]
    actual = [names[axiom["name"]] for axiom in axioms]
    if actual != expected:
        raise ValueError(f"axiom ledger differs: {actual} != {expected}")
    for axiom in axioms:
        shape = expression_shape(axiom["type"], expressions, names)
        if axiom["isUnsafe"] or shape != approved_axiom(axiom, names, levels):
            raise ValueError("axiom statement differs from the approved logical basis")


def checked_paths(group):
    if "original" in group:
        checked_paths(group["original"])
    paths = pinned_files(group)
    for path, key in paths:
        if digest(path.read_bytes()) != group[key]:
            raise ValueError(f"pinned source/export changed: {path}")
    return paths


def check_files(group):
    checked_paths(group)
    return check_export(group)


def export_rows(group):
    data = export_path(group).read_bytes()
    if digest(data) != group["export_sha256"]:
        raise ValueError(f"pinned export changed: {export_path(group)}")
    return [json.loads(line) for line in data.decode().splitlines()]


def check_export(group):
    rows = export_rows(group)
    names, theorems = names_and_theorems(rows)
    check_axioms(rows, names, group["axioms"])
    if not set(group["theorems"]) <= theorems.keys():
        raise ValueError("the export omits a required theorem")
    check_statements(group, rows)
    return rows, names, theorems


def verify_lean(group):
    env = {**os.environ, "PATH": str(Path.home() / ".elan/bin") + os.pathsep + os.environ["PATH"]}
    project = ROOT / group["lean_project"] if "lean_project" in group else TOOLS
    if project != TOOLS:
        env["LEAN_PATH"] = os.pathsep.join(filter(None, [str(project / ".lake/build/lib/lean"), env.get("LEAN_PATH")]))
    if not group.get("core"):
        run(["lake", "build", group["module"]], cwd=project, env=env)
    exported = run(["lake", "exe", "lean4export", group["module"], "--", *group["theorems"]], cwd=TOOLS, env=env)
    if exported.encode() != export_path(group).read_bytes():
        raise ValueError("fresh Lean export differs from the pinned recording")
    run(["lake", "exe", "lean4checker", "--fresh", group["module"]], cwd=TOOLS, env=env)
    verify_dependencies(group, env)


def expected_axioms(group, name):
    ledger = group["theorem_axioms"]
    if set(ledger) != set(group["theorems"]) or not set(ledger[name]) <= set(group["axioms"]):
        raise ValueError("per-theorem axiom ledger differs from the corpus")
    return ledger[name]


def verify_dependencies(group, env):
    with tempfile.TemporaryDirectory(prefix="osprey-lean-axioms-") as directory:
        path = Path(directory) / "RootAxioms.lean"
        header = ("prelude\n" if group.get("prelude") else "") + "import " + group["module"] + "\n"
        path.write_text(header + "\n".join("#print axioms " + n for n in group["theorems"]) + "\n")
        actual = run(["lake", "env", "lean", str(path)], cwd=TOOLS, env=env).splitlines()
    expected = [lean_axioms_line(n, expected_axioms(group, n)) for n in group["theorems"]]
    if actual != expected:
        raise ValueError(f"fresh Lean axiom dependencies differ: {actual} != {expected}")


def lean_axioms_line(name, axioms):
    suffix = "depends on axioms: [" + ", ".join(axioms) + "]" if axioms else "does not depend on any axioms"
    return f"'{name}' {suffix}"


def checker_command(compiler, memory, reuse):
    suffix = "wasm" if memory == "wasm32" else memory
    binary = OUTPUT / ("check-batch." + suffix)
    if not reuse:
        option = "--target=wasm32" if memory == "wasm32" else "--memory=" + memory
        run([str(compiler), "examples/proof/batch", "--compile", "--quiet", "--no-warnings", option, "-o", str(binary)], cwd=ROOT)
    return binary_command(binary)


def binary_command(binary):
    return ["node", str(ROOT / "scripts/proof-batch-wasm.mjs"), str(binary)] if binary.suffix == ".wasm" else [str(binary)]


def checker_request(command, path, name):
    wasm = command[0] == "node"
    arguments = command + ([str(path.parent)] if wasm else [])
    input_path = path.name if wasm else str(path)
    return arguments, f"{input_path}\t{name}\n"


def check(command, path, name):
    arguments, source = checker_request(command, path, name)
    return run(arguments, input=source, cwd=ROOT).strip()


def mutation(rows, target, proof):
    result = [json.loads(json.dumps(row)) for row in rows]
    position = next(i for i, row in enumerate(result) if row.get("thm", {}).get("name") == target["name"])
    if proof:
        result[position]["thm"]["value"] = target["type"]
        return result
    fresh = max(row.get("ie", -1) for row in result) + 1
    nodes = [{"ie": fresh, "sort": 0}, {"ie": fresh + 1, "bvar": 0},
             {"ie": fresh + 2, "forallE": {"binderInfo": "default", "body": fresh + 1, "name": 0, "type": fresh}}]
    result[position]["thm"]["type"] = fresh + 2
    return result[:position] + nodes + result[position:]


def rejected(command, rows, target, name, proof):
    with tempfile.TemporaryDirectory(prefix="osprey-proof-mutation-") as directory:
        path = Path(directory) / "mutation.ndjson"
        path.write_text("".join(json.dumps(row, separators=(",", ":")) + "\n" for row in mutation(rows, target, proof)))
        output = check(command, path, name)
    if not output.startswith(f"FAIL {name}: kernel: type mismatch:"):
        raise AssertionError(f"mutation must be rejected for a type mismatch, got {output}")


def verify_group(command, group, memory):
    rows, _, theorems = check_files(group)
    for name in group["theorems"]:
        expected = f"PASS {name} | axioms: {', '.join(expected_axioms(group, name)) or 'none'}"
        output = check(command, export_path(group), name)
        if output != expected:
            raise AssertionError(f"{memory}: expected {expected}, got {output}")
        rejected(command, rows, theorems[name], name, True)
        rejected(command, rows, theorems[name], name, False)
        print(f"{memory}: {output}; corrupted proof and false statement rejected", flush=True)


def verify_contract(command):
    verify_unused_axiom(command)
    path = ROOT / "examples/proof/kernel/exports/nat_add_comm.ndjson"
    kinds = {"Nat": "a type former", "Nat.zero": "a constructor", "Nat.rec": "a recursor", "Nat.add": "a definition"}
    for name, kind in kinds.items():
        expected = f"FAIL {name}: declared {kind}, not a theorem"
        if check(command, path, name) != expected:
            raise AssertionError(f"non-theorem accepted: {name}")
    path = CORPUS / "stacks.ndjson"
    if check(command, path, "propext") != "FAIL propext: declared an axiom, not a theorem":
        raise AssertionError("an axiom is not a proved theorem")
    if not check(command, path, "missing.theorem").startswith("FAIL missing.theorem: kernel:"):
        raise AssertionError("missing theorem accepted")
    if not check(command, CORPUS / "missing.ndjson", "missing").startswith("FAIL missing: read:"):
        raise AssertionError("missing file accepted")


def verify_unused_axiom(command):
    source = ROOT / "examples/proof/kernel/exports/id_imp.ndjson"
    rows = [json.loads(line) for line in source.read_text().splitlines()]
    fresh = max(row.get("in", 0) for row in rows) + 1
    prop = next(row["ie"] for row in rows if row.get("sort") == 0)
    rows += [{"in": fresh, "str": {"pre": 0, "str": "unusedAssumption"}},
             {"axiom": {"isUnsafe": False, "levelParams": [], "name": fresh, "type": prop}}]
    with tempfile.TemporaryDirectory(prefix="osprey-unused-axiom-") as directory:
        path = Path(directory) / "unused.ndjson"
        path.write_text("".join(json.dumps(row) + "\n" for row in rows))
        if check(command, path, "id_imp") != "PASS id_imp | axioms: none":
            raise AssertionError("an unrelated axiom was reported as a theorem dependency")


def arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, default=ROOT / "target/ci/osprey")
    parser.add_argument("--corpus", choices=("openai", "lean-core"), default="openai")
    parser.add_argument("--memory", choices=("all", "gc", "arc", "wasm32"), default="all")
    parser.add_argument("--reuse-binaries", action="store_true")
    parser.add_argument("--skip-lean", action="store_true", help="rerun Osprey only; does not establish an independent Lean cross-check")
    return parser.parse_args()


def main():
    args = arguments()
    groups = core_groups() if args.corpus == "lean-core" else json.loads((CORPUS / "corpus.json").read_text())["groups"]
    OUTPUT.mkdir(parents=True, exist_ok=True)
    for group in groups:
        check_files(group)
        if not args.skip_lean:
            verify_lean(group)
    memories = ("gc", "arc", "wasm32") if args.memory == "all" else (args.memory,)
    for memory in memories:
        command = checker_command(args.compiler.resolve(), memory, args.reuse_binaries)
        verify_contract(command)
        for group in groups:
            verify_group(command, group, memory)


if __name__ == "__main__":
    main()
