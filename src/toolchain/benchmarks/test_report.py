"""report.py's tests: the headline figures, run provenance and the README update."""
import json
import math
import os
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from report import fmt_time, load, tables_fragment, present_langs, ratios, render, runs_table, summary, update_readme, value_table, wins


# One recorded hyperfine timing: mean, spread and range, in seconds.
TIMING = {"mean": 1.0, "stddev": 0.1, "min": 0.9, "max": 1.2}


class HeadlineTests(unittest.TestCase):
    """Wins count any Osprey backend; ratios read the default backend, GC."""

    @staticmethod
    def cell(mean: float) -> dict:
        return {"status": "ok", "rss": 10, "mean": mean}

    def data(self) -> dict:
        return {
            "arc_wins": {"osprey-arc": self.cell(1.0), "osprey-gc": self.cell(4.0), "rust": self.cell(2.0)},
            "gc_wins": {"osprey-arc": self.cell(9.0), "osprey-gc": self.cell(1.0), "rust": self.cell(2.0)},
            "rust_wins": {"osprey-arc": self.cell(3.0), "osprey-gc": self.cell(4.0), "rust": self.cell(2.0)},
            "tied": {"osprey-gc": self.cell(2.0), "rust": self.cell(2.0)},
            "wasm_only": {"osprey-wasm": self.cell(1.0), "rust": self.cell(2.0)},
        }

    def test_a_win_is_a_strictly_lowest_cell_in_any_osprey_column(self) -> None:
        data = self.data()
        self.assertEqual(wins(data, sorted(data), "mean"), ["arc_wins", "gc_wins", "wasm_only"])

    def test_ratios_divide_each_osprey_column_by_the_language(self) -> None:
        data = self.data()
        self.assertEqual(ratios(data, sorted(data), "osprey-gc", "rust", "mean"), [2.0, 0.5, 2.0, 1.0])
        self.assertEqual(ratios(data, sorted(data), "osprey-arc", "rust", "mean"), [0.5, 4.5, 1.5])

    def test_star_marks_each_osprey_win_and_the_summary_names_them(self) -> None:
        data = self.data()
        langs, cases = present_langs(data), sorted(data)
        table = value_table(data, langs, cases, "mean", fmt_time)
        self.assertEqual(table.count("★"), 3)
        self.assertEqual(table.count('class="num best"'), 2)
        text = summary(data, langs, cases)
        self.assertNotIn("bench-card", text)
        self.assertIn("is-compact", text, "the relative table sizes to its content")
        for header in ("Compared with", "Osprey backend", "Geometric mean", "Median", "Geometric SD", "Cases"):
            self.assertIn(f">{header}</th>", text)
        arc = [0.5, 4.5, 1.5]
        mean = math.exp(sum(map(math.log, arc)) / 3)
        sd = math.exp(math.sqrt(sum((math.log(x) - math.log(mean)) ** 2 for x in arc) / 3))
        self.assertIn(f'<td class="sub">Osprey (ARC)</td><td class="num">{mean:.2f}×</td><td class="num">1.50×</td>'
                      f'<td class="num">×{sd:.2f}</td><td class="num">3</td>', text)
        self.assertIn('<td rowspan="2">Rust</td>', text)
        self.assertNotIn("Osprey (wasm)</td>", text.split("Rust</td>")[1].split("</tr>")[0])
        self.assertNotIn("Lowest time", text)

    def test_the_relative_table_explains_its_ratios_under_the_heading(self) -> None:
        data = self.data()
        page = tables_fragment(data, present_langs(data), sorted(data), {})
        heading, rest = page.split("<h2>Osprey time relative to each language</h2>\n", 1)
        self.assertTrue(rest.startswith("<p>For each program, Osprey's mean time is divided by"))
        for fact in ("geometric mean", "median", "geometric standard deviation", "1.50×"):
            self.assertIn(fact, rest.split("</p>")[0])

    def test_time_cells_show_the_mean_its_deviation_and_a_tooltip_of_every_statistic(self) -> None:
        cell = {"status": "ok", "rss": 10, "mean": 0.1223, "stddev": 0.0012, "min": 0.12,
                "max": 0.13, "median": 0.1219, "run": "2026-11-06 · Apple M4 Max"}
        table = value_table({"fib": {"osprey-arc": cell}}, ["osprey-arc"], ["fib"], "mean", fmt_time)
        self.assertIn('122.3&nbsp;ms ★<span class="sd">±1.2</span>', table, "the star stays beside its time")
        for fact in ("mean 122.3 ms", "median 121.9 ms", "standard deviation 1.2 ms",
                     "min 120.0 ms", "max 130.0 ms", "2026-11-06 · Apple M4 Max"):
            self.assertIn(fact, table)


