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
from contextlib import nullcontext

sys.path.insert(0, str(Path(__file__).resolve().parent))
from merge_results import atomic_write, merge, valid_timing
from test_report import TIMING, HeadlineTests, RunProvenanceTests, UpdateReadmeTests  # the report suites run with this one in both modes
from report import load, present_langs

FIB_CELL = {"status": "ok", "rss": 10, **TIMING}
"""The durable published cell for the fib/osprey-gc measurement."""


def write_export(root: Path, case: str, result: dict) -> None:
    """Write the hyperfine export a rerun leaves behind for `case`."""
    (root / "hf" / f"{case}.json").write_text(json.dumps({"results": [result]}))


def snapshot(root: Path) -> dict[str, bytes]:
    """Every file under `root`, keyed by relative path, for byte-exact comparison."""
    return {str(p.relative_to(root)): p.read_bytes() for p in root.rglob("*") if p.is_file()}


def raw_row(case: str, lang: str, status: str = "ok", rss: int = 10) -> str:
    """One `raw.jsonl` line recording `case` as measured for `lang`."""
    return json.dumps({"case": case, "lang": lang, "status": status, "rss": rss})


def published(out: Path) -> dict:
    """The `results.json` document published under `out`."""
    return json.loads((out / "results.json").read_text())


def published_tree(out: Path, rows: list[str], cases: Optional[dict] = None,
                   languages: Optional[list[str]] = None) -> Path:
    """A published results directory: its `hf/` slot, `rows` of raw lines, and `cases`."""
    (out / "hf").mkdir(parents=True, exist_ok=True)
    (out / "raw.jsonl").write_text("".join(row + "\n" for row in rows))
    if cases is not None:
        head = {"languages": languages} if languages else {}
        (out / "results.json").write_text(json.dumps({**head, "cases": cases}))
    return out


class TempDirs:
    """Temporary directories that live until the test ends, so bodies stay flat."""

    def temp_dir(self) -> Path:
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        return Path(directory.name)


class LspFixtureTests(TempDirs, unittest.TestCase):
    """LSP timings must analyze a valid project under [ARITH-EFFECT]."""

    def test_synthetic_project_compiles_and_runs_with_an_explicit_arithmetic_policy(self):
        repo = Path(__file__).resolve().parent.parent.parent.parent
        compiler = os.environ.get("OSPREY_BIN", str(repo / "target/release/osprey"))
        generate = runpy.run_path(str(repo / "scripts/tests/benchmark-lsp.py"))["synthetic_project"]
        root = generate(self.temp_dir())
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
        data = published(out)["cases"]
        measured = sum(cell.get("status") == "ok" and valid_timing(cell)
                       for langs in data.values() for cell in langs.values())
        self.assertGreaterEqual(measured, 264, "published benchmark timings have been erased")
        self.assertEqual(load(out), data, "regeneration must preserve the published measurements")


