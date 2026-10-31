#!/usr/bin/env python3
"""Pin, export and probe Lean 4.25.0 core theorem groups for examples/proof/lean-core.

Grouped mode (default): for each group, validate selected names against the
stdlib source, run the pinned lean4export (benchmarks/proof/tools) once per
group, compute export stats, resolve closure axiom names from the NDJSON
interning, then probe each selected theorem through
target/proof/openai/check-batch.gc. Emits corpus.json.

Frontier mode (--frontier, optional --only <comma-separated group ids>): for
the currently BLOCKED groups, export and probe each selected theorem's
complete closure individually (one pinned exporter run per root), recording
every success and every refusal verbatim, then consolidate the independently
accepted roots of each group into one shared export re-verified against the
probe binary. Emits frontier.json (checked in); single-root exports are
temporary under target/proof/openai/lean-core-frontier and removed afterwards.

Never invents axioms or weakens statements: refusals are recorded verbatim.
A root is accepted only when the probe binary exits 0 AND its complete output is
one `PASS <exact requested name> | axioms: <ledger>` line; anything else is blocked. closure_axioms is
the whole exported closure's axiom ledger from the NDJSON interning; the
binary's `axioms:` line reports only the checked proof's dependencies. These
are distinct ledgers and are never compared as equal.
"""
import hashlib
import importlib.util
import json
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[4]
PROOF_SPEC = importlib.util.spec_from_file_location("proof_openai", REPO / "scripts/proof-openai.py")
PROOF = importlib.util.module_from_spec(PROOF_SPEC)
PROOF_SPEC.loader.exec_module(PROOF)
TOOLCHAIN = Path.home() / ".elan/toolchains/leanprover--lean4---v4.25.0"
TOOLCHAIN_LABEL = "~/.elan/toolchains/leanprover--lean4---v4.25.0"
EXPORTER_LABEL = "lean4export cfefafacc917b2d14c5f9bb092ed337f71317a4a (tag v4.25.0)"
CHECKER_LABEL = ("target/proof/openai/check-batch.gc (parent-owned; may predate latest "
                 "kernel work — parent rebuilds and runs the final matrix)")
TOOLS = REPO / "benchmarks/proof/tools"
CHECKER = REPO / "target/proof/openai/check-batch.gc"
OUT = REPO / "examples/proof/lean-core"
FRONTIER_TMP = REPO / "target/proof/openai/lean-core-frontier"
sys.path.insert(0, str(Path(__file__).parent))
from list_theorems import scan  # noqa: E402

