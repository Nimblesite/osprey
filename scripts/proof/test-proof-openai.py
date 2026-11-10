#!/usr/bin/env python3
"""Trust-ledger regressions against the actual recorded proof corpus."""
import copy
import importlib.util
import json
import re
from contextlib import ExitStack, contextmanager
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

def load_script(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def windows_checkout_bytes(relative):
    """Bytes a Windows checkout with core.autocrlf=true writes for a tracked path."""
    return subprocess.run(["git", "-C", str(PROOF.ROOT), "-c", "core.autocrlf=true",
                           "cat-file", "--filters", f"HEAD:{relative}"], capture_output=True, check=True).stdout


PROOF = load_script("proof_openai", Path(__file__).with_name("proof-openai.py"))
BUILDER = load_script("core_builder", PROOF.CORE / "tools/build_corpus.py")


def verify_stubbed(group, checker, **stubs):
    """verify_group against a stubbed toolchain; `stubs` maps a PROOF helper to its mock kwargs."""
    with ExitStack() as stack, mock.patch("builtins.print"):
        for attribute, options in {"rejected": {}, **stubs}.items():
            stack.enter_context(mock.patch.object(PROOF, attribute, **options))
        PROOF.verify_group(checker, group, "gc")


@contextmanager
def staging_workspace():
    assets = load_script("proof_assets", PROOF.ROOT / "scripts/proof/workbench-proof-assets.py")
    with tempfile.TemporaryDirectory() as directory:
        yield assets, Path(directory)


class CorpusTests(unittest.TestCase):
    def test_name_reference_refuses_values_the_kernel_json_cannot_represent(self):
        for value in ({"i": -1}, {"i": 2 ** 63}, {"str": "a\0b"}):
            with self.subTest(value=value), self.assertRaises(ValueError):
                PROOF.name_component(value)

    def test_group_receipts_keep_ambiguous_axiom_names_distinct(self):
        group = {"theorems": ["t"]}
        for names, receipt in ((["none"], '"none"'), (["Ax, Other"], '"Ax, Other"'), (['"quoted"'], json.dumps('"quoted"'))):
            with self.subTest(names=names):
                verify_stubbed(group, [], check_files={"return_value": ([], {}, {"t": {}})},
                               export_path={"return_value": Path("unused")}, expected_axioms={"return_value": names},
                               check={"return_value": f"PASS t | axioms: {receipt}"})

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
        self.assertEqual(names, {0: "", 1: "A", 2: "A.B", 3: '"A.B"', 4: "5", 5: '"5"', 6: '""', 7: json.dumps('"A.B"'), 8: '"A.B".C'})
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

    def assert_embedded_twins(self, groups, tests_path):
        tests = (PROOF.ROOT / tests_path).read_text()
        for group in groups:
            PROOF.check_files(group)
            found = re.search(r"^    (?:export )?" + re.escape(group["embedded"]) + r" \(\) = (.+)$", tests, re.MULTILINE)
            self.assertIsNotNone(found, group["id"])
            self.assertEqual(json.loads(found.group(1)), PROOF.export_path(group).read_text())

    def test_workbench_custom_operator_fixtures_match_pinned_lean_sources(self):
        controls = json.loads((PROOF.ROOT / "src/osprey/examples/proof/kernel/exports/notation_cases.json").read_text())
        self.assertEqual(controls["toolchain"], "leanprover/lean4:v4.25.0")
        self.assert_embedded_twins(controls["groups"], "src/osprey/workbench/tests/imported.ospml")

    def test_kernel_export_fixture_twin_is_byte_identical_to_the_pinned_export(self):
        group = json.loads((PROOF.ROOT / "src/osprey/examples/proof/kernel/exports/recursor_cases.json").read_text())
        self.assert_embedded_twins([group], "src/osprey/examples/proof/kernel/tests/export.ospml")

    def test_number_theory_port_preserves_five_original_statements(self):
        groups = {group["id"]: group for group in self.groups}
        self.assertIn("five-color-orientation", groups)
        group = groups["five-color-orientation"]
        rows, _, _ = PROOF.check_files(group)
        self.assertEqual(len(group["theorems"]), 5)
        self.assertEqual(PROOF.statement_shapes(rows, group["theorems"]),
                         PROOF.statement_shapes(PROOF.export_rows(group["original"]), group["theorems"]))
        self.assertNotEqual(group["export_sha256"], group["original"]["export_sha256"])
        self.assertEqual(group["lean_project"], "src/osprey/examples/proof/openai-number-theory/lean")
        self.assertEqual(set(group["theorem_axioms"]), set(group["theorems"]))
        self.assertTrue(all(basis == ["propext"] for basis in group["theorem_axioms"].values()))

    def test_fresh_lean_replay_builds_the_existing_port_project(self):
        group = {**self.groups[0], "lean_project": "src/osprey/examples/proof/openai-number-theory/lean"}
        project = PROOF.ROOT / group["lean_project"]
        recorded = PROOF.export_path(group).read_text()
        with mock.patch.object(PROOF, "run", return_value=recorded) as runner, \
             mock.patch.object(PROOF, "verify_dependencies") as dependencies:
            PROOF.verify_lean(group)
        self.assertEqual(runner.call_args_list[0].kwargs["cwd"], project)
        for call in runner.call_args_list[1:]:
            self.assertEqual(call.kwargs["cwd"], PROOF.TOOLS)
            self.assertEqual(call.kwargs["env"]["LEAN_PATH"].split(PROOF.os.pathsep)[0], str(project / ".lake/build/lib/lean"))
        self.assertEqual(dependencies.call_args.args[1]["LEAN_PATH"].split(PROOF.os.pathsep)[0], str(project / ".lake/build/lib/lean"))

    def test_root_axioms_are_distinct_from_the_export_ledger(self):
        group = self.groups[0]
        def checked(command, path, name):
            names = ", ".join(group["theorem_axioms"][name]) or "none"
            return f"PASS {name} | axioms: {names}"
        verify_stubbed(group, ["checker"], check={"side_effect": checked})

    def rejected_wrong_statement(self, changer):
        self.rejected_axiom(changer("type", next(row["ie"] for row in self.rows if row.get("sort") == 0)))

    def test_axiom_cannot_be_given_the_wrong_statement(self):
        self.rejected_wrong_statement(self.changed_axiom)

    def test_unsafe_axiom_is_not_trusted(self):
        self.rejected_axiom(self.changed_axiom("isUnsafe", True))

    def changed_quotient_axiom(self, key, value):
        return self.changed_axiom(key, value, "Quot.sound")

    def test_quotient_soundness_cannot_be_replaced_by_an_arbitrary_statement(self):
        self.rejected_wrong_statement(self.changed_quotient_axiom)

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

    def mutated_theorem(self, proof):
        name = self.groups[0]["theorems"][0]
        rows = PROOF.mutation(self.rows, self.theorems[name], proof)
        names, theorems = PROOF.names_and_theorems(rows)
        return self.theorems[name], names, theorems[name], {row["ie"]: row for row in rows if "ie" in row}

    def test_corruption_retains_the_theorem_statement(self):
        target, _, changed, _ = self.mutated_theorem(True)
        self.assertEqual(changed["type"], target["type"])
        self.assertEqual(changed["value"], target["type"])

    def test_false_statement_retains_the_actual_proof(self):
        target, names, changed, expressions = self.mutated_theorem(False)
        self.assertEqual(changed["value"], target["value"])
        self.assertEqual(PROOF.expression_shape(changed["type"], expressions, names), ["forallE", ["sort", 0], ["bvar", 0]])

    def test_core_inventory_preserves_all_selected_and_blocked_roots(self):
        groups = PROOF.core_groups()
        self.assertEqual(sum(len(group["theorems"]) for group in groups), 175)
        self.assertEqual(len({name for group in groups for name in group["theorems"]}), 175)
        self.assertIn("Prod.map_id", {name for group in groups for name in group["theorems"]})

    def test_core_exports_use_the_same_structural_axiom_guard(self):
        self.assert_exports(PROOF.core_groups())

    def rejects_core_groups(self, reason, corrupt):
        corpus, frontier = PROOF.core_inventory()
        broken = copy.deepcopy(frontier)
        corrupt(corpus, broken)
        with self.assertRaisesRegex(ValueError, reason):
            PROOF.core_groups(corpus, broken)

    def test_core_frontier_cannot_silently_drop_a_selected_root(self):
        self.rejects_core_groups("partition", lambda corpus, frontier: frontier["groups"][0]["blocked"].pop())

    def test_core_frontier_cannot_promote_a_blocked_root_without_an_export(self):
        def promote(corpus, frontier):
            group = next(group for group in frontier["groups"] if group["blocked"])
            group["accepted"].append(group["blocked"].pop())
        self.rejects_core_groups("consolidation", promote)

    def test_core_toolchain_cannot_disagree_with_the_pinned_source_root(self):
        self.rejects_core_groups("toolchain", lambda corpus, frontier: corpus.update(toolchain="leanprover/lean4:unknown"))

    def test_core_unknown_frontier_group_cannot_be_ignored(self):
        def ignore_group(corpus, frontier):
            extra = {**copy.deepcopy(frontier["groups"][0]), "id": "unknown-group"}
            frontier["groups"].append(extra)
        self.rejects_core_groups("group", ignore_group)

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
        self.assertEqual(command, ["node", str(PROOF.ROOT / "scripts/proof/proof-batch-wasm.mjs"), str(binary)])
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
        with staging_workspace() as (assets, destination):
            for relative, value in (("proof-libraries/stacks.ndjson", b"corrupt"),
                                    ("proof-libraries/LICENSE.lean4", None),
                                    ("proof-libraries/unlisted.ndjson", b"unexpected")):
                assets.stage(destination)
                assets.stage(destination, check=True)
                path = destination / relative
                path.unlink() if value is None else path.write_bytes(value)
                with self.assertRaisesRegex(ValueError, "missing|stale|inventory"):
                    assets.stage(destination, check=True)

    def test_windows_autocrlf_checkout_preserves_every_pinned_proof_export(self):
        assets = load_script("proof_assets", PROOF.ROOT / "scripts/proof/workbench-proof-assets.py")
        groups = assets.groups()
        manifest = json.loads(assets.MANIFEST.read_text())["libraries"]
        self.assertEqual({entry["path"] for entry in manifest}, {f"proof-libraries/{group['id']}.ndjson" for group in groups})
        for group in groups:
            relative = PROOF.export_path(group).relative_to(PROOF.ROOT).as_posix()
            checked_out = windows_checkout_bytes(relative)
            self.assertEqual(checked_out.count(b"\r\n"), 0,
                             f"{relative}: a Windows autocrlf checkout rewrites pinned export bytes as CRLF")
            self.assertEqual(PROOF.digest(checked_out), group["export_sha256"], relative)

    def test_proof_staging_refuses_a_symlink_without_touching_its_destination(self):
        with staging_workspace() as (assets, root):
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
        with staging_workspace() as (assets, destination):
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

    def rejects_basis(self, reason):
        with self.assertRaisesRegex(ValueError, reason):
            self.check_basis()

    def test_actual_choice_oracle_and_embedded_twin_remain_pinned(self):
        base = PROOF.ROOT / "src/osprey/examples/proof/kernel/exports/choice_cases"
        manifest = json.loads(base.with_suffix(".json").read_text())
        source = base.with_suffix(".ndjson").read_text()
        self.assertEqual(PROOF.digest(source.encode()), manifest["export_sha256"])
        fixtures = (PROOF.ROOT / "src/osprey/examples/proof/kernel/src/fixtures.ospml").read_text()
        embedded = re.search(r"^    export choiceCases \(\) = (.+)$", fixtures, re.MULTILINE)
        self.assertIsNotNone(embedded)
        self.assertEqual(json.loads(embedded.group(1)), source)
        rows = [json.loads(line) for line in source.splitlines()]
        names, _ = PROOF.names_and_theorems(rows)
        PROOF.check_axioms(rows, names, ["Classical.choice"])

    def test_pinned_classical_choice_has_its_exact_standard_basis(self):
        self.check_basis()

    def test_choice_cannot_drop_its_nonempty_hypothesis(self):
        root = self.expressions[self.axiom["type"]]
        root["forallE"]["body"] = next(row["ie"] for row in self.rows if row.get("bvar") == 0)
        self.rejects_basis("axiom")

    def test_choice_cannot_change_its_universe_parameters(self):
        self.axiom["levelParams"] = []
        self.rejects_basis("axiom")

    def test_choice_cannot_be_unsafe(self):
        self.axiom["isUnsafe"] = True
        self.rejects_basis("axiom")

    def test_choice_nonempty_uses_the_same_universe(self):
        root = self.expressions[self.axiom["type"]]
        hypothesis = self.expressions[self.expressions[root["forallE"]["body"]]["forallE"]["type"]]
        self.expressions[hypothesis["app"]["fn"]]["const"]["us"] = []
        self.rejects_basis("axiom")

    def test_choice_nonempty_cannot_use_a_different_predicate(self):
        self.names[next(index for index, name in self.names.items() if name == "Nonempty")] = "False"
        self.rejects_basis("axiom")

    def test_choice_cannot_be_omitted_from_the_declared_ledger(self):
        self.expected.remove("Classical.choice")
        self.rejects_basis("ledger")


if __name__ == "__main__":
    unittest.main()
