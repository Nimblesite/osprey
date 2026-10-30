#!/usr/bin/env python3
"""Check mobile tool selection and scheduling without an SDK or a network."""

import os
from pathlib import Path
import re
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
# Editor task panels read only files named Makefile, never their includes, and
# list every line that starts `NAME:`, `NAME :=` included. A public target's
# name starts with a letter; private steps start with `_`.
PANEL_ENTRY = re.compile(r"^([a-zA-Z_][a-zA-Z0-9_-]*)\s*:", re.M)
RULE = re.compile(r"^([a-zA-Z_][a-zA-Z0-9_-]*)[ \t]*:(?!=)", re.M)
STUB_TOOL = """#!/bin/sh
case "$*" in
  *"am instrument"*) printf '%s\\n' "$REPORT" ;;
  *"settings get"*) echo null ;;
  *"exec-out"*)
    case "$SCREENSHOT_MODE" in
      unavailable) echo 'stub screenshot transfer failed' >&2; exit 7 ;;
      malformed) printf 'not a PNG' ;;
      *) printf '\\211PNG\\r\\n\\032\\n' ;;
    esac ;;
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


class BuildFragments(unittest.TestCase):
    def test_task_panels_list_every_public_target_and_no_variable(self):
        fragments = [*ROOT.glob("scripts/make/*.mk"), *ROOT.glob("scripts/*.mk")]
        hidden = {f"{path.name}: {name}" for path in fragments for name in RULE.findall(path.read_text())
                  if not name.startswith("_")}
        self.assertEqual(hidden, set(), "public targets in fragments, where task panels cannot see them")
        for makefile in [ROOT / "Makefile", *ROOT.glob("*/Makefile")]:
            text = makefile.read_text()
            listed = set(PANEL_ENTRY.findall(text)) - set(RULE.findall(text))
            self.assertEqual(listed, set(), f"{makefile.relative_to(ROOT)}: `NAME :=` lines a task panel lists")

    def test_node_dependency_guard_follows_nested_build_fragments(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Makefile").write_text("\n".join(f"target{n}:" for n in range(20)) + "\ninclude tasks.mk\n")
            (root / "tasks.mk").write_text("include website.mk\n")
            for prerequisite, succeeds in [("$(WEBSITE_NODE_DEPS)", True), ("", False)]:
                with self.subTest(prerequisite=prerequisite):
                    (root / "website.mk").write_text(f"site: {prerequisite}\n\tcd website && npm run build\n")
                    result = subprocess.run(
                        ["node", str(ROOT / "scripts/verify-node-deps-guard.mjs")],
                        cwd=root, capture_output=True, text=True,
                    )
                    self.assertEqual(result.returncode == 0, succeeds, result.stderr)
                    self.assertIn("1 node-tool recipes" if succeeds else "site: runs a node tool in website/", result.stderr)


class CorpusScheduling(unittest.TestCase):
    def test_native_worker_isolates_writes_and_preserves_failure_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            compiler = root / "target/release/osprey"
            compiler.parent.mkdir(parents=True)
            compiler.write_text('#!/bin/sh\nprintf payload > worker-artifact\nprintf failed\nexit 7\n')
            compiler.chmod(0o755)
            result = subprocess.run(
                ["zsh", str(ROOT / "crates/run_test_corpus.sh"), "--worker", "default", str(root), "1", "unused.osp"],
                cwd=root, env=dict(os.environ, OSPREY_ROOT=str(root)), capture_output=True, text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((root / "1.status").read_text(), "7\n")
            self.assertEqual((root / "1.stdout").read_text(), "failed")
            self.assertFalse((root / "worker-artifact").exists())
            self.assertEqual((root / "1.work/worker-artifact").read_text(), "payload")

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
            pinned = root / "cache/osprey-distributions/gradle-9.6.0/bin/gradle"
            for script, version in [(unrelated, "wrong-gradle-8"), (pinned, "Gradle 9.6.0")]:
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
            self.assertEqual(result.stdout.strip(), "Gradle 9.6.0")


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


def instrumentation_result(report, screenshot_mode="valid"):
    """Exit status of the bank's Android test driver for a device that prints `report`."""
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        stub_android_tree(root)
        env = {
            "PATH": BASE_PATH, "REPORT": report, "SCREENSHOT_MODE": screenshot_mode, "OSPREY_ANDROID_SKIP_BUILD": "1",
            "OSPREY_ANDROID_SERIAL": "stub", "ANDROID_HOME": str(root / "sdk"),
            "ANDROID_NDK_HOME": str(root / "ndk"),
        }
        return subprocess.run(
            ["bash", str(root / BANK_RUN), "--test"], env=env,
            capture_output=True, text=True, check=False,
        )


class BankAndroidVerdict(unittest.TestCase):
    def test_report_is_judged_with_tools_every_runner_has(self):
        self.assertEqual(instrumentation_result("Time: 23.282\n\nOK (6 tests)").returncode, 0)
        failures = "FAILURES!!!\nTests run: 6,  Failures: 1"
        for report in [failures, "INSTRUMENTATION_FAILED: stub", "Time: 1.2"]:
            self.assertNotEqual(instrumentation_result(report).returncode, 0, report)

    def test_passing_instrumentation_cannot_hide_failed_screenshot_collection(self):
        for mode in ["unavailable", "malformed"]:
            with self.subTest(mode=mode):
                result = instrumentation_result("OK (6 tests)", mode)
                self.assertNotEqual(result.returncode, 0, result.stderr)
                self.assertIn("screenshot", result.stderr.lower())


if __name__ == "__main__":
    unittest.main()
