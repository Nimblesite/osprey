#!/usr/bin/env python3
"""Checks the `osprey hunt` corpus tooling without KLEE [SYMEX-CORPUS] [SYMEX-CLI]
(docs/specs/tooling/SymbolicExecution.md): scripts/hunt_expect.py, scripts/hunt_summary.py,
scripts/hunt_corpus.sh against a stand-in `osprey`, and that
compiler/runtime/klee/hunt_format_cases.inc is what scripts/oracles/hunt_format.py writes.
"""

import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


hunt_expect = load("hunt_expect", "scripts/hunt_expect.py")
hunt_summary = load("hunt_summary", "scripts/hunt_summary.py")
hunt_format = load("hunt_format", "scripts/oracles/hunt_format.py")


def finding(cls="memory", file="dir/main.osp", line=4, frame=None, test="test000001", replay="not-run"):
    return {"test": test, "class": cls, "file": file, "line": line, "runtimeFrame": frame, "replay": replay}


def report(findings=(), incomplete=(), program="main.osp", paths=3, completed=2):
    return {
        "program": program,
        "engine": "KLEE",
        "explored": {"paths": paths, "completed": completed},
        "findings": list(findings),
        "incomplete": list(incomplete),
    }


def run_main(module, argv):
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        code = module.main(argv)
    return code, out.getvalue()


class ExpectedFileTests(unittest.TestCase):
    """`.expected`: one `<class> <file>:<line>` per line, or `none`; comments and blanks ignored."""

    def parse(self, text):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "p.expected"
            path.write_text(text)
            return hunt_expect.expected_findings(path)

    def test_every_spelling_of_none_is_the_empty_set(self):
        for text in ["none", "none\n", "  none  \n", "# header\nnone\n", "none # nothing\n", "\n\nnone\n\n"]:
            self.assertEqual(self.parse(text), set(), repr(text))

    def test_findings_are_trimmed_deduplicated_and_unordered(self):
        expected = {"memory main.osp:4", "unmodelled process.osp:4"}
        for text in [
            "memory main.osp:4\nunmodelled process.osp:4\n",
            "unmodelled process.osp:4\nmemory main.osp:4\n",
            "  memory main.osp:4  \n# why\nunmodelled process.osp:4 # spawn\n\nmemory main.osp:4\n",
        ]:
            self.assertEqual(self.parse(text), expected, repr(text))

    def test_an_empty_file_is_not_none(self):
        self.assertEqual(self.parse(""), set())
        self.assertEqual(self.parse("none\nmemory a.osp:1\n"), {"none", "memory a.osp:1"})


class ActualFindingTests(unittest.TestCase):
    def test_a_located_finding_is_class_and_basename_line(self):
        for file in ["main.osp", "dir/main.osp", "/abs/dir/main.osp"]:
            self.assertEqual(hunt_expect.actual_findings(report([finding(file=file)])), {"memory main.osp:4"}, file)

    def test_an_unlocated_finding_falls_back_to_frame_then_test(self):
        self.assertEqual(
            hunt_expect.actual_findings(report([finding(file=None, frame="r.c:9 in f")])), {"memory r.c:9 in f"}
        )
        self.assertEqual(hunt_expect.actual_findings(report([finding(file=None, frame=None)])), {"memory test000001"})

    def test_the_replay_verdict_never_matters(self):
        for replay in ["not-run", "reproduced", "klee-only", "skipped"]:
            self.assertEqual(hunt_expect.actual_findings(report([finding(replay=replay)])), {"memory main.osp:4"})