GROUPS = [
    {"id": "nat-basic", "module": "Init.Data.Nat.Basic", "src": "Init/Data/Nat/Basic.lean", "names": [
        "Nat.succ_add", "Nat.add_succ", "Nat.add_one", "Nat.add_one_ne_zero", "Nat.zero_ne_add_one",
        "Nat.zero_add", "Nat.add_comm", "Nat.add_assoc", "Nat.add_left_comm", "Nat.add_right_comm",
        "Nat.add_left_cancel", "Nat.add_right_cancel", "Nat.eq_zero_of_add_eq_zero",
        "Nat.eq_zero_of_add_eq_zero_left", "Nat.mul_succ", "Nat.succ_mul", "Nat.mul_comm",
        "Nat.left_distrib", "Nat.right_distrib", "Nat.mul_add", "Nat.add_mul", "Nat.mul_assoc",
        "Nat.mul_left_comm", "Nat.mul_two", "Nat.two_mul", "Nat.mul_zero", "Nat.zero_mul",
        "Nat.mul_one", "Nat.one_mul", "Nat.pow_zero", "Nat.pow_succ", "Nat.sub_zero",
        "Nat.succ_lt_succ", "Nat.not_add_one_le_zero", "Nat.add_one_pos", "Nat.pred_lt",
        "Nat.sub_one_lt", "Nat.succ_sub_succ", "Nat.sub_add_eq", "Nat.le_of_eq", "Nat.lt.step",
        "Nat.le_of_succ_le", "Nat.le_of_lt"]},
    {"id": "nat-lemmas", "module": "Init.Data.Nat.Lemmas", "src": "Init/Data/Nat/Lemmas.lean", "names": [
        "Nat.sub_one", "Nat.one_add", "Nat.lt_iff_add_one_le", "Nat.one_add_le_iff",
        "Nat.pred_one_add", "Nat.add_succ_sub_one", "Nat.succ_add_sub_one", "Nat.pred_sub",
        "Nat.self_add_sub_one", "Nat.sub_one_add_self", "Nat.self_add_pred", "Nat.pred_add_self",
        "Nat.add_add_add_comm", "Nat.succ_eq_one_add", "Nat.succ_add_eq_add_succ",
        "Nat.eq_zero_of_add_eq_zero_right", "Nat.add_eq_zero_iff", "Nat.add_left_cancel_iff",
        "Nat.add_right_cancel_iff", "Nat.add_left_inj", "Nat.add_right_inj", "Nat.add_eq_left",
        "Nat.add_eq_right", "Nat.lt_of_add_lt_add_right", "Nat.lt_of_add_lt_add_left",
        "Nat.add_lt_add_iff_left", "Nat.add_lt_add_iff_right", "Nat.add_lt_add_of_le_of_lt",
        "Nat.add_lt_add_of_lt_of_le", "Nat.mul_mul_mul_comm", "Nat.succ_mul_succ",
        "Nat.add_one_mul_add_one", "Nat.sub_sub_self", "Nat.sub_sub_sub_cancel_right",
        "Nat.add_sub_cancel_right", "Nat.add_sub_cancel'", "Nat.succ_sub_sub_succ"]},
    {"id": "bool", "module": "Init.Data.Bool", "src": "Init/Data/Bool.lean", "names": [
        "Bool.false_ne_true", "Bool.eq_false_or_eq_true", "Bool.eq_iff_iff",
        "Bool.decide_eq_true", "Bool.decide_eq_false", "Bool.and_comm", "Bool.and_left_comm",
        "Bool.and_right_comm", "Bool.not_and", "Bool.not_or", "Bool.or_comm", "Bool.or_left_comm",
        "Bool.or_right_comm", "Bool.and_or_distrib_left", "Bool.and_or_distrib_right",
        "Bool.or_and_distrib_left", "Bool.or_and_distrib_right", "Bool.and_self_left",
        "Bool.or_self_left", "Bool.and_not_self", "Bool.or_not_self", "Bool.xor_false",
        "Bool.xor_true", "Bool.coe_iff_coe"]},
    {"id": "option-basic", "module": "Init.Data.Option.Basic", "src": "Init/Data/Option/Basic.lean", "names": [
        "Option.getD_none", "Option.getD_some", "Option.map_none", "Option.map_some",
        "Option.isSome_none", "Option.isSome_some", "Option.isNone_none", "Option.isNone_some",
        "Option.bind_none", "Option.bind_some", "Option.map_id", "Option.join_none",
        "Option.join_some", "Option.getM_none", "Option.getM_some", "Option.all_some",
        "Option.any_some", "Option.orElse_some", "Option.orElse_none"]},
    {"id": "list-basic", "module": "Init.Data.List.Basic", "src": "Init/Data/List/Basic.lean", "names": [
        "List.length_nil", "List.length_singleton", "List.length_cons", "List.head_cons",
        "List.headD_nil", "List.headD_cons", "List.tail_nil", "List.tail_cons", "List.map_nil",
        "List.map_cons", "List.filter_nil", "List.filterMap_nil", "List.filterMap_cons",
        "List.foldr_nil", "List.foldr_cons", "List.foldl_nil", "List.foldl_cons",
        "List.reverse_nil", "List.nil_append", "List.cons_append", "List.append_nil",
        "List.append_assoc", "List.length_append", "List.any_nil", "List.any_cons",
        "List.all_nil", "List.all_cons"]},
    {"id": "list-lemmas", "module": "Init.Data.List.Lemmas", "src": "Init/Data/List/Lemmas.lean", "names": [
        "List.map_id", "List.map_id'", "List.map_append", "List.map_reverse",
        "List.length_reverse", "List.reverse_reverse", "List.isEmpty_reverse", "List.mem_append",
        "List.not_mem_append", "List.forall_mem_append", "List.mem_cons_self",
        "List.getElem_cons", "List.getElem_zero", "List.getElem_singleton"]},
    {"id": "prod", "module": "Init.Data.Prod", "src": "Init/Data/Prod.lean", "names": [
        "Prod.map_id", "Prod.map_id'", "Prod.map_comp_map", "Prod.map_map", "Prod.swap_swap",
        "Prod.fst_swap", "Prod.snd_swap", "Prod.swap_prod_mk", "Prod.swap_swap_eq",
        "Prod.swap_inj", "Prod.map_comp_swap"]},
]

