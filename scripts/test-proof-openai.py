#!/usr/bin/env python3
"""Trust-ledger regressions against the actual recorded proof corpus."""
import copy
import importlib.util
import json
import re
from pathlib import Path
import tempfile
import unittest
from unittest import mock

def load_script(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


PROOF = load_script("proof_openai", Path(__file__).with_name("proof-openai.py"))
BUILDER = load_script("core_builder", PROOF.CORE / "tools/build_corpus.py")


class CorpusTests(unittest.TestCase):
    def test_name_reference_refuses_values_the_kernel_json_cannot_represent(self):
        for value in ({"i": -1}, {"i": 2 ** 63}, {"str": "a\0b"}):
            with self.subTest(value=value), self.assertRaises(ValueError):
                PROOF.name_component(value)

    def test_group_receipts_keep_ambiguous_axiom_names_distinct(self):
        group = {"theorems": ["t"]}
        for names, receipt in ((["none"], '"none"'), (["Ax, Other"], '"Ax, Other"'),
                               (['"quoted"'], json.dumps('"quoted"'))):
            with self.subTest(names=names), mock.patch.object(PROOF, "check_files", return_value=([], {}, {"t": {}})), \
                    mock.patch.object(PROOF, "expected_axioms", return_value=names), \
                    mock.patch.object(PROOF, "export_path", return_value=Path("unused")), \
                    mock.patch.object(PROOF, "check", return_value=f"PASS t | axioms: {receipt}"), \
                    mock.patch.object(PROOF, "rejected"), mock.patch("builtins.print"):
                PROOF.verify_group([], group, "gc")

    def setUp(self):
        self.groups = json.loads((PROOF.CORPUS / "corpus.json").read_text())["groups"]
        self.rows, self.names, self.theorems = PROOF.check_files(self.groups[0])

    def changed_axiom(self, key, value, name=None):
        rows = copy.deepcopy(self.rows)
        candidates = [row["axiom"] for row in rows if "axiom" in row]
        next(axiom for axiom in candidates if name is None or self.names[axiom["name"]] == name)[key] = value
        return rows

    def rejected_axiom(self, rows):
        with self.assertRaises((ValueError, KeyError)):
            PROOF.check_axioms(rows, self.names, self.groups[0]["axioms"])

    def test_export_name_identity_preserves_components_and_leaf_kinds(self):
        rows = [{"in": 1, "str": {"pre": 0, "str": "A"}},
                {"in": 2, "str": {"pre": 1, "str": "B"}},
                {"in": 3, "str": {"pre": 0, "str": "A.B"}},
                {"in": 4, "num": {"pre": 0, "i": 5}},
                {"in": 5, "str": {"pre": 0, "str": "5"}},
                {"in": 6, "str": {"pre": 0, "str": ""}},
                {"in": 7, "str": {"pre": 0, "str": '"A.B"'}},
                {"in": 8, "str": {"pre": 3, "str": "C"}}]
        names, _ = PROOF.names_and_theorems(rows)
        self.assertEqual(names, {0: "", 1: "A", 2: "A.B", 3: '"A.B"', 4: "5",
                                 5: '"5"', 6: '""', 7: json.dumps('"A.B"'), 8: '"A.B".C'})
        self.assertEqual(len(set(names.values())), len(names))
        self.assertEqual(BUILDER.build_name_table([json.dumps(row) for row in rows]),
                         {key: value for key, value in names.items() if key})

    def test_actual_pinned_sources_exports_and_axiom_statements(self):
        self.assert_exports(self.groups)

    def assert_exports(self, groups):
        for group in groups:
            rows, _, theorems = PROOF.check_files(group)
            self.assertGreater(len(rows), 100)
            self.assertTrue(set(group["theorems"]) <= theorems.keys())

    def test_workbench_custom_operator_fixtures_match_pinned_lean_sources(self):
        path = PROOF.ROOT / "examples/proof/kernel/exports/notation_cases.json"
        controls = json.loads(path.read_text())
        self.assertEqual(controls["toolchain"], "leanprover/lean4:v4.25.0")
        tests = (PROOF.ROOT / "apps/workbench/tests/imported.ospml").read_text()
        for group in controls["groups"]:
            PROOF.check_files(group)
            found = re.search(r"^    (?:export )?" + re.escape(group["embedded"]) + r" \(\) = (.+)$", tests, re.MULTILINE)
            self.assertIsNotNone(found, group["id"])
            self.assertEqual(json.loads(found.group(1)), PROOF.export_path(group).read_text())

    def test_kernel_export_fixture_twin_is_byte_identical_to_the_pinned_export(self):
        group = json.loads((PROOF.ROOT / "examples/proof/kernel/exports/recursor_cases.json").read_text())
        PROOF.check_files(group)
        tests = (PROOF.ROOT / "examples/proof/kernel/tests/export.ospml").read_text()
        found = re.search(r"^    (?:export )?" + re.escape(group["embedded"]) + r" \(\) = (.+)$", tests, re.MULTILINE)
        self.assertIsNotNone(found, group["id"])
        self.assertEqual(json.loads(found.group(1)), PROOF.export_path(group).read_text())

    def test_number_theory_port_preserves_five_original_statements(self):
        groups = {group["id"]: group for group in self.groups}
        self.assertIn("five-color-orientation", groups)
        group = groups["five-color-orientation"]
        rows, _, _ = PROOF.check_files(group)
        self.assertEqual(len(group["theorems"]), 5)
        self.assertEqual(PROOF.statement_shapes(rows, group["theorems"]),
                         PROOF.statement_shapes(PROOF.export_rows(group["original"]), group["theorems"]))
        self.assertNotEqual(group["export_sha256"], group["original"]["export_sha256"])
        self.assertEqual(group["lean_project"], "examples/proof/openai-number-theory/lean")
        self.assertEqual(set(group["theorem_axioms"]), set(group["theorems"]))
        self.assertTrue(all(basis == ["propext"] for basis in group["theorem_axioms"].values()))

    def test_fresh_lean_replay_builds_the_existing_port_project(self):
        group = {**self.groups[0], "lean_project": "examples/proof/openai-number-theory/lean"}
        project = PROOF.ROOT / group["lean_project"]
        recorded = PROOF.export_path(group).read_text()
        with mock.patch.object(PROOF, "run", return_value=recorded) as runner, \
             mock.patch.object(PROOF, "verify_dependencies") as dependencies:
            PROOF.verify_lean(group)
        self.assertEqual(runner.call_args_list[0].kwargs["cwd"], project)
        for call in runner.call_args_list[1:]:
            self.assertEqual(call.kwargs["cwd"], PROOF.TOOLS)
            self.assertEqual(call.kwargs["env"]["LEAN_PATH"].split(PROOF.os.pathsep)[0],
                             str(project / ".lake/build/lib/lean"))
        self.assertEqual(dependencies.call_args.args[1]["LEAN_PATH"].split(PROOF.os.pathsep)[0],
                         str(project / ".lake/build/lib/lean"))

    def test_root_axioms_are_distinct_from_the_export_ledger(self):
        group = self.groups[0]
        def checked(command, path, name):
            names = ", ".join(group["theorem_axioms"][name]) or "none"
            return f"PASS {name} | axioms: {names}"
        with mock.patch.object(PROOF, "check", side_effect=checked), \
             mock.patch.object(PROOF, "rejected"), mock.patch("builtins.print"):
            PROOF.verify_group(["checker"], group, "gc")

    def test_axiom_cannot_be_given_the_wrong_statement(self):
        prop = next(row["ie"] for row in self.rows if row.get("sort") == 0)
        self.rejected_axiom(self.changed_axiom("type", prop))

    def test_unsafe_axiom_is_not_trusted(self):
        self.rejected_axiom(self.changed_axiom("isUnsafe", True))

    def changed_quotient_axiom(self, key, value):
        return self.changed_axiom(key, value, "Quot.sound")

    def test_quotient_soundness_cannot_be_replaced_by_an_arbitrary_statement(self):
        prop = next(row["ie"] for row in self.rows if row.get("sort") == 0)
        self.rejected_axiom(self.changed_quotient_axiom("type", prop))

    def test_quotient_soundness_requires_its_universe_parameter(self):
        self.rejected_axiom(self.changed_quotient_axiom("levelParams", []))

    def test_unsafe_quotient_soundness_is_not_trusted(self):
        self.rejected_axiom(self.changed_quotient_axiom("isUnsafe", True))

    def test_axiom_universe_parameters_cannot_change(self):
        self.rejected_axiom(self.changed_axiom("levelParams", [1]))

    def test_extra_axiom_cannot_hide_in_a_dependency(self):
        extra = copy.deepcopy(next(row for row in self.rows if "axiom" in row))
        self.rejected_axiom([*self.rows, extra])

    def test_ledger_cannot_silently_omit_propext(self):
        with self.assertRaises(ValueError):
            PROOF.check_axioms(self.rows, self.names, ["Quot.sound"])

    def test_corruption_retains_the_theorem_statement(self):
        target = self.theorems[self.groups[0]["theorems"][0]]
        rows = PROOF.mutation(self.rows, target, True)
        _, theorems = PROOF.names_and_theorems(rows)
        changed = theorems[self.groups[0]["theorems"][0]]
        self.assertEqual(changed["type"], target["type"])
        self.assertEqual(changed["value"], target["type"])

    def test_false_statement_retains_the_actual_proof(self):
        name = self.groups[0]["theorems"][0]
        target = self.theorems[name]
        rows = PROOF.mutation(self.rows, target, False)
        names, theorems = PROOF.names_and_theorems(rows)
        changed = theorems[name]
        expressions = {row["ie"]: row for row in rows if "ie" in row}
        self.assertEqual(changed["value"], target["value"])
        self.assertEqual(PROOF.expression_shape(changed["type"], expressions, names),
                         ["forallE", ["sort", 0], ["bvar", 0]])

    def test_core_inventory_preserves_all_selected_and_blocked_roots(self):
        groups = PROOF.core_groups()
        self.assertEqual(sum(len(group["theorems"]) for group in groups), 175)
        self.assertEqual(len({name for group in groups for name in group["theorems"]}), 175)
        self.assertIn("Prod.map_id", {name for group in groups for name in group["theorems"]})

    def test_core_exports_use_the_same_structural_axiom_guard(self):
        self.assert_exports(PROOF.core_groups())

    def test_core_frontier_cannot_silently_drop_a_selected_root(self):
        corpus, frontier = PROOF.core_inventory()
        broken = copy.deepcopy(frontier)
        broken["groups"][0]["blocked"].pop()
        with self.assertRaisesRegex(ValueError, "partition"):
            PROOF.core_groups(corpus, broken)

    def test_core_frontier_cannot_promote_a_blocked_root_without_an_export(self):
        corpus, frontier = PROOF.core_inventory()
        broken = copy.deepcopy(frontier)
        group = next(group for group in broken["groups"] if group["blocked"])
        group["accepted"].append(group["blocked"].pop())
        with self.assertRaisesRegex(ValueError, "consolidation"):
            PROOF.core_groups(corpus, broken)

    def test_core_toolchain_cannot_disagree_with_the_pinned_source_root(self):
        corpus, frontier = PROOF.core_inventory()
        corpus["toolchain"] = "leanprover/lean4:unknown"
        with self.assertRaisesRegex(ValueError, "toolchain"):
            PROOF.core_groups(corpus, frontier)

    def test_core_unknown_frontier_group_cannot_be_ignored(self):
        corpus, frontier = PROOF.core_inventory()
        extra = copy.deepcopy(frontier["groups"][0])
        extra["id"] = "unknown-group"
        frontier["groups"].append(extra)
        with self.assertRaisesRegex(ValueError, "group"):
            PROOF.core_groups(corpus, frontier)

    def test_core_probe_requires_the_exact_requested_name_and_one_clean_receipt(self):
        cases = [("PASS Nat.add_comm | axioms: none", 0, "accepted"),
                 ("PASS Nat.add_comm_extra | axioms: none", 0, "blocked"),
                 ("PASS Nat.add_comm | axioms: none\nFAIL other: kernel error", 0, "blocked"),
                 ("PASS Nat.add_comm | axioms: none", 1, "blocked")]
        for output, status, expected in cases:
            with self.subTest(output=output, status=status):
                result = BUILDER.classified({"name": "Nat.add_comm", "result": output, "exit": status})
                self.assertEqual(result["status"], expected)

    def test_wasm_checker_request_uses_a_fixture_relative_path(self):
        binary = Path("proof.wasm")
        command = PROOF.binary_command(binary)
        self.assertEqual(command, ["node", str(PROOF.ROOT / "scripts/proof-batch-wasm.mjs"), str(binary)])
        arguments, source = PROOF.checker_request(command, Path("fixtures/proof.ndjson"), "target")
        self.assertEqual(arguments, [*command, "fixtures"])
        self.assertEqual(source, "proof.ndjson\ttarget\n")

    def test_core_grouped_inventory_rejects_prefixed_and_mixed_receipts(self):
        probes = [{"name": "Nat.add_comm", "result": receipt, "exit": 0} for receipt in
                  ("PASS Nat.add_comm | axioms: none", "PASS Nat.add_comm_extra | axioms: none",
                   "PASS Nat.add_comm | axioms: none\nFAIL other: kernel error")]
        entry = {"stdlib_source_sha256": "pinned", "probes": probes}
        result = BUILDER.inventory_row("nat-basic", {"names": ["Nat.add_comm"]}, {"nat-basic": entry})
        self.assertEqual(result["corpus_accepted_strict"], 1)

    def test_compatible_port_cannot_change_an_original_statement_after_repinning(self):
        group = copy.deepcopy(next(group for group in PROOF.core_groups() if "original" in group))
        rows, _, theorems = PROOF.check_files(group)
        left, right = (theorems[name] for name in group["theorems"][:2])
        left["type"], left["value"] = right["type"], right["value"]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "changed.ndjson"
            path.write_text("".join(json.dumps(row) + "\n" for row in rows))
            group.update(export_path=str(path), export_sha256=PROOF.digest(path.read_bytes()))
            with self.assertRaisesRegex(ValueError, "original statement"):
                PROOF.check_files(group)

    def test_bundled_proofs_reject_stale_exports_missing_licenses_and_unlisted_files(self):
        assets = load_script("proof_assets", PROOF.ROOT / "scripts/workbench-proof-assets.py")
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory)
            for relative, value in (("proof-libraries/stacks.ndjson", b"corrupt"),
                                    ("proof-libraries/LICENSE.lean4", None),
                                    ("proof-libraries/unlisted.ndjson", b"unexpected")):
                assets.stage(destination)
                assets.stage(destination, check=True)
                path = destination / relative
                path.unlink() if value is None else path.write_bytes(value)
                with self.assertRaisesRegex(ValueError, "missing|stale|inventory"):
                    assets.stage(destination, check=True)

    def test_proof_staging_refuses_a_symlink_without_touching_its_destination(self):
        assets = load_script("proof_assets", PROOF.ROOT / "scripts/workbench-proof-assets.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bundle, outside = root / "bundle", root / "outside"
            bundle.mkdir()
            outside.mkdir()
            sentinel = outside / "keep"
            sentinel.write_text("untouched")
            (bundle / "proof-libraries").symlink_to(outside, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, "symlink"):
                assets.stage(bundle)
            self.assertEqual(sentinel.read_text(), "untouched")

    def test_packaging_committed_proofs_does_not_require_a_lean_installation(self):
        assets = load_script("proof_assets", PROOF.ROOT / "scripts/workbench-proof-assets.py")
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory)
            with mock.patch.object(Path, "home", return_value=destination / "absent-home"):
                assets.stage(destination)
                assets.stage(destination, check=True)