class RunProvenanceTests(unittest.TestCase):
    """Every published figure names the machine and toolchains that measured it."""

    def test_a_fresh_export_records_its_run_and_retained_cells_keep_theirs(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            old = {"status": "ok", "rss": 10, **TIMING, "run": "2026-10-05 · Old CPU"}
            (out / "hf").mkdir()
            (out / "raw.jsonl").write_text('{"case":"fib","lang":"osprey-gc","status":"ok","rss":12}\n')
            (out / "results.json").write_text(json.dumps({
                "runs": {"2026-10-05 · Old CPU": {"date": "2026-10-05", "cpu": "Old CPU", "os": "Linux", "tools": {}},
                         "2026-01-01 · Gone": {"date": "2026-01-01", "cpu": "Gone", "os": "Linux", "tools": {}}},
                "cases": {"fib": {"rust": old, "osprey-gc": dict(old)}}}))
            export = {"command": "osprey-gc", "mean": 2.0, "stddev": 0.1, "min": 1.9, "max": 2.2, "median": 1.95}
            (out / "hf/fib.json").write_text(json.dumps({"results": [export]}))
            with patch("report.bake_website_fragment", return_value=None), \
                    patch("report.update_readme", return_value=None):
                render(out)
            published = json.loads((out / "results.json").read_text())
            fresh = published["cases"]["fib"]["osprey-gc"]
            self.assertEqual(published["cases"]["fib"]["rust"], old)
            self.assertEqual(fresh["median"], 1.95)
            self.assertNotEqual(fresh["run"], old["run"])
            run = published["runs"][fresh["run"]]
            self.assertTrue(run["cpu"] and run["os"] and run["date"])
            self.assertIn("hyperfine", run["tools"])
            self.assertEqual(set(published["runs"]), {old["run"], fresh["run"]}, "unreferenced runs are pruned")
            self.assertFalse((out / "results.html").exists())

    def test_a_retimed_cell_drops_a_median_its_new_export_lacks(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            (out / "hf").mkdir()
            (out / "raw.jsonl").write_text('{"case":"fib","lang":"rust","status":"ok","rss":10}\n')
            cell = {"status": "ok", "rss": 10, **TIMING, "median": 9.0}
            (out / "results.json").write_text(json.dumps({"cases": {"fib": {"rust": cell}}}))
            (out / "hf/fib.json").write_text(json.dumps({"results": [{"command": "rust", **TIMING}]}))
            self.assertNotIn("median", load(out, "now")["fib"]["rust"])

    def test_the_runs_table_lists_each_run_with_its_columns_and_cases(self) -> None:
        runs = {"r1": {"date": "2026-10-03", "cpu": "AMD Ryzen 9 3900X", "os": "Linux (WSL2)",
                       "tools": {"Rust": "1.98.1", "GCC": "15.2.0"}, "note": "Pinned to logical CPU 20"}}
        ok = {"status": "ok", "rss": 1, "mean": 1.0, "run": "r1"}
        data = {"fib": {"rust": ok, "c": ok}, "tak": {"rust": ok, "c": {**ok, "run": "r2"}}}
        table = runs_table(runs, data, ["c", "rust"], ["fib", "tak"])
        for fact in ("2026-10-03", "AMD Ryzen 9 3900X", "Linux (WSL2)", "Rust 1.98.1", "GCC 15.2.0",
                     "Pinned to logical CPU 20", "Rust: every case", "C: fib"):
            self.assertIn(fact, table)


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
                "osprey-arc": {"rss": 3_031_040},
                "osprey-gc": {"rss": 999_999_999},
            }}
            self.assertEqual(update_readme(data, path), path)
            self.assertEqual(path.read_text(), (
                f"intro\n{self.START}\n"
                "> Current measured peaks: `--memory=arc` **3.03 MB** and `--memory=gc` **1 GB**.\n"
                f"{self.END}\noutro\n"
            ))

    def test_filtered_run_leaves_the_readme_untouched(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self.readme(Path(directory))
            before = path.read_text()
            self.assertIsNone(update_readme({"binarytrees": {"osprey-gc": {"rss": 1}}}, path))
            self.assertEqual(path.read_text(), before)

    def test_missing_markers_fail_loudly(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "README.md"
            path.write_text("no markers\n")
            with self.assertRaisesRegex(ValueError, "missing binarytrees result markers"):
                update_readme({}, path)
