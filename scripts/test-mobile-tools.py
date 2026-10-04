#!/usr/bin/env python3
"""Check mobile tool selection and scheduling without an SDK or a network."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent
WRAPPER = ROOT / "examples/mobile/android/gradlew"


def corpus_jobs(target, override=None):
    harness = (ROOT / "crates/run_test_corpus.sh").read_text()
    functions = harness.split("detected_jobs() {", 1)[1].split("\n\nbackend_setup", 1)[0]
    script = "nproc() { print -r -- 32; }\ndetected_jobs() {" + functions
    env = dict(os.environ, TARGET=target)
    env.pop("OSPREY_TEST_JOBS", None)
    if override is not None:
        env["OSPREY_TEST_JOBS"] = override
    return subprocess.run(
        ["zsh", "-c", script + "\nconfigured_jobs"], env=env,
        capture_output=True, text=True, check=False,
    )


class CorpusScheduling(unittest.TestCase):
    def test_android_defaults_to_one_worker_per_device(self):
        for target in ["android", "android-x64", "android-arm64"]:
            with self.subTest(target=target):
                result = corpus_jobs(target)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.strip(), "1")

    def test_host_targets_retain_bounded_parallelism(self):
        for target in ["native", "wasm32"]:
            with self.subTest(target=target):
                result = corpus_jobs(target)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.strip(), "4")

    def test_android_accepts_an_explicit_worker_budget(self):
        result = corpus_jobs("android-x64", "2")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "2")

    def test_android_rejects_invalid_worker_budgets(self):
        for override in ["", "0", "-1", "many"]:
            with self.subTest(override=override):
                result = corpus_jobs("android-x64", override)
                self.assertEqual(result.returncode, 2)
                self.assertIn("must be a positive integer", result.stderr)
                self.assertEqual(result.stdout, "")


class GradleSelection(unittest.TestCase):
    def test_unrelated_gradle_on_path_cannot_override_pinned_distribution(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            unrelated = root / "bin/gradle"
            pinned = root / "cache/osprey-distributions/gradle-8.7/bin/gradle"
            for script, version in [(unrelated, "wrong-gradle-9"), (pinned, "Gradle 8.7")]:
                script.parent.mkdir(parents=True)
                script.write_text(f"#!/bin/sh\nprintf '%s\\n' '{version}'\n")
                script.chmod(0o755)
            env = dict(os.environ, GRADLE_USER_HOME=str(root / "cache"))
            env.pop("GRADLE_BIN", None)
            env["PATH"] = str(unrelated.parent) + os.pathsep + env["PATH"]
            result = subprocess.run(
                ["bash", str(WRAPPER), "--version"], env=env,
                capture_output=True, text=True, check=True,
            )
            self.assertEqual(result.stdout.strip(), "Gradle 8.7")


if __name__ == "__main__":
    unittest.main()
