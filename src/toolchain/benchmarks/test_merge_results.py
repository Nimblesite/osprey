import json
import os
import runpy
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path
from typing import Optional
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from merge_results import atomic_write, merge, valid_timing
from test_report import TIMING, HeadlineTests, RunProvenanceTests, UpdateReadmeTests  # the report suites run with this one in both modes
from report import load, present_langs


class LspFixtureTests(unittest.TestCase):
    """LSP timings must analyze a valid project under [ARITH-EFFECT]."""

    def test_synthetic_project_compiles_and_runs_with_an_explicit_arithmetic_policy(self):
        repo = Path(__file__).resolve().parent.parent.parent.parent
        compiler = os.environ.get("OSPREY_BIN", str(repo / "target/release/osprey"))
        generate = runpy.run_path(str(repo / "scripts/tests/benchmark-lsp.py"))["synthetic_project"]
        with tempfile.TemporaryDirectory() as directory:
            root = generate(Path(directory))
            self.assertEqual(len(list((root / "src").glob("*.ospml"))), 10)
            for argument, expected in [(41, "42\n"), (9223372036854775807, "-9223372036854775808\n")]:
                (root / "src/main.ospml").write_text(f'print (helper1 {argument})\n')
                result = subprocess.run([compiler, str(root), "--run", "--quiet"],
                                        cwd=repo, capture_output=True, text=True, timeout=30)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, expected)


class PublishedMeasurementsTests(unittest.TestCase):
    def test_recorded_timings_stay_above_the_recovered_baseline(self) -> None:
        # 24 cases × 11 measured columns recovered on 2026-10-11. Only raise this floor.
        out = Path(__file__).resolve().parent / "results"
        data = json.loads((out / "results.json").read_text())["cases"]
        measured = sum(cell.get("status") == "ok" and valid_timing(cell)
                       for langs in data.values() for cell in langs.values())
        self.assertGreaterEqual(measured, 264, "published benchmark timings have been erased")
        self.assertEqual(load(out), data, "regeneration must preserve the published measurements")