class ExpectMainTests(unittest.TestCase):
    def check(self, findings, expected_text, raw=None):
        with tempfile.TemporaryDirectory() as tmp:
            report_path, expected_path = Path(tmp) / "r.json", Path(tmp) / "case.expected"
            report_path.write_text(raw if raw is not None else json.dumps(report(findings, completed=2 if findings else 3)))
            expected_path.write_text(expected_text)
            return run_main(hunt_expect, ["hunt_expect.py", str(report_path), str(expected_path)])

    def test_an_exact_match_passes_in_either_order(self):
        both = [finding(), finding("unmodelled", "process.osp", 4)]
        for found in [both, both[::-1]]:
            code, out = self.check(found, "unmodelled process.osp:4\nmemory main.osp:4\n")
            self.assertEqual(code, 0, out)
            self.assertIn("case: ok (2 finding(s), 3 paths)", out)

    def test_none_passes_only_with_no_findings(self):
        self.assertEqual(self.check([], "none\n")[0], 0)
        code, out = self.check([finding()], "none\n")
        self.assertEqual(code, 1)
        self.assertIn("case: unexpected  memory main.osp:4", out)

    def test_unexplored_incomplete_or_malformed_reports_fail(self):
        for body in [report(paths=0, completed=0), report(completed=0), report(paths=3, completed=2),
                     report(incomplete=[{"reason": "solver timeout"}]),
                     {}, [], report(paths=-1), report(paths="3")]:
            with self.subTest(body=body):
                code, out = self.check([], "none", raw=json.dumps(body))
                self.assertEqual(code, 1, out)

    def test_recorded_process_failure_is_not_a_clean_pass(self):
        with tempfile.TemporaryDirectory() as tmp:
            r, e = Path(tmp) / "r.json", Path(tmp) / "c.expected"
            r.write_text(json.dumps(report()))
            e.write_text("none")
            code, out = run_main(hunt_expect, ["hunt_expect.py", str(r), str(e), "2"])
            self.assertEqual(code, 1, out)

    def test_missing_extra_and_wrong_line_or_class_all_fail(self):
        cases = [
            ([], "memory main.osp:4", "missing  memory main.osp:4"),
            ([finding(line=5)], "memory main.osp:4", "unexpected  memory main.osp:5"),
            ([finding("abort")], "memory main.osp:4", "unexpected  abort main.osp:4"),
            ([finding(), finding(line=9)], "memory main.osp:4", "unexpected  memory main.osp:9"),
        ]
        for found, expected, message in cases:
            code, out = self.check(found, expected)
            self.assertEqual(code, 1, out)
            self.assertIn(message, out)

    def test_an_unreadable_report_fails_naming_the_case(self):
        for raw in ["", "not json", "{"]:
            code, out = self.check([], "none", raw=raw)
            self.assertEqual(code, 1)
            self.assertIn("case: no report", out)
        code, out = run_main(hunt_expect, ["hunt_expect.py", "/nonexistent/r.json", str(ROOT / "tests/hunt/clean.expected")])
        self.assertEqual((code, "clean: no report" in out), (1, True))


class SummaryTests(unittest.TestCase):
    """A corpus run directory: code/<slug>, json/<slug>.json, log/<slug>.log."""

    def corpus(self, tmp, entries):
        out = Path(tmp)
        for name in ["code", "json", "log"]:
            (out / name).mkdir()
        for slug, code, body, log in entries:
            (out / "code" / slug).write_text(f"{code}\n")
            (out / "json" / f"{slug}.json").write_text(body)
            (out / "log" / f"{slug}.log").write_text(log)
        return out

    def summary(self, entries, *flags):
        with tempfile.TemporaryDirectory() as tmp:
            out = self.corpus(tmp, entries)
            return run_main(hunt_summary, ["hunt_summary.py", str(out), *flags])[1]

    def entries(self):
        located = json.dumps(report([finding(replay="reproduced")], program="tests/a.osp"))
        unlocated = json.dumps(report([finding("unmodelled", None, None, "r.c:1 in fork")], program="tests/b.osp"))
        clean = json.dumps(report(program="tests/c.osp", incomplete=[{"test": "t", "reason": "max-time"}]))
        return [
            ("tests_a.osp", 1, located, ""),
            ("tests_b.osp", 0, unlocated, ""),
            ("tests_c.osp", 0, clean, ""),
            ("tests_d.osp", 2, "", "osprey hunt: klee was not found | install it\nsecond line"),
            ("tests_e.osp", 1, "", "type error"),
        ]

    def test_the_header_counts_programs_findings_and_failures(self):
        text = self.summary(self.entries())
        self.assertIn("# Hunt corpus: 5 programs, 2 with findings, 2 did not hunt", text)
        self.assertIn("| program | exit | paths/complete | incomplete | findings | note |", text)

    def test_each_row_names_place_verdict_and_first_log_line(self):
        text = self.summary(self.entries())
        self.assertIn("| tests/a.osp | 1 | 3/2 | 0 | memory dir/main.osp:4 (reproduced) |  |", text)
        self.assertIn("| tests/b.osp | 0 | 3/2 | 0 | unmodelled r.c:1 in fork (not-run) |  |", text)
        self.assertIn("| tests/c.osp | 0 | 3/2 | 1 |  |  |", text)
        self.assertIn("| tests_d.osp | 2 |  |  |  | osprey hunt: klee was not found \\| install it |", text)
        self.assertNotIn("second line", text)

    def test_findings_only_drops_clean_rows_but_keeps_failures(self):
        text = self.summary(self.entries(), "--findings-only")
        self.assertIn("tests/a.osp", text)
        self.assertIn("tests/b.osp", text)
        self.assertNotIn("tests/c.osp", text)
        self.assertIn("tests_d.osp", text)
        self.assertIn("tests_e.osp", text)

    def test_a_long_note_is_cut_to_120_characters(self):
        text = self.summary([("x", 2, "", "e" * 500)])
        self.assertIn("| " + "e" * 120 + " |", text)
        self.assertNotIn("e" * 121, text)


