#!/usr/bin/env python3
"""Check mobile tool selection and scheduling without an SDK or a network."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent
WRAPPER = ROOT / "examples/mobile/android/gradlew"
BANK_RUN = "examples/projects/modules/mobile/android/run.sh"
ANDROID_ENV = "scripts/android-env.sh"
# What every runner has: no Homebrew, no cargo bin, no developer extras.
BASE_PATH = "/usr/bin:/bin"
STUB_TOOL = """#!/bin/sh
case "$*" in
  *"am instrument"*) printf '%s\\n' "$REPORT" ;;
  *"settings get"*) echo null ;;
esac
"""


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


def stub_android_tree(root):
    """Copy the bank driver beside a stub SDK whose adb prints $REPORT."""
    hosts = ["darwin-x86_64", "linux-x86_64"]
    clangs = [root / "ndk/toolchains/llvm/prebuilt" / host / "bin/clang" for host in hosts]
    for stub in [root / "sdk/platform-tools/adb", *clangs]:
        stub.parent.mkdir(parents=True)
        stub.write_text(STUB_TOOL)
        stub.chmod(0o755)
    for script in [BANK_RUN, ANDROID_ENV]:
        (root / script).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(ROOT / script, root / script)
    (root / BANK_RUN).parent.joinpath("build").mkdir()


def instrumentation_verdict(report):
    """Exit status of the bank's Android test driver for a device that prints `report`."""
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        stub_android_tree(root)
        env = {
            "PATH": BASE_PATH, "REPORT": report, "OSPREY_ANDROID_SKIP_BUILD": "1",
            "OSPREY_ANDROID_SERIAL": "stub", "ANDROID_HOME": str(root / "sdk"),
            "ANDROID_NDK_HOME": str(root / "ndk"),
        }
        return subprocess.run(
            ["bash", str(root / BANK_RUN), "--test"], env=env,
            capture_output=True, text=True, check=False,
        ).returncode


class BankAndroidVerdict(unittest.TestCase):
    def test_report_is_judged_with_tools_every_runner_has(self):
        self.assertEqual(instrumentation_verdict("Time: 23.282\n\nOK (6 tests)"), 0)
        failures = "FAILURES!!!\nTests run: 6,  Failures: 1"
        for report in [failures, "INSTRUMENTATION_FAILED: stub", "Time: 1.2"]:
            self.assertNotEqual(instrumentation_verdict(report), 0, report)


if __name__ == "__main__":
    unittest.main()