class MergeResultsTests(TempDirs, unittest.TestCase):
    def test_interrupted_atomic_write_keeps_the_original_file(self) -> None:
        path = self.temp_dir() / "results.json"
        path.write_text("original")
        with patch("merge_results.os.replace", side_effect=OSError("interrupted")):
            with self.assertRaisesRegex(OSError, "interrupted"):
                atomic_write(path, "replacement")
        self.assertEqual(path.read_text(), "original")
        self.assertEqual(list(path.parent.iterdir()), [path])

    def fixture_pair(self, status: str = "ok", resumed: bool = False) -> tuple[Path, Path]:
        """A published/partial fixture pair in its own root, already resumed when asked."""
        root = self.temp_dir()
        destination, update = root / "published", root / "partial"
        for out, row_status, rss in ((destination, "ok", 10), (update, status, 20)):
            published_tree(out, [raw_row("fib", "osprey-gc", row_status, rss)])
        write_export(destination, "fib", {"command": "osprey-gc", **TIMING})
        if resumed:
            shutil.copy(destination / "hf/fib.json", update / "hf/fib.json")
        return destination, update

    def assert_merge_untouched(self, destination: Path, update: Path, reason: Optional[str] = None) -> None:
        """A merge refused for `reason` (or skipped, when None) leaves every published file untouched."""
        refuses = nullcontext() if reason is None else self.assertRaisesRegex(ValueError, reason)
        before = snapshot(destination)
        with refuses:
            merge(destination, update, {"osprey-gc"})
        self.assertEqual(snapshot(destination), before)

    def test_missing_or_invalid_timing_refuses_the_merge_before_any_write(self) -> None:
        for result in (None, {}, {"command": "osprey-gc", "mean": 0},
                       {"command": "osprey-gc", **TIMING, "mean": float("nan")}):
            with self.subTest(result=result):
                destination, update = self.fixture_pair()
                if result is not None:
                    write_export(update, "fib", result)
                self.assert_merge_untouched(destination, update, "")

    def test_incompatible_skip_cannot_replace_a_published_measurement(self) -> None:
        self.assert_merge_untouched(*self.fixture_pair("wasm_incompatible"))

    def test_malformed_later_case_does_not_partially_publish(self) -> None:
        destination, update = self.fixture_pair(resumed=True)
        with (update / "raw.jsonl").open("a") as raw:
            raw.write(raw_row("sort", "osprey-gc", rss=20) + "\n")
        (update / "hf/sort.json").write_text("{broken")
        self.assert_merge_untouched(destination, update, "")

    def test_unmeasured_rss_keeps_the_last_peak_when_timing_is_replaced(self) -> None:
        destination, update = self.fixture_pair(resumed=True)
        (update / "raw.jsonl").write_text(raw_row("fib", "osprey-gc", rss=0) + "\n")
        merge(destination, update, {"osprey-gc"})
        self.assertEqual(json.loads((destination / "raw.jsonl").read_text())["rss"], 10)

    def test_failed_partial_run_does_not_modify_published_results(self) -> None:
        self.assert_merge_untouched(*self.fixture_pair("build_failed"), "partial benchmark failed")

    def add_baselines(self, destination: Path) -> None:
        with (destination / "raw.jsonl").open("a") as raw:
            for lang, rss in (("osprey-wasm", 0), ("rust", 20)):
                raw.write(raw_row("fib", lang, rss=rss) + "\n")
        path = destination / "hf/fib.json"
        data = json.loads(path.read_text())
        data["results"].extend({"command": lang, **TIMING, "mean": mean}
                               for lang, mean in (("osprey-wasm", 9.0), ("rust", 0.5)))
        path.write_text(json.dumps(data))

    def test_language_the_rerun_never_measured_keeps_its_published_row(self) -> None:
        destination, update = self.fixture_pair()
        self.add_baselines(destination)
        write_export(update, "fib", {"command": "osprey-gc", **TIMING, "mean": 0.9})
        merge(destination, update, {"osprey-gc", "osprey-wasm"})
        rows = {r["lang"]: r for r in map(json.loads, (destination / "raw.jsonl").read_text().splitlines())}
        self.assertEqual({l: r["rss"] for l, r in rows.items()}, {"osprey-gc": 20, "osprey-wasm": 0, "rust": 20})
        means = {r["command"]: r["mean"] for r in json.loads((destination / "hf/fib.json").read_text())["results"]}
        self.assertEqual(means, {"osprey-gc": 0.9, "osprey-wasm": 9.0, "rust": 0.5})


