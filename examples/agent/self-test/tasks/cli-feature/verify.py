import ast
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


def fail(message):
    raise SystemExit(f"cli-feature: {message}")


def test_ids(suite):
    for test in suite:
        if isinstance(test, unittest.TestSuite):
            yield from test_ids(test)
        else:
            yield test.id()


def main():
    project = Path(sys.argv[1]).resolve()
    tests = subprocess.run(
        [sys.executable, "-m", "unittest"], cwd=project, text=True,
        capture_output=True,
    )
    if tests.returncode:
        fail("visible unittest suite fails\n" + tests.stdout + tests.stderr)
    suite = unittest.defaultTestLoader.discover(str(project))
    required = {
        "test_todo.TodoTests.test_add_and_list",
        "test_todo.TodoTests.test_list_empty",
        "test_todo.TodoTests.test_persists_tasks",
    }
    ids = set(test_ids(suite))
    missing = required - ids
    if missing:
        fail(f"required visible tests missing: {sorted(missing)}")
    new_tests = {
        test_id.rsplit(".", 1)[1]
        for test_id in ids - required
        if test_id.startswith("test_todo.")
    }
    if not new_tests:
        fail("no tests were added for the done subcommand")
    tree = ast.parse((project / "test_todo.py").read_text(encoding="utf-8"))
    new_methods = [
        node for node in ast.walk(tree)
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
        and node.name in new_tests
    ]
    if not any(
        isinstance(node, ast.Constant) and node.value == "done"
        for method in new_methods
        for node in ast.walk(method)
    ):
        fail("a new test must mention the done subcommand")
    with tempfile.TemporaryDirectory() as folder:
        data = Path(folder) / "todo.json"

        def run(*args):
            return subprocess.run(
                [sys.executable, str(project / "todo.py"), "--file", str(data), *args],
                cwd=folder, text=True, capture_output=True,
            )

        added = run("add", "water plants")
        if added.returncode or added.stdout != "added 1: water plants\n":
            fail("add no longer has its required output or status")
        done = run("done", "1")
        if (done.returncode, done.stdout, done.stderr) != (
            0, "done 1: water plants\n", "",
        ):
            fail("done success has incorrect output, stderr, or exit status")
        listed = run("list")
        if listed.stdout != "[x] 1: water plants\n" or listed.returncode:
            fail("done was not persisted and shown by list")
        before = data.read_bytes()
        missing_task = run("done", "99")
        expected = (1, "", "error: task 99 not found\n")
        actual = (missing_task.returncode, missing_task.stdout, missing_task.stderr)
        if actual != expected or data.read_bytes() != before:
            fail("missing-ID error must exit 1, use stderr, and not change JSON")
        state = json.loads(data.read_text(encoding="utf-8"))
        if state[0]["done"] is not True:
            fail("done did not store a JSON true value")
    print("verified: cli-feature")


if __name__ == "__main__":
    main()
