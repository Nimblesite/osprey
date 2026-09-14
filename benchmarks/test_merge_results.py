import json
import tempfile
import unittest
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from merge_results import merge


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


if __name__ == "__main__":
    unittest.main()