class ClassicalChoiceTests(unittest.TestCase):
    """Trust policy only: these original Nat certificates remain unpromoted."""
    def setUp(self):
        path = PROOF.CORE / "exports/nat-lemmas.ndjson"
        self.assertEqual(PROOF.digest(path.read_bytes()), "20511ce02417e4be5e97b3c3ba688d94c841891409f4914cdfb3c0d699bc4b02")
        self.rows = [json.loads(line) for line in path.read_text().splitlines()]
        self.names, _ = PROOF.names_and_theorems(self.rows)
        self.expected = ["propext", "Classical.choice", "Quot.sound"]
        self.expressions = {row["ie"]: row for row in self.rows if "ie" in row}
        self.axiom = next(row["axiom"] for row in self.rows if "axiom" in row and self.names[row["axiom"]["name"]] == "Classical.choice")

    def check_basis(self):
        PROOF.check_axioms(self.rows, self.names, self.expected)

    def test_pinned_classical_choice_has_its_exact_standard_basis(self):
        self.check_basis()

    def test_choice_cannot_drop_its_nonempty_hypothesis(self):
        root = self.expressions[self.axiom["type"]]
        root["forallE"]["body"] = next(row["ie"] for row in self.rows if row.get("bvar") == 0)
        with self.assertRaisesRegex(ValueError, "axiom"):
            self.check_basis()

    def test_choice_cannot_change_its_universe_parameters(self):
        self.axiom["levelParams"] = []
        with self.assertRaisesRegex(ValueError, "axiom"):
            self.check_basis()

    def test_choice_cannot_be_unsafe(self):
        self.axiom["isUnsafe"] = True
        with self.assertRaisesRegex(ValueError, "axiom"):
            self.check_basis()

    def test_choice_nonempty_uses_the_same_universe(self):
        root = self.expressions[self.axiom["type"]]
        hypothesis = self.expressions[self.expressions[root["forallE"]["body"]]["forallE"]["type"]]
        self.expressions[hypothesis["app"]["fn"]]["const"]["us"] = []
        with self.assertRaisesRegex(ValueError, "axiom"):
            self.check_basis()

    def test_choice_nonempty_cannot_use_a_different_predicate(self):
        self.names[next(index for index, name in self.names.items() if name == "Nonempty")] = "False"
        with self.assertRaisesRegex(ValueError, "axiom"):
            self.check_basis()

    def test_choice_cannot_be_omitted_from_the_declared_ledger(self):
        self.expected.remove("Classical.choice")
        with self.assertRaisesRegex(ValueError, "ledger"):
            self.check_basis()


if __name__ == "__main__":
    unittest.main()
