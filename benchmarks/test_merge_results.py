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


if __name__ == "__main__":
    unittest.main()
