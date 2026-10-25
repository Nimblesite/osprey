#!/usr/bin/env python3
"""The app runner must reject empty tests and preserve exact output bytes."""
import runpy
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

RUNNER = runpy.run_path(str(Path(__file__).with_name("test-python-app-ports.py")))
PASS = "report\n1..1\nok 1 - app\n# tests=1 passed=1 failed=0 skipped=0\n"
EMPTY = "report\n1..0\n# tests=0 passed=0 failed=0 skipped=0\n"


class AppRunnerContracts(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.app = RUNNER["app_dir"](self.root, "classifier")
        self.app.mkdir(parents=True)

    def check(self, expected, actual=None, code=0, stderr="", memory="default"):
        (self.app / "expected_output.txt").write_bytes(expected.encode())
        result = subprocess.CompletedProcess([], code, expected if actual is None else actual, stderr)
        check_one = RUNNER["check_one"]
        with patch.dict(check_one.__globals__, run_app=lambda *_: result):
            return check_one(Path("compiler"), self.root, "classifier", memory, False)

    def test_empty_or_failed_assertions_cannot_become_a_golden(self):
        for text in (EMPTY, PASS.replace("passed=1 failed=0", "passed=0 failed=1")):
            with self.subTest(text=text):
                self.assertTrue(self.check(text), "A matching invalid golden must fail")

    def test_success_still_requires_exact_output_and_zero_exit(self):
        self.assertFalse(self.check(PASS))
        self.assertTrue(self.check(PASS, actual=PASS + "unexpected\n"))
        self.assertTrue(self.check(PASS, code=1))

    def test_arc_requires_one_zero_live_sentinel(self):
        self.assertFalse(self.check(PASS, stderr="[osp-arc] exit: 0 live objects\n", memory="arc"))
        for stderr in ("", "[osp-arc] exit: 1 live objects\n", "[osp-arc] exit: 0 live objects\n" * 2):
            with self.subTest(stderr=stderr):
                self.assertTrue(self.check(PASS, stderr=stderr, memory="arc"))

    def test_subprocess_keeps_crlf_bytes_for_exact_comparison(self):
        compiler = self.root / "compiler"
        compiler.write_text("#!/usr/bin/env python3\nimport os\nos.write(1, b'report\\r\\n')\n")
        compiler.chmod(0o755)
        result = RUNNER["run_app"](compiler, self.app, "default")
        self.assertEqual(result.stdout, "report\r\n")


if __name__ == "__main__":
    unittest.main()