@unittest.skipIf(os.name == "nt", "the corpus sweep is a bash script for macOS and Linux")
class CorpusScriptTests(unittest.TestCase):
    """scripts/hunt_corpus.sh against a stand-in `osprey` that records its arguments."""

    def sweep(self, *arguments):
        with tempfile.TemporaryDirectory() as tmp:
            fake = Path(tmp) / "osprey"
            fake.write_text('#!/bin/sh\necho "$*" >> "$(dirname "$0")/calls"\necho \'{"findings": []}\'\nexit 3\n')
            fake.chmod(0o755)
            out = Path(tmp) / "out"
            done = subprocess.run(
                ["bash", str(ROOT / "scripts/hunt_corpus.sh"), "--osprey", str(fake), "--out", str(out), "--jobs", "2", *arguments],
                capture_output=True,
                text=True,
                check=False,
            )
            calls_file = Path(tmp) / "calls"
            calls = sorted(calls_file.read_text().splitlines()) if calls_file.exists() else []
            codes = {p.name: p.read_text().strip() for p in (out / "code").iterdir()} if (out / "code").exists() else {}
            return done, calls, codes

    def test_help_prints_the_synopsis(self):
        done = subprocess.run(["bash", str(ROOT / "scripts/hunt_corpus.sh"), "--help"], capture_output=True, text=True, check=False)
        self.assertEqual(done.returncode, 0)
        self.assertIn("scripts/hunt_corpus.sh [--all]", done.stdout)

    def test_each_program_is_hunted_once_with_json_quiet_and_the_budget(self):
        done, calls, codes = self.sweep("--max-time", "7", "tests/hunt/clean.osp", "tests/hunt/process.osp")
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(
            calls,
            [
                "hunt tests/hunt/clean.osp --json --quiet --max-time 7",
                "hunt tests/hunt/process.osp --json --quiet --max-time 7",
            ],
        )
        self.assertEqual(codes, {"tests_hunt_clean.osp": "3", "tests_hunt_process.osp": "3"}, "exit codes are recorded")

    def test_the_skip_list_is_honoured_unless_all_is_given(self):
        skipped = "examples/db_postgres/pg_smoke.osp"
        listed = (ROOT / "tests/HUNT_UNPORTABLE.txt").read_text()
        self.assertIn(f"{skipped} ", listed)
        _, calls, _ = self.sweep(skipped, "tests/hunt/clean.osp")
        self.assertEqual(calls, ["hunt tests/hunt/clean.osp --json --quiet --max-time 20"])
        _, calls, _ = self.sweep("--all", skipped, "tests/hunt/clean.osp")
        self.assertEqual(len(calls), 2)
        self.assertIn(f"hunt {skipped} --json --quiet --max-time 20", calls)

    def test_a_project_directory_is_one_program(self):
        project = "examples/gpt2/generate"
        self.assertTrue((ROOT / project / "osprey.toml").exists() or (ROOT / project).is_dir())
        _, _, codes = self.sweep("--all", project)
        self.assertEqual(list(codes), ["examples_gpt2_generate.project"])


class FormatOracleTests(unittest.TestCase):
    """The committed float matrix is exactly what the oracle writes today."""

    def test_the_committed_cases_match_the_oracle(self):
        committed = (ROOT / "compiler/runtime/klee/hunt_format_cases.inc").read_text().replace("\r\n", "\n")
        with tempfile.TemporaryDirectory() as tmp:
            original = hunt_format.OUT
            hunt_format.OUT = Path(tmp) / "cases.inc"
            try:
                hunt_format.main()
                fresh = hunt_format.OUT.read_text().replace("\r\n", "\n")
            finally:
                hunt_format.OUT = original
        self.assertEqual(committed, fresh, "run python3 scripts/oracles/hunt_format.py")

    def test_every_value_meets_every_format(self):
        rows = [line for line in (ROOT / "compiler/runtime/klee/hunt_format_cases.inc").read_text().splitlines() if line.startswith("{")]
        self.assertEqual(len(rows), len(hunt_format.VALUES) * len(hunt_format.FORMATS))
        self.assertEqual(len(set(hunt_format.FORMATS)), len(hunt_format.FORMATS))
        self.assertEqual(len(set(hunt_format.VALUES)), len(hunt_format.VALUES))

    def test_the_c_literal_escapes_quotes_and_backslashes(self):
        self.assertEqual(hunt_format.c_string('a"b\\c'), '"a\\"b\\\\c"')
        self.assertEqual(hunt_format.c_string("%-12.4f|"), '"%-12.4f|"')


if __name__ == "__main__":
    unittest.main()
