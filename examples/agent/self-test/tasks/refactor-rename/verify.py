import inspect
from pathlib import Path
import re
import subprocess
import sys
import unittest


def fail(message):
    raise SystemExit(f"refactor-rename: {message}")


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
        "test_pipeline.PipelineTests.test_core",
        "test_pipeline.PipelineTests.test_module_routes",
        "test_pipeline.PipelineTests.test_dispatch",
    }
    missing = required - set(test_ids(suite))
    if missing:
        fail(f"required visible tests missing: {sorted(missing)}")
    for source in project.rglob("*.py"):
        if re.search(r"\bcompact\b", source.read_text(encoding="utf-8")):
            fail(f"old name remains in {source.relative_to(project)}")
    sys.path.insert(0, str(project))
    import pipeline
    from pipeline import cli, dispatch, titles
    from pipeline.core import normalize

    if pipeline.normalize is not normalize:
        fail("package root does not export the renamed function")
    if str(inspect.signature(normalize)) != "(text, *, lowercase=True)":
        fail(f"wrong normalize signature: {inspect.signature(normalize)}")
    expected = "hello-wide-world"
    routes = [
        normalize("  Hello   Wide World "),
        titles.title_key("Hello Wide World"),
        cli.command_name("Hello Wide World"),
        dispatch.run("normalize", "Hello Wide World"),
    ]
    if routes != [expected] * 4:
        fail(f"not every call route preserves behavior: {routes}")
    if normalize(" Hello World ", lowercase=False) != "Hello-World":
        fail("lowercase=False must preserve case")
    if "compact" in dispatch.HANDLERS or "normalize" not in dispatch.HANDLERS:
        fail("dispatch table still uses the old route")
    print("verified: refactor-rename")


if __name__ == "__main__":
    main()
