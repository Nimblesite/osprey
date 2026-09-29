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

# Each row is label | expected verdict | nine job outcomes | five scope flags.
# S/K/F/C mean success/skipped/failure/cancelled; 1/0 mean true/false.
# A cancelled job outside a tag's required set still fails the global sweep.
ROWS = """full release, every channel published|pass|SSSSSSSSS|11110
full release, Homebrew silently skipped|fail|SSKSSSSSS|11110
full release, a build leg failed|fail|FKKKKKKSK|11110
full release, web compiler deploy failed|fail|SSSSSSSSF|11110
full release, a job was cancelled|fail|SSSCSSSSS|11110
website-only tag, site deployed|pass|KKKKKKKSK|00010
website-only tag, site silently skipped|fail|KKKKKKKKK|00010
website-only tag, an unrequired job was cancelled|fail|CKKKKKKSK|00010
prerelease leaves the live site alone|pass|SSSSSSSKS|11111
prerelease, Open VSX skipped|fail|SSSSSSKKS|11111
vsix-only tag, extension published|pass|SKKKSSSKK|01100
vsix-only tag, Marketplace skipped|fail|SKKKSKSKK|01100
a job that should always run did not|fail|SSSSSSSSS|11110"""


def case(row):
    label, verdict, outcomes, flags = row.split("|")
    assert len(outcomes) == len(JOBS) and len(flags) == 5, label
    statuses = {"S": OK, "K": SKIP, "F": FAIL, "C": CANCEL}
    return (label, verdict == "pass", tuple(statuses[code] for code in outcomes),
            *("true" if flag == "1" else "false" for flag in flags))


TABLE = tuple(case(row) for row in ROWS.splitlines())


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
