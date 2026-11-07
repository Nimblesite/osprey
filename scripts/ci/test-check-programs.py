#!/usr/bin/env python3
"""Regression guard for the compile-everything gate, scripts/ci/check-programs.py.

The gate decides CI green ([CI-PROGRAMS-COMPILE]), so a silent skip there is a
false negative exactly where it hurts most. Three such skips shipped once
(fixed in ef8214cb0): a stray .osp inside a project folder was dropped because
its folder matched a project, a Python-flavour .py was dropped whenever a
shebang or licence line preceded the flavour marker, and the compiler was
hard-wired to target/release/osprey so CI could pass without running the
compiler under test.

This test builds a throwaway git repository under target/, commits fixtures
covering each trap plus the pass paths, runs the gate there with cwd = the
throwaway repo and OSPREY_BIN = a logging wrapper around the real compiler,
then asserts the exact FAIL set and the exact final summary line. Point
--script at an older copy of the gate to watch the traps fail it again.
"""
import argparse
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import os

REPO = pathlib.Path(__file__).resolve().parents[2]
DEFAULT_SCRIPT = REPO / "scripts/ci/check-programs.py"

GOOD = 'print("ok")\n'
RED = 'print(1 + "x")\n'
# The flavour marker is deliberately not the first line: a shebang and a
# licence comment stand in front of it, as in real tracked scripts.
PY_RED = ('#!/usr/bin/env python3\n'
          '# Copyright 2026 Nobody. Licensed MIT.\n'
          '# osprey: flavor=python\n' + RED)
# A `//` marker the compiler reads and rejects in a .py (flavour disagreement).
PY_SLASH = '// osprey: flavor=ml\nfn bad() { missing }\n'
PY_PLAIN = '# a plain python file\ndef add(a, b):\n    return a + b\n'
PROJECT_TOML = ('[project]\nname = "gate-fixture"\nsource_roots = ["src"]\n'
                'default_namespace = "app"\nentry = "main.osp"\n')
# Every tracked fixture: app/ is the only project; scratch/stray.osp sits
# inside it but outside its --source-graph; x/ holds the gate's excluded trees.
FIXTURES = {
    "app/osprey.toml": PROJECT_TOML,
    "app/src/main.osp": GOOD,
    "app/scratch/stray.osp": RED,
    "loose/good.osp": GOOD,
    "loose/bad.osp": RED,
    "pyflavor/script.py": PY_RED,
    "pyflavor/slash.py": PY_SLASH,
    "pyflavor/plain.py": PY_PLAIN,
    "x/failscompilation/red.osp": RED,
    "x/sample-registry/red.osp": RED,
}
PROJECT = "app"
MUST_FAIL = ("app/scratch/stray.osp", "loose/bad.osp", "pyflavor/script.py", "pyflavor/slash.py")
MUST_PASS = ("loose/good.osp",)
EXCLUDED = ("x/failscompilation/red.osp", "x/sample-registry/red.osp")
FINAL_LINE = f"{1 + len(MUST_FAIL) + len(MUST_PASS)} programs, {len(MUST_FAIL)} failed"
FAIL_LINE = re.compile(r"^FAIL (\S+)$", re.MULTILINE)
GATE_ARGS = ("--run", "--quiet", "--no-warnings")
# Every gate compile goes through this wrapper; $* lands in $WRAPPER_LOG, then
# the real compiler takes over, so a gate that ignores OSPREY_BIN logs nothing.
WRAPPER = ('#!/bin/sh\n'
           'printf \'%s\\n\' "$*" >> "$WRAPPER_LOG"\n'
           'exec "$REAL_COMPILER" "$@"\n')


def git(root, *argv):
    done = subprocess.run(["git", "-c", "user.email=gate@example.com", "-c", "user.name=gate",
                           "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", *argv],
                          cwd=root, capture_output=True, text=True)
    if done.returncode != 0:
        sys.exit(f"git {' '.join(argv)} failed in the fixture repo:\n{done.stderr}")
    return done.stdout


def commit_fixtures(root):
    git(root, "init", "-q")
    git(root, "add", *FIXTURES)
    git(root, "commit", "-q", "-m", "gate fixtures")


def make_repo(root, args):
    """Lay out the throwaway repo: fixtures, the gate under test, the wrapper."""
    for path, text in FIXTURES.items():
        file = root/path
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(text)
    (root/"scripts/ci").mkdir(parents=True)
    shutil.copyfile(args.script, root/"scripts/ci/check-programs.py")
    wrapper = root/"compiler-wrapper.sh"
    wrapper.write_text(WRAPPER)
    wrapper.chmod(0o755)
    commit_fixtures(root)


