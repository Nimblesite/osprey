import unittest

from pipeline import compact
from pipeline.cli import command_name
from pipeline.dispatch import run
from pipeline.titles import title_key


class PipelineTests(unittest.TestCase):
    def test_core(self):
        self.assertEqual(compact("  Hello   Wide World "), "hello-wide-world")

    def test_module_routes(self):
        self.assertEqual(title_key("A Useful Title"), "a-useful-title")
        self.assertEqual(command_name("Build Project"), "build-project")

    def test_dispatch(self):
        self.assertEqual(run("compact", "Dispatch Me"), "dispatch-me")


if __name__ == "__main__":
    unittest.main()