class MergeResultsTests(unittest.TestCase):
    def test_interrupted_atomic_write_keeps_the_original_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "results.json"
            path.write_text("original")
            with patch("merge_results.os.replace", side_effect=OSError("interrupted")):
                with self.assertRaisesRegex(OSError, "interrupted"):
                    atomic_write(path, "replacement")
            self.assertEqual(path.read_text(), "original")
            self.assertEqual(list(path.parent.iterdir()), [path])

    def fixture(self, root: Path, status: str = "ok") -> tuple[Path, Path]:
        destination, update = root / "published", root / "partial"
        for out in (destination, update):
            (out / "hf").mkdir(parents=True)
            (out / "raw.jsonl").write_text(json.dumps({
                "case": "fib", "lang": "osprey-gc", "status": "ok", "rss": 10,
            }) + "\n")
        (destination / "hf/fib.json").write_text(json.dumps({"results": [{
            "command": "osprey-gc", **TIMING,
        }]}))
        (update / "raw.jsonl").write_text(json.dumps({
            "case": "fib", "lang": "osprey-gc", "status": status, "rss": 20,
        }) + "\n")
        return destination, update

    def snapshot(self, root: Path) -> dict[str, bytes]:
        return {str(p.relative_to(root)): p.read_bytes() for p in root.rglob("*") if p.is_file()}

    def assert_merge_refused(self, destination: Path, update: Path) -> None:
        """A refused merge must leave every published file untouched."""
        before = self.snapshot(destination)
        with self.assertRaises(ValueError):
            merge(destination, update, {"osprey-gc"})
        self.assertEqual(self.snapshot(destination), before)

    def test_missing_or_invalid_timing_refuses_the_merge_before_any_write(self) -> None:
        for result in (None, {}, {"command": "osprey-gc", "mean": 0},
                       {"command": "osprey-gc", **TIMING, "mean": float("nan")}):
            with self.subTest(result=result), tempfile.TemporaryDirectory() as directory:
                destination, update = self.fixture(Path(directory))
                if result is not None:
                    (update / "hf/fib.json").write_text(json.dumps({"results": [result]}))
                self.assert_merge_refused(destination, update)

    def test_incompatible_skip_cannot_replace_a_published_measurement(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            destination, update = self.fixture(Path(directory), "wasm_incompatible")
            before = self.snapshot(destination)
            merge(destination, update, {"osprey-gc"})
            self.assertEqual(self.snapshot(destination), before)

    def test_malformed_later_case_does_not_partially_publish(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            destination, update = self.fixture(Path(directory))
            shutil.copy(destination / "hf/fib.json", update / "hf/fib.json")
            with (update / "raw.jsonl").open("a") as raw:
                raw.write('{"case":"sort","lang":"osprey-gc","status":"ok","rss":20}\n')
            (update / "hf/sort.json").write_text("{broken")
            self.assert_merge_refused(destination, update)

    def test_unmeasured_rss_keeps_the_last_peak_when_timing_is_replaced(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            destination, update = self.fixture(Path(directory))
            shutil.copy(destination / "hf/fib.json", update / "hf/fib.json")
            (update / "raw.jsonl").write_text('{"case":"fib","lang":"osprey-gc","status":"ok","rss":0}\n')
            merge(destination, update, {"osprey-gc"})
            self.assertEqual(json.loads((destination / "raw.jsonl").read_text())["rss"], 10)

    def test_failed_partial_run_does_not_modify_published_results(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            destination, update = self.fixture(Path(directory), "build_failed")
            before = self.snapshot(destination)
            with self.assertRaisesRegex(ValueError, "partial benchmark failed"):
                merge(destination, update, {"osprey-gc"})
            self.assertEqual(self.snapshot(destination), before)

    def add_baselines(self, destination: Path) -> None:
        with (destination / "raw.jsonl").open("a") as raw:
            for lang, rss in (("osprey-wasm", 0), ("rust", 20)):
                raw.write(json.dumps({"case": "fib", "lang": lang, "status": "ok", "rss": rss}) + "\n")
        path = destination / "hf/fib.json"
        data = json.loads(path.read_text())
        data["results"].extend({"command": lang, **TIMING, "mean": mean}
                               for lang, mean in (("osprey-wasm", 9.0), ("rust", 0.5)))
        path.write_text(json.dumps(data))

    def test_language_the_rerun_never_measured_keeps_its_published_row(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            destination, update = self.fixture(Path(directory))
            self.add_baselines(destination)
            (update / "hf/fib.json").write_text(json.dumps({"results": [
                {"command": "osprey-gc", **TIMING, "mean": 0.9},
            ]}))
            merge(destination, update, {"osprey-gc", "osprey-wasm"})
            rows = {r["lang"]: r for r in map(json.loads, (destination / "raw.jsonl").read_text().splitlines())}
            self.assertEqual({l: r["rss"] for l, r in rows.items()}, {"osprey-gc": 20, "osprey-wasm": 0, "rust": 20})
            means = {r["command"]: r["mean"]
                     for r in json.loads((destination / "hf/fib.json").read_text())["results"]}
            self.assertEqual(means, {"osprey-gc": 0.9, "osprey-wasm": 9.0, "rust": 0.5})


class RecordedTimingTests(unittest.TestCase):
    """The hyperfine exports are untracked, so results.json is the only durable
    record of a timing. A run that measures some cases must not erase the rest."""

    def published(self, out: Path, status: str) -> None:
        (out / "hf").mkdir()
        (out / "raw.jsonl").write_text(
            f'{{"case":"fib","lang":"osprey-gc","status":"{status}","rss":10}}\n'
            '{"case":"sort","lang":"osprey-gc","status":"ok","rss":20}\n'
        )
        cell = {"status": "ok", "rss": 10, **TIMING}
        (out / "results.json").write_text(
            json.dumps({"languages": ["osprey-gc"], "cases": {"fib": {"osprey-gc": cell}}})
        )

    def export(self, out: Path, case: str, mean: float) -> None:
        result = {"command": "osprey-gc", "mean": mean, "stddev": 0.2, "min": 1.8, "max": 2.4}
        (out / "hf" / f"{case}.json").write_text(json.dumps({"results": [result]}))

    def test_a_case_with_no_hyperfine_export_keeps_its_published_timing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            self.published(out, "ok")
            self.export(out, "sort", 2.0)

            data = load(out)

            self.assertEqual(data["fib"]["osprey-gc"], {"status": "ok", "rss": 10, **TIMING})
            self.assertEqual(data["sort"]["osprey-gc"]["mean"], 2.0)

    def test_a_fresh_export_replaces_the_published_timing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            self.published(out, "ok")
            self.export(out, "fib", 3.0)

            self.assertEqual(load(out)["fib"]["osprey-gc"]["mean"], 3.0)

    def test_a_failed_rerun_keeps_the_last_published_measurement(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            self.published(out, "wrong_output")

            self.assertEqual(load(out)["fib"]["osprey-gc"], {"status": "ok", "rss": 10, **TIMING})

    def test_missing_raw_rows_keep_entire_published_cells(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            self.published(out, "ok")
            (out / "raw.jsonl").write_text("")
            self.assertEqual(load(out)["fib"]["osprey-gc"], {"status": "ok", "rss": 10, **TIMING})

    def test_zero_rss_and_invalid_timing_cannot_erase_published_values(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            self.published(out, "ok")
            (out / "raw.jsonl").write_text('{"case":"fib","lang":"osprey-gc","status":"ok","rss":0}\n')
            for mean in (0, -1, float("nan"), float("inf"), None):
                self.export(out, "fib", mean)
                with self.subTest(mean=mean):
                    self.assertEqual(load(out)["fib"]["osprey-gc"], {"status": "ok", "rss": 10, **TIMING})

    def test_a_partial_timing_export_cannot_erase_published_values(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            self.published(out, "ok")
            (out / "hf/fib.json").write_text(json.dumps({"results": [{"command": "osprey-gc", "mean": 2}]}))
            self.assertEqual(load(out)["fib"]["osprey-gc"]["mean"], TIMING["mean"])


class HarnessPublicationTests(unittest.TestCase):
    def fixture(self, root: Path) -> Path:
        bench = root / "src" / "toolchain" / "benchmarks"
        bench.mkdir(parents=True)
        source = Path(__file__).resolve().parent
        for name in ("run.sh", "merge_results.py", "report.py"):
            shutil.copy(source / name, bench / name)
        (bench / "README.md").write_text(f"{UpdateReadmeTests.START}\n{UpdateReadmeTests.END}\n")
        (bench / "cases/fib").mkdir(parents=True)
        (bench / "cases/fib/fib.osp").write_text("print(42)\n")
        (bench / "cases/fib/expected.txt").write_text("42\n")
        (root / "target/release").mkdir(parents=True)
        (root / "src/toolchain/website/src/_includes").mkdir(parents=True)
        (bench / "results").mkdir()
        (bench / "results/raw.jsonl").write_text('{"case":"fib","lang":"rust","status":"ok","rss":10}\n')
        (bench / "results/results.json").write_text(json.dumps({"cases": {
            "fib": {"rust": {"status": "ok", "rss": 10, **TIMING}},
        }}))
        return bench

    @staticmethod
    def page(root: Path) -> str:
        return (root / "src/toolchain/website/src/_includes/benchmarks-tables.html").read_text()

    def executable(self, path: Path, text: str) -> None:
        path.write_text(text)
        path.chmod(0o755)

    def run_harness(self, bench: Path, extra: Optional[dict[str, str]] = None) -> subprocess.CompletedProcess:
        return subprocess.run(["zsh", str(bench / "run.sh")], capture_output=True, text=True,
                              env={**os.environ, "BENCH_NO_BUILD": "1", "BENCH_PARTIAL": "0",
                                   "BENCH_MEMRUNS": "1", **(extra or {})}, timeout=30)

    def test_full_run_with_missing_compiler_preserves_every_published_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            bench = self.fixture(Path(directory))
            before = MergeResultsTests().snapshot(bench / "results")
            result = self.run_harness(bench)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(MergeResultsTests().snapshot(bench / "results"), before)

    def compiler(self, root: Path) -> None:
        self.executable(root / "target/release/osprey",
                        '#!/bin/sh\nprintf "#!/bin/sh\\nprintf 42" > fib\nchmod +x fib\n')

    def test_failed_malformed_or_interrupted_timing_preserves_every_published_file(self) -> None:
        for command in ("exit 1", 'kill -TERM "$PPID"; exit 0',
                        'while [ "$1" != "--export-json" ]; do shift; done; shift; echo "{broken" > "$1"'):
            with self.subTest(command=command), tempfile.TemporaryDirectory() as directory:
              root = Path(directory)
              bench = self.fixture(root)
              self.compiler(root)
              tools = root / "tools"
              tools.mkdir()
              self.executable(tools / "hyperfine", f"#!/bin/sh\n{command}\n")
              before = MergeResultsTests().snapshot(bench / "results")
              result = self.run_harness(bench, {"PATH": f"{tools}:{os.environ['PATH']}"})
              self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
              self.assertEqual(MergeResultsTests().snapshot(bench / "results"), before)

    def timing_tools(self, root: Path) -> dict[str, str]:
        """A stand-in hyperfine that exports TIMING for every `-n` language."""
        tools = root / "tools"
        tools.mkdir()
        self.executable(tools / "hyperfine", '#!/usr/bin/env python3\nimport json, sys\n'
                        'args = sys.argv[1:]\npath = args[args.index("--export-json") + 1]\n'
                        'langs = [args[i+1] for i, arg in enumerate(args) if arg == "-n"]\n'
                        f'timing = {TIMING!r}\n'
                        'with open(path, "w") as out:\n'
                        ' json.dump({"results": [{"command": l, **timing} for l in langs]}, out)\n')
        return {"PATH": f"{tools}:{os.environ['PATH']}"}

    def test_successful_full_run_preserves_absent_language_timings(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bench = self.fixture(root)
            self.compiler(root)
            result = self.run_harness(bench, self.timing_tools(root))
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            cells = json.loads((bench / "results/results.json").read_text())["cases"]["fib"]
            self.assertEqual(cells["rust"], {"status": "ok", "rss": 10, **TIMING})
            self.assertEqual(cells["osprey-gc"]["mean"], 1.0)

    def test_every_osprey_column_names_its_memory_backend_and_none_is_unlabelled(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bench = self.fixture(root)
            log = root / "compiles.log"
            self.executable(root / "target/release/osprey",
                            f'#!/bin/sh\necho "$*" >> "{log}"\nprintf "#!/bin/sh\\nprintf 42" > fib\nchmod +x fib\n')
            result = self.run_harness(bench, {**self.timing_tools(root), "BENCH_PARTIAL": "1"})
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            compiles = log.read_text().splitlines()
            self.assertEqual(len(compiles), 2)
            self.assertEqual(sum("--memory=arc" in c for c in compiles), 1)
            self.assertEqual(sum("--memory=gc" in c for c in compiles), 1)
            published = json.loads((bench / "results/results.json").read_text())
            self.assertEqual(set(published["cases"]["fib"]), {"rust", "osprey-arc", "osprey-gc"})
            self.assertEqual(published["languages"], ["osprey-arc", "osprey-gc", "rust"])
            page = self.page(root)
            self.assertFalse((bench / "results/results.html").exists(), "the website page is the only report")
            self.assertNotIn(">Osprey</th>", page)
            self.assertIn(">Osprey (ARC)</th>", page)
            self.assertIn(">Osprey (GC)</th>", page)

    def test_a_recorded_backendless_cell_is_never_published_again(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            cell = {"status": "ok", "rss": 10, **TIMING}
            (out / "hf").mkdir()
            (out / "raw.jsonl").write_text('{"case":"fib","lang":"osprey","status":"ok","rss":10}\n')
            (out / "results.json").write_text(json.dumps({"cases": {"fib": {"osprey": cell, "rust": cell}}}))
            data = load(out)
            self.assertNotIn("osprey", data["fib"])
            self.assertEqual(present_langs(data), ["rust"])

    def codon_fixture(self, root: Path, value: int) -> tuple[Path, dict[str, str]]:
        """Benchmark fixture whose codon case runs `value`, with a codon that prints it."""
        bench = self.fixture(root)
        self.compiler(root)
        (bench / "cases/fib/fib.codon").write_text(f"print({value})\n")
        tools = self.timing_tools(root)
        self.executable(root / "tools/codon", '#!/bin/sh\nwhile [ "$1" != "-o" ]; do shift; done\n'
                        f'printf "#!/bin/sh\\nprintf {value}" > "$2"\nchmod +x "$2"\n')
        return bench, tools

    def test_selected_column_is_timed_and_every_other_column_keeps_its_record(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bench, tools = self.codon_fixture(root, 42)
            before = json.loads((bench / "results/results.json").read_text())["cases"]["fib"]["rust"]
            result = self.run_harness(bench, {**tools, "BENCH_LANGS": "codon"})
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("codon", result.stdout)
            self.assertNotIn("osprey-arc", result.stdout)
            published = json.loads((bench / "results/results.json").read_text())
            cells = published["cases"]["fib"]
            self.assertEqual(cells["rust"], before)
            self.assertEqual(cells["codon"]["mean"], TIMING["mean"])
            self.assertEqual(cells["codon"]["status"], "ok")
            self.assertEqual(set(cells), {"rust", "codon"})
            self.assertIn("Codon", self.page(root))
            self.assertIn(cells["codon"]["run"], published["runs"])
            self.assertNotIn("run", cells["rust"])

    def test_wrong_codon_output_publishes_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bench, tools = self.codon_fixture(root, 41)
            before = MergeResultsTests().snapshot(bench / "results")
            result = self.run_harness(bench, {**tools, "BENCH_LANGS": "codon"})
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("WRONG OUTPUT", result.stdout)
            self.assertEqual(MergeResultsTests().snapshot(bench / "results"), before)

    def test_osprey_only_run_never_republishes_an_older_runs_export(self) -> None:
        # `results/hf/` is untracked, so it outlives the results.json it was
        # merged into. A newer results.json (another machine, a merged branch)
        # must survive `make -C src/toolchain/benchmarks bench-osprey`, whether or not the run re-measures
        # the case whose stale export is lying around.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bench = self.fixture(root)
            self.compiler(root)
            results = bench / "results"
            recorded = {"status": "ok", "rss": 10, **TIMING}
            (results / "results.json").write_text(json.dumps({"cases": {
                "fib": {"rust": recorded}, "sort": {"rust": recorded},
            }}))
            stale = {"command": "rust", "mean": 5.0, "stddev": 0.5, "min": 4.5, "max": 5.5}
            (results / "hf").mkdir()
            for case in ("fib", "sort"):
                (results / "hf" / f"{case}.json").write_text(json.dumps({"results": [stale]}))
            result = self.run_harness(bench, {**self.timing_tools(root), "BENCH_PARTIAL": "1"})
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            cases = json.loads((results / "results.json").read_text())["cases"]
            self.assertEqual({case: cases[case]["rust"] for case in ("fib", "sort")},
                             {"fib": recorded, "sort": recorded})
            self.assertEqual(cases["fib"]["osprey-gc"]["mean"], 1.0)


if __name__ == "__main__":
    if "--tooling-only" in sys.argv:
        sys.argv.remove("--tooling-only")
        suites = [name for name, value in list(globals().items())
                  if isinstance(value, type) and issubclass(value, unittest.TestCase)
                  and value is not LspFixtureTests]
        unittest.main(defaultTest=suites)
    else:
        unittest.main()