# Groups whose grouped export the current probe binary refuses wholesale at
# frontier time; --frontier probes each of their roots separately.
BLOCKED_GROUPS = ["nat-lemmas", "list-lemmas", "prod"]

DECL_KEYS = {"axiom", "def", "thm", "opaque", "quot", "inductive"}


def sha256_bytes(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def declared_theorems(src_rel):
    return set(scan((TOOLCHAIN / "src/lean" / src_rel).read_text().splitlines()))


def validate_group(group):
    known = declared_theorems(group["src"])
    missing = [n for n in group["names"] if n not in known]
    if missing:
        sys.exit(f"{group['id']}: selected names not declared in {group['src']}: {missing}")


def run_exporter(module, names, dest):
    cmd = ["lake", "exe", "lean4export", module, "--", *names]
    proc = subprocess.run(cmd, cwd=TOOLS, capture_output=True, text=True, timeout=1800)
    if proc.returncode != 0:
        return None, (proc.stdout + proc.stderr).strip()
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(proc.stdout)
    return dest, None


def first_key(line):
    m = re.match(r'^\{"([a-zA-Z?]+)":', line)
    return m.group(1) if m else None


def export_stats(export_path):
    lines = export_path.read_text().splitlines()
    decls = [l for l in lines if first_key(l) in DECL_KEYS]
    return {"sha256": hashlib.sha256(export_path.read_bytes()).hexdigest(),
            "bytes": export_path.stat().st_size, "lines": len(lines),
            "declarations": len(decls), "decl_by_kind": {
                k: sum(1 for l in decls if first_key(l) == k) for k in sorted({first_key(l) for l in decls})}}


def build_name_table(lines):
    names, _ = PROOF.names_and_theorems(map(json.loads, lines))
    return {index: name for index, name in names.items() if index}


def axiom_names(lines, table):
    named = (table[obj["axiom"]["name"]] for line in lines
             if "axiom" in (obj := json.loads(line)))
    return sorted(set(named))


def closure_axioms(lines):
    return axiom_names(lines, build_name_table(lines))


def decl_name(obj, kind):
    payload = obj[kind]
    if kind == "inductive":
        return payload["types"][0]["name"] if payload.get("types") else None
    return payload.get("name")


def decl_at_line(lines, table, lineno):
    for i in range(min(lineno, len(lines)) - 1, -1, -1):
        kind = first_key(lines[i])
        if kind in DECL_KEYS:
            name_idx = decl_name(json.loads(lines[i]), kind)
            if name_idx is not None:
                return table.get(name_idx)
    return None


def lean_meta(export_path):
    meta = json.loads(export_path.read_text().splitlines()[0])["meta"]
    return meta["lean"], meta["format"]["version"], meta["exporter"]["version"]


def axiom_types_from_lean(names):
    src = "import Init\n" + "\n".join(f"#check @{n}" for n in sorted(names)) + "\n"
    probe_file = OUT / "tools" / "axiom_types.lean"
    probe_file.write_text(src)
    proc = subprocess.run([str(TOOLCHAIN / "bin/lean"), str(probe_file)],
                          capture_output=True, text=True, timeout=600)
    if proc.returncode != 0:
        sys.exit(f"lean axiom-type probe failed: {proc.stderr[:400]}")
    return proc.stdout.strip()


def probe(export_path, name):
    proc = subprocess.run([str(CHECKER)], input=f"{export_path}\t{name}\n",
                          capture_output=True, text=True, timeout=3600)
    out = (proc.stdout + proc.stderr).strip()
    return {"name": name, "result": out, "seconds": None, "exit": proc.returncode}


def classified(result):
    receipt = rf"PASS {re.escape(result['name'])} \| axioms: [^\r\n]+"
    ok = result["exit"] == 0 and re.fullmatch(receipt, result["result"]) is not None
    result["status"] = "accepted" if ok else "blocked"
    return result


def timed_probe(export_path, name):
    started = time.time()
    result = classified(probe(export_path, name))
    result["seconds"] = round(time.time() - started, 1)
    print(f"  {result['status']:>8} {result['result'][:160]} ({result['seconds']}s)", flush=True)
    return result


def probe_all(export_path, names):
    return [timed_probe(export_path, name) for name in names]


def group_by_id(gid):
    return next(g for g in GROUPS if g["id"] == gid)


def new_report(kind):
    return {"toolchain": "leanprover/lean4:v4.25.0", "toolchain_path": TOOLCHAIN_LABEL,
            "exporter": EXPORTER_LABEL, "probe_binary": CHECKER_LABEL, "kind": kind, "groups": []}


def group_header(group):
    validate_group(group)
    return {"id": group["id"], "module": group["module"], "stdlib_source": group["src"],
            "stdlib_source_sha256": sha256_bytes(TOOLCHAIN / "src/lean" / group["src"]),
            "selected": group["names"]}


def export_or_refuse(entry, group):
    dest = OUT / "exports" / f"{group['id']}.ndjson"
    export_path, err = run_exporter(group["module"], group["names"], dest)
    if err is not None:
        entry.update({"export": "REFUSED", "export_refusal": err})
        return None
    entry["export_path"] = str(export_path.relative_to(REPO))
    return export_path


def finish_group_entry(entry, export_path):
    entry.update(export_stats(export_path))
    entry["axioms"] = closure_axioms(export_path.read_text().splitlines())
    lean, fmt, exporter_version = lean_meta(export_path)
    entry.update({"lean_meta": lean, "format_version": fmt, "exporter_version": exporter_version})
    print(f"  export {entry['lines']} lines, {entry['declarations']} declarations, "
          f"axioms: {entry['axioms']}", flush=True)


def corpus_group(report, group):
    print(f"== {group['id']} ({group['module']})", flush=True)
    entry = group_header(group)
    export_path = export_or_refuse(entry, group)
    if export_path is None:
        print(f"  EXPORT REFUSED: {entry['export_refusal'][:300]}", flush=True)
    else:
        finish_group_entry(entry, export_path)
        entry["probes"] = probe_all(export_path, group["names"])
    report["groups"].append(entry)
    return entry if export_path else None


def corpus_main():
    OUT.mkdir(exist_ok=True)
    (OUT / "exports").mkdir(exist_ok=True)
    report = new_report("grouped")
    lean_seen, axioms_seen = None, set()
    for group in GROUPS:
        entry = corpus_group(report, group)
        if entry is None:
            continue
        lean_seen = entry["lean_meta"]
        axioms_seen.update(entry["axioms"])
    if lean_seen:
        report["lean"] = lean_seen
    if axioms_seen:
        report["axiom_types"] = axiom_types_from_lean(sorted(axioms_seen))
    (OUT / "corpus.json").write_text(json.dumps(report, indent=2) + "\n")
    print("wrote", OUT / "corpus.json", flush=True)


def root_export_path(group, name):
    return FRONTIER_TMP / group["id"] / f"{name}.ndjson"


def root_entry(group, name):
    dest = root_export_path(group, name)
    export_path, err = run_exporter(group["module"], [name], dest)
    entry = {"group": group["id"], "module": group["module"], "name": name,
             "stdlib_source_sha256": sha256_bytes(TOOLCHAIN / "src/lean" / group["src"])}
    if err is not None:
        entry.update({"export": "REFUSED", "export_refusal": err, "status": "blocked"})
        print(f"  {name}: EXPORT REFUSED {err[:160]}", flush=True)
        return entry
    lines = export_path.read_text().splitlines()
    table = build_name_table(lines)
    entry["temp_export"] = str(export_path.relative_to(REPO))
    entry.update(export_stats(export_path))
    entry["closure_axioms"] = axiom_names(lines, table)
    entry["probe"] = timed_probe(export_path, name)
    entry["status"] = entry["probe"]["status"]
    return with_refusal_decl(entry, lines, table)


def with_refusal_decl(entry, lines, table):
    if entry["status"] != "blocked":
        return entry
    refusal = re.search(r"line (\d+)", entry["probe"]["result"])
    if refusal:
        entry["refusal_declaration"] = decl_at_line(lines, table, int(refusal.group(1)))
    return entry


def consolidation_entry(group, dest, names):
    export_path, err = run_exporter(group["module"], names, dest)
    if err is not None:
        return {"names": list(names), "retained": False, "export_refusal": err}
    probes = [timed_probe(export_path, n) for n in names]
    failing = [p for p in probes if p["status"] != "accepted"]
    return {"names": list(names), "export_path": str(dest.relative_to(REPO)),
            **export_stats(dest),
            "probes": [{"name": p["name"], "result": p["result"], "exit": p["exit"],
                        "status": p["status"]} for p in probes],
            "passing": [p["name"] for p in probes if p["status"] == "accepted"],
            "failing": [p["name"] for p in failing],
            "failing_results": [p["result"] for p in failing],
            "retained": False}


def drop_unretained(dest):
    if dest.exists():
        dest.unlink()


def greedy_trial(group, dest, accepted):
    kept, dropped = [], []
    for name in accepted:
        export_path, err = run_exporter(group["module"], kept + [name], dest)
        if err is not None:
            dropped.append({"name": name, "export_refusal": err})
            continue
        result = timed_probe(export_path, name)
        if result["status"] == "accepted":
            kept.append(name)
        else:
            dropped.append({"name": name, "result": result["result"]})
    return kept, dropped


def greedy_consolidation(group, dest, accepted):
    kept, dropped = greedy_trial(group, dest, accepted)
    if not kept:
        drop_unretained(dest)
        return {"names": accepted, "retained": False, "greedy_kept": [], "greedy_dropped": dropped,
                "note": "no consolidatable subset found; no export retained"}
    final = consolidation_entry(group, dest, kept)
    final.update({"greedy_kept": kept, "greedy_dropped": dropped,
                  "retained": not final["failing"]})
    if not final["retained"]:
        drop_unretained(dest)
    return final


def consolidate(group, accepted):
    suffix = "" if set(accepted) == set(group["names"]) else "-accepted"
    dest = OUT / "exports" / f"{group['id']}{suffix}.ndjson"
    print(f"  consolidating {len(accepted)} accepted roots -> {dest.name}", flush=True)
    entry = consolidation_entry(group, dest, accepted)
    if "export_refusal" in entry:
        print(f"  consolidation export REFUSED: {entry['export_refusal'][:160]}", flush=True)
        return entry
    if not entry["failing"]:
        entry["retained"] = True
        print(f"  consolidation retained: all {len(accepted)} accepted against shared export", flush=True)
        return entry
    print(f"  consolidation introduced blockers for {entry['failing']}; greedy fallback", flush=True)
    return greedy_consolidation(group, dest, accepted)


def frontier_group(group):
    print(f"== frontier {group['id']} ({group['module']}), {len(group['names'])} roots", flush=True)
    roots = [root_entry(group, name) for name in group["names"]]
    accepted = [r["name"] for r in roots if r["status"] == "accepted"]
    entry = {**group_header(group), "roots": roots, "accepted": accepted,
             "blocked": [r["name"] for r in roots if r["status"] != "accepted"]}
    entry["consolidation"] = (consolidate(group, accepted) if accepted
                              else {"retained": False, "note": "no independently accepted root"})
    return entry


def frontier_totals(groups):
    per = {e["id"]: {"selected": len(e["selected"]), "accepted": len(e["accepted"]),
                     "blocked": len(e["blocked"]),
                     "consolidation_retained": e["consolidation"].get("retained", False)}
           for e in groups}
    return {"per_group": per, "all": {
        "selected": sum(v["selected"] for v in per.values()),
        "accepted": sum(v["accepted"] for v in per.values()),
        "blocked": sum(v["blocked"] for v in per.values())}}


def inventory_row(gid, group, by_id):
    row = {"names": len(group["names"]), "source_sha256": by_id[gid]["stdlib_source_sha256"]}
    if gid in BLOCKED_GROUPS:
        return {**row, "ledger": "frontier.json groups[] roots (per-root)"}
    accepted = sum(classified(dict(probe))["status"] == "accepted" for probe in by_id[gid]["probes"])
    return {**row, "ledger": "corpus.json probes on the unchanged grouped export",
            "corpus_accepted_strict": accepted}


def frontier_inventory():
    full = json.loads((OUT / "corpus.json").read_text())
    by_id = {g["id"]: g for g in full["groups"]}
    inv = {g["id"]: inventory_row(g["id"], g, by_id) for g in GROUPS}
    return {"total_names": sum(v["names"] for v in inv.values()), "per_group": inv,
            "note": ("existing 7 grouped exports under exports/ are untouched; "
                     "<gid>-accepted.ndjson files are additional consolidations of "
                     "frontier-accepted roots only and never replace the grouped export")}


def frontier_manifest(report):
    seen = sorted({a for e in report["groups"] for r in e["roots"]
                   for a in r.get("closure_axioms", [])})
    if seen:
        report["closure_axiom_names_union"] = seen
        report["axiom_types"] = axiom_types_from_lean(seen)
    report["ledger_note"] = ("roots[].closure_axioms is the complete axiom ledger of each "
                             "root's exported closure from the NDJSON interning; the probe "
                             "binary's `axioms:` line reports only the checked proof's own "
                             "dependencies. Distinct ledgers; never compared as equal.")
    report["conservative_note"] = ("Counts are probe-binary observations of the current "
                                   "target/proof/openai/check-batch.gc only. No final matrix "
                                   "claim: the parent rebuilds the kernel and reruns GC/ARC/wasm.")
    report["temp_exports_note"] = ("roots[].temp_export paths under "
                                   "target/proof/openai/lean-core-frontier are removed after the "
                                   "run; each root's export is reproducible via `lake exe "
                                   "lean4export <module> -- <name>` from benchmarks/proof/tools "
                                   "and verified by its recorded sha256.")
    report["inventory_all_175"] = frontier_inventory()
    (OUT / "frontier.json").write_text(json.dumps(report, indent=2) + "\n")


def frontier_main(only):
    FRONTIER_TMP.mkdir(parents=True, exist_ok=True)
    report = new_report("frontier")
    report["frontier_temp_dir"] = str(FRONTIER_TMP.relative_to(REPO))
    report["groups"] = [frontier_group(g) for g in GROUPS
                        if g["id"] in BLOCKED_GROUPS and (not only or g["id"] in only)]
    report["totals"] = frontier_totals(report["groups"])
    frontier_manifest(report)
    print("wrote", OUT / "frontier.json", flush=True)
    shutil.rmtree(FRONTIER_TMP, ignore_errors=True)
    shutil.rmtree(OUT / "tools" / "__pycache__", ignore_errors=True)


def parse_only(argv):
    if "--only" not in argv:
        return None
    return set(argv[argv.index("--only") + 1].split(","))


def main():
    argv = sys.argv[1:]
    if "--frontier" in argv:
        frontier_main(parse_only(argv))
    else:
        corpus_main()


if __name__ == "__main__":
    main()