def gate_run(root, args):
    """Run the gate exactly as make _check_programs does, wrapper as compiler."""
    env = {**os.environ, "OSPREY_BIN": str(root/"compiler-wrapper.sh"),
           "REAL_COMPILER": str(args.compiler), "WRAPPER_LOG": str(root/"wrapper.log")}
    return subprocess.run([str(args.compiler), "scripts/ci/check-programs.py", *GATE_ARGS],
                          cwd=root, env=env, capture_output=True, text=True)


def summary_line(done):
    lines = [line for line in done.stdout.splitlines() if line.strip()]
    return lines[-1] if lines else ""


def wrapper_log(root):
    return (root/"wrapper.log").read_text() if (root/"wrapper.log").exists() else ""


def fixture_checks(done, root, fails, log):
    """One check per guarded trap or pass path; each yields None or a reason."""
    return reported_checks(done, fails, log) + compiler_checks(root, fails, log)


def reported_checks(done, fails, log):
    """What the gate reports: the summary line and which fixtures FAIL."""
    return (
        ("fixture 1: project compiles and counts are exact",
         None if summary_line(done) == FINAL_LINE
         else f"last line {summary_line(done)!r}, want {FINAL_LINE!r}\n{done.stdout}"),
        ("fixture 2: stray .osp inside the project is reported",
         None if "app/scratch/stray.osp" in fails else f"FAIL set {sorted(fails)}"),
        ("fixture 3: loose red reported, loose good compiled and not reported",
         None if "loose/bad.osp" in fails and "loose/good.osp" not in fails and "loose/good.osp" in log
         else f"FAIL set {sorted(fails)}, log mentions good: {'loose/good.osp' in log}"),
        ("fixture 4: python flavour behind shebang+licence is reported",
         None if "pyflavor/script.py" in fails else f"FAIL set {sorted(fails)}"),
        ("fixture 4: a // flavour marker in a .py is compiled and its rejection reported",
         None if "pyflavor/slash.py" in fails else f"FAIL set {sorted(fails)}"),
        ("fixture 4: plain .py without the marker is never compiled",
         None if "pyflavor/plain.py" not in log and "pyflavor/plain.py" not in done.stdout
         else "plain.py was compiled or counted"),
    )


def compiler_checks(root, fails, log):
    """The compiler under test ran every job, and the exclusions hold."""
    return (
        ("fixture 5: failscompilation/ and sample-registry/ are never reported",
         None if not fails.intersection(EXCLUDED) else f"FAIL set {sorted(fails)}"),
        ("fixture 6: every compile ran the compiler under test",
         None if "app --source-graph" in log and "loose/bad.osp --llvm" in log
         else "wrapper log is empty or missing gate invocations:\n" + log),
        ("fixture 6: no hard-wired target/release/osprey output in the repo",
         None if not (root/"target/release/osprey").exists()
         else "target/release/osprey exists, so a hard-wired path would compile"),
        ("gate: FAIL set is exactly the red fixtures",
         None if fails == set(MUST_FAIL) else f"FAIL set {sorted(fails)}, want {sorted(MUST_FAIL)}"),
    )


def report(checks):
    failures = 0
    for label, reason in checks:
        if reason is None:
            print(f"  ok   {label}")
        else:
            failures += 1
            print(f"  FAIL {label}\n       {reason}")
    return failures


def parse_args():
    parser = argparse.ArgumentParser(description="Regression test for check-programs.py")
    parser.add_argument("--compiler", type=pathlib.Path, required=True,
                        help="osprey binary under test (make passes $(BIN))")
    parser.add_argument("--script", type=pathlib.Path, default=DEFAULT_SCRIPT,
                        help="gate script under test (default: the repo's)")
    args = parser.parse_args()
    args.compiler = args.compiler.resolve()
    args.script = args.script if args.script.is_absolute() else (REPO/args.script).resolve()
    if not args.compiler.is_file():
        sys.exit(f"compiler {args.compiler} is not a file — build it first")
    if not args.script.is_file():
        sys.exit(f"gate script {args.script} is not a file")
    return args


def main():
    args = parse_args()
    with tempfile.TemporaryDirectory(prefix="check-programs-test-", dir=REPO/"target") as directory:
        root = pathlib.Path(directory)
        make_repo(root, args)
        done = gate_run(root, args)
        failures = report(fixture_checks(done, root, set(re.findall(FAIL_LINE, done.stdout)),
                                         wrapper_log(root)))
    if failures:
        print(f"\ncheck-programs gate regression: FAIL ({failures} cases)")
        return 1
    print(f"check-programs gate regression: OK ({FINAL_LINE})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
