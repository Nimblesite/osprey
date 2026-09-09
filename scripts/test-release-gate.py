#!/usr/bin/env python3
"""Decision table for release.yml's `release-complete` backstop.

That job is the only thing standing between a release that published nothing and
a green tick, and it runs exactly once per tag — on a tag, where a mistake in it
is discovered by users. So its logic is exercised here against fabricated job
results instead of being trusted.

The shell under test is read out of the workflow rather than copied, so there is
one implementation: edit the gate and this runs the edited gate. If the step's
shape changes so the body can no longer be found, that is a failure too — a test
that silently stops testing is worse than no test.
"""
import json
import pathlib
import re
import subprocess
import sys
import tempfile

WORKFLOW = pathlib.Path(".github/workflows/release.yml")
STEP = "Assert the release actually happened"
RUN_BLOCK = re.compile(r"^ {8}run: \|\n(?P<body>(?: {10}.*\n|\n)*)", re.MULTILINE)

# Jobs whose outcome the gate is asked about, in the order the table lists them.
JOBS = (
    "build",
    "github-release",
    "brew",
    "scoop",
    "vsix",
    "publish-marketplace",
    "publish-openvsx",
    "deploy-site",
    "deploy-webcompiler",
)
ALWAYS_RUN = ("scope", "version", "preflight")

OK, SKIP, FAIL, CANCEL = "success", "skipped", "failure", "cancelled"

# (label, expect_pass, per-job results, full, build_matrix, vsix, website, prerelease)
TABLE = (
    ("full release, every channel published", True,
     (OK, OK, OK, OK, OK, OK, OK, OK, OK), "true", "true", "true", "true", "false"),
    ("full release, Homebrew silently skipped", False,
     (OK, OK, SKIP, OK, OK, OK, OK, OK, OK), "true", "true", "true", "true", "false"),
    ("full release, a build leg failed", False,
     (FAIL, SKIP, SKIP, SKIP, SKIP, SKIP, SKIP, OK, SKIP), "true", "true", "true", "true", "false"),
    ("full release, web compiler deploy failed", False,
     (OK, OK, OK, OK, OK, OK, OK, OK, FAIL), "true", "true", "true", "true", "false"),
    ("full release, a job was cancelled", False,
     (OK, OK, OK, CANCEL, OK, OK, OK, OK, OK), "true", "true", "true", "true", "false"),
    ("website-only tag, site deployed", True,
     (SKIP, SKIP, SKIP, SKIP, SKIP, SKIP, SKIP, OK, SKIP), "false", "false", "false", "true", "false"),
    ("website-only tag, site silently skipped", False,
     (SKIP,) * 9, "false", "false", "false", "true", "false"),
    # A cancelled job this tag does not require is caught only by the
    # failure/cancelled sweep, so this is the case that pins that sweep.
    ("website-only tag, an unrequired job was cancelled", False,
     (CANCEL, SKIP, SKIP, SKIP, SKIP, SKIP, SKIP, OK, SKIP), "false", "false", "false", "true", "false"),
    ("prerelease leaves the live site alone", True,
     (OK, OK, OK, OK, OK, OK, OK, SKIP, OK), "true", "true", "true", "true", "true"),
    ("prerelease, Open VSX skipped", False,
     (OK, OK, OK, OK, OK, OK, SKIP, SKIP, OK), "true", "true", "true", "true", "true"),
    ("vsix-only tag, extension published", True,
     (OK, SKIP, SKIP, SKIP, OK, OK, OK, SKIP, SKIP), "false", "true", "true", "false", "false"),
    ("vsix-only tag, Marketplace skipped", False,
     (OK, SKIP, SKIP, SKIP, OK, SKIP, OK, SKIP, SKIP), "false", "true", "true", "false", "false"),
    ("a job that should always run did not", False,
     (OK, OK, OK, OK, OK, OK, OK, OK, OK), "true", "true", "true", "true", "false"),
)


def gate_script(text):
    """The `run:` body of the backstop step, de-indented into a runnable script."""
    at = text.find(STEP)
    if at == -1:
        sys.exit(f"{WORKFLOW}: no step named {STEP!r} — the backstop is gone or renamed")
    block = RUN_BLOCK.search(text, at)
    if block is None:
        sys.exit(f"{WORKFLOW}: found {STEP!r} but no `run: |` body under it")
    body = "".join(line[10:] if line.startswith(" " * 10) else line
                   for line in block.group("body").splitlines(keepends=True))
    if "release-complete" not in text or "required=" not in body:
        sys.exit(f"{WORKFLOW}: the extracted body is not the backstop — the parser is broken")
    return body


def run(script, results, scope, summary):
    env = {
        "PATH": "/usr/bin:/bin:/usr/local/bin:/opt/homebrew/bin",
        "GITHUB_STEP_SUMMARY": summary,
        "RESULTS": json.dumps(results),
        "WANT_FULL": scope[0], "WANT_BUILD": scope[1], "WANT_VSIX": scope[2],
        "WANT_SITE": scope[3], "IS_PRERELEASE": scope[4],
    }
    done = subprocess.run(["bash", "-c", script], env=env, capture_output=True, text=True)
    return done.returncode == 0, done.stdout + done.stderr


def main():
    script = gate_script(WORKFLOW.read_text())
    failures = []
    with tempfile.NamedTemporaryFile(suffix=".md") as summary:
        for label, expect_pass, outcomes, *scope in TABLE:
            results = {job: {"result": OK} for job in ALWAYS_RUN}
            # The last row drops an unconditional job to prove the gate notices.
            if label.startswith("a job that should always run"):
                results["preflight"] = {"result": SKIP}
            results.update({job: {"result": r} for job, r in zip(JOBS, outcomes)})
            passed, output = run(script, results, scope, summary.name)
            if passed != expect_pass:
                failures.append(f"{label}: expected {'pass' if expect_pass else 'fail'}, got "
                                f"{'pass' if passed else 'fail'}\n{output}")
            else:
                print(f"  ok   {label}")

    if failures:
        print("\nrelease gate decision table: FAIL")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print(f"release gate decision table: OK ({len(TABLE)} cases)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
