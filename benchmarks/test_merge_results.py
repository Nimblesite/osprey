import json
import os
import runpy
import subprocess
import tempfile
import unittest
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from merge_results import merge
from report import load, update_readme


class LspFixtureTests(unittest.TestCase):
    """LSP timings must analyze a valid project under [ARITH-EFFECT]."""

    def test_synthetic_project_compiles_and_runs_with_an_explicit_arithmetic_policy(self):
        repo = Path(__file__).resolve().parent.parent
        compiler = os.environ.get("OSPREY_BIN", str(repo / "target/release/osprey"))
        generate = runpy.run_path(str(repo / "scripts/benchmark-lsp.py"))["synthetic_project"]
        with tempfile.TemporaryDirectory() as directory:
            root = generate(Path(directory))
            self.assertEqual(len(list((root / "src").glob("*.ospml"))), 10)
            for argument, expected in [(41, "42\n"), (9223372036854775807, "-9223372036854775808\n")]:
                (root / "src/main.ospml").write_text(f'print (helper1 {argument})\n')
                result = subprocess.run([compiler, str(root), "--run", "--quiet"],
                                        cwd=repo, capture_output=True, text=True, timeout=30)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, expected)


class MergeResultsTests(unittest.TestCase):
    def test_failed_partial_run_does_not_modify_published_results(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            destination, update = root / "published", root / "partial"
            (destination / "hf").mkdir(parents=True)
            (update / "hf").mkdir(parents=True)

            published = (
                '{"case":"fib","lang":"osprey","status":"ok","rss":10}\n'
                '{"case":"fib","lang":"rust","status":"ok","rss":20}\n'
            )
            (destination / "raw.jsonl").write_text(published)
            (destination / "hf" / "fib.json").write_text(
                json.dumps({"results": [{"command": "osprey", "mean": 1.0}]})
            )
            (update / "raw.jsonl").write_text(
                '{"case":"fib","lang":"osprey","status":"build_failed","rss":0}\n'
            )

            with self.assertRaisesRegex(ValueError, "partial benchmark failed"):
                merge(destination, update, {"osprey"})

            self.assertEqual((destination / "raw.jsonl").read_text(), published)


    def test_language_the_rerun_never_measured_keeps_its_published_row(self) -> None:
        """An absent toolchain must not erase that language's recorded numbers.

        `osprey-wasm` is in the re-run's language list but writes no record when
        the wasm runtime archive is missing. Keying the merge on the language
        list alone deleted the published wasm row for every re-run case.
        """
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            destination, update = root / "published", root / "partial"
            (destination / "hf").mkdir(parents=True)
            (update / "hf").mkdir(parents=True)

            (destination / "raw.jsonl").write_text(
                '{"case":"fib","lang":"osprey","status":"ok","rss":10}\n'
                '{"case":"fib","lang":"osprey-wasm","status":"ok","rss":0}\n'
                '{"case":"fib","lang":"rust","status":"ok","rss":20}\n'
            )
            (destination / "hf" / "fib.json").write_text(json.dumps({"results": [
                {"command": "osprey", "mean": 1.0},
                {"command": "osprey-wasm", "mean": 9.0},
                {"command": "rust", "mean": 0.5},
            ]}))
            (update / "raw.jsonl").write_text(
                '{"case":"fib","lang":"osprey","status":"ok","rss":11}\n'
            )
            (update / "hf" / "fib.json").write_text(json.dumps({"results": [
                {"command": "osprey", "mean": 0.9},
            ]}))

            merge(destination, update, {"osprey", "osprey-wasm"})

            rows = {(r["case"], r["lang"]): r for r in
                    (json.loads(l) for l in (destination / "raw.jsonl").read_text().splitlines() if l)}
            self.assertEqual(rows[("fib", "osprey")]["rss"], 11)
            self.assertEqual(rows[("fib", "osprey-wasm")]["rss"], 0)
            self.assertEqual(rows[("fib", "rust")]["rss"], 20)
            means = {r["command"]: r["mean"]
                     for r in json.loads((destination / "hf" / "fib.json").read_text())["results"]}
            self.assertEqual(means, {"osprey": 0.9, "osprey-wasm": 9.0, "rust": 0.5})


class RecordedTimingTests(unittest.TestCase):
    """The hyperfine exports are untracked, so results.json is the only durable
    record of a timing. A run that measures some cases must not erase the rest."""

    TIMING = {"mean": 1.0, "stddev": 0.1, "min": 0.9, "max": 1.2}

    def published(self, out: Path, status: str) -> None:
        (out / "hf").mkdir()
        (out / "raw.jsonl").write_text(
            f'{{"case":"fib","lang":"osprey","status":"{status}","rss":10}}\n'
            '{"case":"sort","lang":"osprey","status":"ok","rss":20}\n'
        )
        cell = {"status": "ok", "rss": 10, **self.TIMING}
        (out / "results.json").write_text(
            json.dumps({"languages": ["osprey"], "cases": {"fib": {"osprey": cell}}})
        )

    def export(self, out: Path, case: str, mean: float) -> None:
        result = {"command": "osprey", "mean": mean, "stddev": 0.2, "min": 1.8, "max": 2.4}
        (out / "hf" / f"{case}.json").write_text(json.dumps({"results": [result]}))

    def test_a_case_with_no_hyperfine_export_keeps_its_published_timing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            self.published(out, "ok")
            self.export(out, "sort", 2.0)

            data = load(out)

            self.assertEqual(data["fib"]["osprey"], {"status": "ok", "rss": 10, **self.TIMING})
            self.assertEqual(data["sort"]["osprey"]["mean"], 2.0)

    def test_a_fresh_export_replaces_the_published_timing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            self.published(out, "ok")
            self.export(out, "fib", 3.0)

            self.assertEqual(load(out)["fib"]["osprey"]["mean"], 3.0)

    def test_a_cell_that_stopped_passing_has_no_timing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            self.published(out, "wrong_output")

            self.assertEqual(load(out)["fib"]["osprey"], {"status": "wrong_output", "rss": 10})


class UpdateReadmeTests(unittest.TestCase):
    """`update_readme` keeps every quoted binarytrees figure honest; pin it."""

    START = "> <!-- binarytrees-results:start -->"
    END = "> <!-- binarytrees-results:end -->"

    def readme(self, root: Path) -> Path:
        path = root / "README.md"
        path.write_text(f"intro\n{self.START}\n> stale\n{self.END}\noutro\n")
        return path

    def test_measured_peaks_replace_the_marked_line_with_gb_and_mb_units(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self.readme(Path(directory))
            data = {"binarytrees": {
                "osprey": {"rss": 1_896_349_696},
                "osprey-arc": {"rss": 3_031_040},
                "osprey-gc": {"rss": 999_999_999},
            }}
            self.assertEqual(update_readme(data, path), path)
            self.assertEqual(path.read_text(), (
                f"intro\n{self.START}\n"
                "> Current measured peaks: default **1.9 GB**, "
                "`--memory=arc` **3.03 MB**, and `--memory=gc` **1 GB**.\n"
                f"{self.END}\noutro\n"
            ))

    def test_filtered_run_leaves_the_readme_untouched(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self.readme(Path(directory))
            before = path.read_text()
            self.assertIsNone(update_readme({"binarytrees": {"osprey": {"rss": 1}}}, path))
            self.assertEqual(path.read_text(), before)

    def test_missing_markers_fail_loudly(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "README.md"
            path.write_text("no markers\n")
            with self.assertRaisesRegex(ValueError, "missing binarytrees result markers"):
                update_readme({}, path)


if __name__ == "__main__":
    unittest.main()
