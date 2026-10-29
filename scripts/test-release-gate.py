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
ALWAYS_RUN = ("scope", "version", "preflight", "audit")

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
a job that should always run did not|fail|SSSSSSSSS|11110
audit skipped on full release|fail|SSSSSSSSS|11110
audit skipped on website release|fail|KKKKKKKSK|00010
audit skipped on prerelease|fail|SSSSSSSKS|11111
audit skipped on extension release|fail|SKKKSSSKK|01100
audit failed on full release|fail|SSSSSSSSS|11110"""


def case(row):
    label, verdict, outcomes, flags = row.split("|")
    assert len(outcomes) == len(JOBS) and len(flags) == 5, label
    statuses = {"S": OK, "K": SKIP, "F": FAIL, "C": CANCEL}
    return (label, verdict == "pass", tuple(statuses[code] for code in outcomes),
            *("true" if flag == "1" else "false" for flag in flags))


TABLE = tuple(case(row) for row in ROWS.splitlines())


def step_script(text, step):
    """Read the actual workflow shell body, never a copy of its implementation."""
    at = text.find(f"- name: {step}\n")
    if at == -1:
        sys.exit(f"{WORKFLOW}: no step named {step!r}")
    block = RUN_BLOCK.search(text, at)
    if block is None:
        sys.exit(f"{WORKFLOW}: found {step!r} but no `run: |` body under it")
    return "".join(line[10:] if line.startswith(" " * 10) else line
                   for line in block.group("body").splitlines(keepends=True))


def gate_script(text):
    body = step_script(text, STEP)
    if "release-complete" not in text or "required=" not in body:
        sys.exit(f"{WORKFLOW}: the extracted body is not the backstop — the parser is broken")
    return body


NATIVE_ARCHIVES = tuple(f"lib{name}{mode}.a" for name in ("fiber_runtime", "http_runtime")
                        for mode in ("", "_gc", "_arc"))
APPLE_ARCHIVES = tuple(f"libosprey_runtime_{target}{mode}.a" for target in ("ios", "ios_sim", "macos")
                       for mode in ("", "_arc"))


def package_result(text, platform, archives, empty_archive=""):
    script = step_script(text, "Package")
    for key, value in {"needs.version.outputs.version": "0.0.0-test", "matrix.platform": platform,
                       "runner.os": "macOS" if platform.startswith("darwin") else "Linux"}.items():
        script = script.replace("${{ " + key + " }}", value)
    with tempfile.TemporaryDirectory(prefix="osprey-release-") as directory:
        root = pathlib.Path(directory)
        for folder in ("target/release", "compiler/lib"):
            (root/folder).mkdir(parents=True)
        for name in ("target/release/osprey", "target/osprey-pkg", "target/osprey-registry"):
            (root/name).write_text("binary")
        (root/"target/local-registry").mkdir()
        for name in (*NATIVE_ARCHIVES, *archives):
            (root/"compiler/lib"/name).write_text("" if name == empty_archive else "archive")
        return subprocess.run(["bash", "-euo", "pipefail", "-c", script], cwd=root,
                              capture_output=True, text=True)


def package_failures(text):
    cases = [("Linux without Apple SDK archives", "linux-x64", (), True),
             ("Darwin with all Apple archives", "darwin-arm64", APPLE_ARCHIVES, True)]
    cases.extend((f"Darwin missing {missing}", "darwin-arm64",
                  tuple(name for name in APPLE_ARCHIVES if name != missing), False)
                 for missing in APPLE_ARCHIVES)
    failures = []
    cases.extend((f"Darwin empty {empty}", "darwin-arm64", APPLE_ARCHIVES, False, empty)
                 for empty in APPLE_ARCHIVES)
    for label, platform, archives, expected, *empty in cases:
        result = package_result(text, platform, archives, *empty)
        if (result.returncode == 0) != expected:
            failures.append(f"{label}: unexpected packaging result\n{result.stdout}{result.stderr}")
        else:
            print(f"  ok   {label}")
    if "run: make _runtime_ios _runtime_ios_sim _runtime_macos" not in text:
        failures.append("Darwin build must compile all supported Apple runtime archives")
    return failures


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


def case_results(label, outcomes):
    results = {job: {"result": OK} for job in ALWAYS_RUN}
    if label.startswith("a job that should always run"):
        results["preflight"] = {"result": SKIP}
    if label.startswith("audit skipped"):
        results["audit"] = {"result": SKIP}
    if label.startswith("audit failed"):
        results["audit"] = {"result": FAIL}
    results.update({job: {"result": r} for job, r in zip(JOBS, outcomes)})
    return results


def decision_failures(script, summary):
    failures = []
    for label, expect_pass, outcomes, *scope in TABLE:
        passed, output = run(script, case_results(label, outcomes), scope, summary)
        if passed != expect_pass:
            failures.append(f"{label}: expected {'pass' if expect_pass else 'fail'}, got "
                            f"{'pass' if passed else 'fail'}\n{output}")
        else:
            print(f"  ok   {label}")
    return failures


def main():
    text = WORKFLOW.read_text()
    failures = package_failures(text)
    with tempfile.NamedTemporaryFile(suffix=".md") as summary:
        failures.extend(decision_failures(gate_script(text), summary.name))
    if failures:
        print("\nrelease gate decision table: FAIL")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print(f"release gate decision table: OK ({len(TABLE)} cases)")
    print(f"release packaging: OK ({2 * len(APPLE_ARCHIVES) + 2} cases)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