class RecordedTimingTests(TempDirs, unittest.TestCase):
    """The hyperfine exports are untracked, so results.json is the only durable
    record of a timing. A run that measures some cases must not erase the rest."""

    def published_dir(self, status: str = "ok", rows: Optional[list[str]] = None) -> Path:
        """A published results directory whose raw rows are `rows`, or a fib/sort pair."""
        if rows is None:
            rows = [raw_row("fib", "osprey-gc", status), raw_row("sort", "osprey-gc", rss=20)]
        return published_tree(self.temp_dir(), rows, {"fib": {"osprey-gc": FIB_CELL}}, ["osprey-gc"])

    def export(self, out: Path, case: str, mean: float) -> None:
        write_export(out, case, {"command": "osprey-gc", "mean": mean,
                                 "stddev": 0.2, "min": 1.8, "max": 2.4})

    def assert_fib_keeps_its_published_cell(self, out: Path) -> None:
        """A rerun that measured nothing valid must not overwrite the durable cell."""
        self.assertEqual(load(out)["fib"]["osprey-gc"], FIB_CELL)

    def test_a_case_with_no_hyperfine_export_keeps_its_published_timing(self) -> None:
        out = self.published_dir()
        self.export(out, "sort", 2.0)
        self.assert_fib_keeps_its_published_cell(out)
        self.assertEqual(load(out)["sort"]["osprey-gc"]["mean"], 2.0)

    def test_a_fresh_export_replaces_the_published_timing(self) -> None:
        out = self.published_dir()
        self.export(out, "fib", 3.0)
        self.assertEqual(load(out)["fib"]["osprey-gc"]["mean"], 3.0)

    def test_a_failed_rerun_keeps_the_last_published_measurement(self) -> None:
        self.assert_fib_keeps_its_published_cell(self.published_dir("wrong_output"))

    def test_missing_raw_rows_keep_entire_published_cells(self) -> None:
        self.assert_fib_keeps_its_published_cell(self.published_dir(rows=[]))

    def test_zero_rss_and_invalid_timing_cannot_erase_published_values(self) -> None:
        out = self.published_dir(rows=[raw_row("fib", "osprey-gc", rss=0)])
        for mean in (0, -1, float("nan"), float("inf"), None):
            with self.subTest(mean=mean):
                self.export(out, "fib", mean)
                self.assert_fib_keeps_its_published_cell(out)

    def test_a_partial_timing_export_cannot_erase_published_values(self) -> None:
        out = self.published_dir()
        write_export(out, "fib", {"command": "osprey-gc", "mean": 2})
        self.assertEqual(load(out)["fib"]["osprey-gc"]["mean"], TIMING["mean"])


