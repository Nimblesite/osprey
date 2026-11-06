import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest

import todo


class TodoTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.path = Path(self.temporary.name) / "tasks.json"

    def tearDown(self):
        self.temporary.cleanup()

    def run_cli(self, *arguments):
        output, errors = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(errors):
            status = todo.main(["--file", str(self.path), *arguments])
        return status, output.getvalue(), errors.getvalue()

    def test_add_and_list(self):
        self.assertEqual(self.run_cli("add", "buy milk")[0], 0)
        status, output, errors = self.run_cli("list")
        self.assertEqual((status, output, errors), (0, "[ ] 1: buy milk\n", ""))

    def test_list_empty(self):
        self.assertEqual(self.run_cli("list"), (0, "", ""))

    def test_persists_tasks(self):
        self.run_cli("add", "first")
        self.run_cli("add", "second")
        tasks = json.loads(self.path.read_text(encoding="utf-8"))
        self.assertEqual([task["id"] for task in tasks], [1, 2])
        self.assertEqual(tasks[1]["text"], "second")


if __name__ == "__main__":
    unittest.main()