class HarnessPublicationTests(TempDirs, unittest.TestCase):
    def fixture(self, root: Path) -> Path:
        """A benchmark checkout whose one published fib/rust measurement is recorded."""
        bench = root / "src" / "toolchain" / "benchmarks"
        files = {
            "README.md": f"{UpdateReadmeTests.START}\n{UpdateReadmeTests.END}\n",
            "cases/fib/fib.osp": "print(42)\n",
            "cases/fib/expected.txt": "42\n",
        }
        for name, text in files.items():
            (bench / name).parent.mkdir(parents=True, exist_ok=True)
            (bench / name).write_text(text)
        published_tree(bench / "results", [raw_row("fib", "rust")], {"fib": {"rust": FIB_CELL}})
        for name in ("run.sh", "merge_results.py", "report.py"):
            shutil.copy(Path(__file__).resolve().parent / name, bench / name)
        (root / "target/release").mkdir(parents=True)
        (root / "src/toolchain/website/src/_includes").mkdir(parents=True)
        return bench

    def prepared(self, log: str = "") -> tuple[Path, Path]:
        """A fixture in its own root, with the compiler the harness expects logged to `log`."""
        root = self.temp_dir()
        bench = self.fixture(root)
        self.compiler(root, root / log if log else None)
        return root, bench

    @staticmethod
    def page(root: Path) -> str:
        return (root / "src/toolchain/website/src/_includes/benchmarks-tables.html").read_text()

    def executable(self, path: Path, text: str) -> None:
        path.write_text(text)
        path.chmod(0o755)

    def tools_dir(self, root: Path, name: str, text: str) -> dict[str, str]:
        """A sealed stand-in `name` in the fixture's `tools/` directory, as a PATH override."""
        tools = root / "tools"
        tools.mkdir(exist_ok=True)
        self.executable(tools / name, text)
        return {"PATH": f"{tools}:{os.environ['PATH']}"}

    def harness(self, bench: Path, extra: Optional[dict[str, str]] = None,
                ok: bool = True) -> subprocess.CompletedProcess:
        """Run the harness with `extra`, requiring it to exit 0 (`ok`) or to fail."""
        result = subprocess.run(["zsh", str(bench / "run.sh")], capture_output=True, text=True,
                                env={**os.environ, "BENCH_NO_BUILD": "1", "BENCH_PARTIAL": "0",
                                     "BENCH_MEMRUNS": "1", **(extra or {})}, timeout=30)
        self.assertEqual(result.returncode == 0, ok, f"exit {result.returncode}: {result.stdout + result.stderr}")
        return result

    def assert_nothing_published(self, bench: Path,
                                 extra: Optional[dict[str, str]] = None) -> subprocess.CompletedProcess:
        """A run that must not publish leaves every published file byte-identical."""
        before = snapshot(bench / "results")
        result = self.harness(bench, extra, ok=False)
        self.assertEqual(snapshot(bench / "results"), before)
        return result

    def run_partial_harness(self, root: Path, bench: Path) -> None:
        """A partial rerun that must succeed, measured by the stand-in hyperfine."""
        self.harness(bench, {**self.timing_tools(root), "BENCH_PARTIAL": "1"})

    def test_full_run_with_missing_compiler_preserves_every_published_file(self) -> None:
        self.assert_nothing_published(self.fixture(self.temp_dir()))

    def compiler(self, root: Path, log: Optional[Path] = None) -> None:
        """The stand-in compiler the harness builds fib with, logging its arguments to `log`."""
        echo = f'echo "$*" >> "{log}"\n' if log else ""
        self.executable(root / "target/release/osprey",
                        f'#!/bin/sh\n{echo}printf "#!/bin/sh\\nprintf 42" > fib\nchmod +x fib\n')

    def test_failed_malformed_or_interrupted_timing_preserves_every_published_file(self) -> None:
        for command in ("exit 1", 'kill -TERM "$PPID"; exit 0',
                        'while [ "$1" != "--export-json" ]; do shift; done; shift; echo "{broken" > "$1"'):
            with self.subTest(command=command):
                root, bench = self.prepared()
                self.assert_nothing_published(bench, self.tools_dir(root, "hyperfine", f"#!/bin/sh\n{command}\n"))

    def timing_tools(self, root: Path) -> dict[str, str]:
        """A stand-in hyperfine that exports TIMING for every `-n` language."""
        return self.tools_dir(root, "hyperfine", '#!/usr/bin/env python3\nimport json, sys\n'
                        'args = sys.argv[1:]\npath = args[args.index("--export-json") + 1]\n'
                        'langs = [args[i+1] for i, arg in enumerate(args) if arg == "-n"]\n'
                        f'timing = {TIMING!r}\n'
                        'with open(path, "w") as out:\n'
                        ' json.dump({"results": [{"command": l, **timing} for l in langs]}, out)\n')

    def test_successful_full_run_preserves_absent_language_timings(self) -> None:
        root, bench = self.prepared()
        result = self.harness(bench, self.timing_tools(root))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        cells = published(bench / "results")["cases"]["fib"]
        self.assertEqual(cells["rust"], FIB_CELL)
        self.assertEqual(cells["osprey-gc"]["mean"], 1.0)

    def test_every_osprey_column_names_its_memory_backend_and_none_is_unlabelled(self) -> None:
        root, bench = self.prepared("compiles.log")
        log = root / "compiles.log"
        self.run_partial_harness(root, bench)
        compiles = log.read_text().splitlines()
        self.assertEqual(len(compiles), 2)
        self.assertEqual(sum("--memory=arc" in c for c in compiles), 1)
        self.assertEqual(sum("--memory=gc" in c for c in compiles), 1)
        self.assertEqual(set(published(bench / "results")["cases"]["fib"]), {"rust", "osprey-arc", "osprey-gc"})
        self.assertEqual(published(bench / "results")["languages"], ["osprey-arc", "osprey-gc", "rust"])
        page = self.page(root)
        self.assertFalse((bench / "results/results.html").exists(), "the website page is the only report")
        self.assertNotIn(">Osprey</th>", page)
        self.assertIn(">Osprey (ARC)</th>", page)
        self.assertIn(">Osprey (GC)</th>", page)

    def test_a_recorded_backendless_cell_is_never_published_again(self) -> None:
        out = published_tree(self.temp_dir(), [raw_row("fib", "osprey")],
                             {"fib": {"osprey": FIB_CELL, "rust": FIB_CELL}})
        data = load(out)
        self.assertNotIn("osprey", data["fib"])
        self.assertEqual(present_langs(data), ["rust"])

    def codon_fixture(self, value: int) -> tuple[Path, Path, dict[str, str]]:
        """Benchmark fixture whose codon case runs `value`, with a codon that prints it."""
        root, bench = self.prepared()
        (bench / "cases/fib/fib.codon").write_text(f"print({value})\n")
        tools = self.timing_tools(root)
        self.tools_dir(root, "codon", '#!/bin/sh\nwhile [ "$1" != "-o" ]; do shift; done\n'
                       f'printf "#!/bin/sh\\nprintf {value}" > "$2"\nchmod +x "$2"\n')
        return root, bench, tools

    def test_selected_column_is_timed_and_every_other_column_keeps_its_record(self) -> None:
        root, bench, tools = self.codon_fixture(42)
        before = published(bench / "results")["cases"]["fib"]["rust"]
        result = self.harness(bench, {**tools, "BENCH_LANGS": "codon"})
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("codon", result.stdout)
        self.assertNotIn("osprey-arc", result.stdout)
        cells = published(bench / "results")["cases"]["fib"]
        self.assertEqual(cells["rust"], before)
        self.assertEqual(cells["codon"]["mean"], TIMING["mean"])
        self.assertEqual(cells["codon"]["status"], "ok")
        self.assertEqual(set(cells), {"rust", "codon"})
        self.assertIn("Codon", self.page(root))
        self.assertIn(cells["codon"]["run"],
                      json.loads((bench / "results/results.json").read_text())["runs"])
        self.assertNotIn("run", cells["rust"])

    def test_wrong_codon_output_publishes_nothing(self) -> None:
        _, bench, tools = self.codon_fixture(41)
        result = self.assert_nothing_published(bench, {**tools, "BENCH_LANGS": "codon"})
        self.assertIn("WRONG OUTPUT", result.stdout)

    def test_osprey_only_run_never_republishes_an_older_runs_export(self) -> None:
        # `results/hf/` is untracked, so it outlives the results.json it was
        # merged into. A newer results.json (another machine, a merged branch)
        # must survive `make -C src/toolchain/benchmarks bench-osprey`, whether or not the run re-measures
        # the case whose stale export is lying around.
        root, bench = self.prepared()
        results = bench / "results"
        (results / "results.json").write_text(
            json.dumps({"cases": {case: {"rust": FIB_CELL} for case in ("fib", "sort")}}))
        stale = {"command": "rust", "mean": 5.0, "stddev": 0.5, "min": 4.5, "max": 5.5}
        for case in ("fib", "sort"):
            write_export(results, case, stale)
        self.run_partial_harness(root, bench)
        cases = published(results)["cases"]
        self.assertEqual({case: cases[case]["rust"] for case in ("fib", "sort")},
                         {"fib": FIB_CELL, "sort": FIB_CELL})
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
