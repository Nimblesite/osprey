"""painting/reference/compare.sh against stub builders, windows, capture and diff."""
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
import unittest

import dependencies as source

# Through PATH, as a shell would: on Windows a bare "bash" makes CreateProcess search System32
# before PATH and start the WSL launcher instead of the MSYS2 or Git bash this suite runs under.
BASH = shutil.which("bash")

COMPARE = source.HERE.parents[3] / "src/osprey/examples/gpui/painting/reference/compare.sh"
PAINTING_RUN = "src/osprey/examples/gpui/painting/run.sh"
STUB_HEADER = '#!/usr/bin/env bash\nset -euo pipefail\n. "$FIXTURE_BIN/_stub.sh"\n'

# The window stub answers with the reference's window number 4100 and the port's 4200, so each
# capture is identifiable in the event log.
WINDOW_STUB = """record probe "$1"
if [ "$1" = painting-reference ] && [ "${FIXTURE_NO_REFERENCE:-0}" != 1 ]; then echo "4100 1000 700"; fi
if [ "$1" = osprey-gpui ] && [ "${FIXTURE_NO_PORT_WINDOW:-0}" != 1 ]; then
    if grep -q '^port-build-end' "$FIXTURE_EVENTS" && grep -q '^port-launch ' "$FIXTURE_EVENTS"; then
        echo "4200 1000 700"
    fi
fi
"""

# The port's entry point, as painting/run.sh behaves: it builds in both modes, exports the
# application name, and can be made to fail its build.
RUN_STUB = """build() {
    record port-build-start "$*"
    if [ "${FIXTURE_BUILD_FAILED:-0}" = 1 ]; then
        record port-build-failed
        echo "fixture: the painting port did not compile" >&2
        exit 3
    fi
    record port-build-end
}
if [ "${1:-}" = --build ]; then
    build "$@"
    exit 0
fi
build "$@"
export OSPREY_GPUI_APP_NAME=painting
exec "$FIXTURE_HOST_CHILD"
"""

STUBS = {
    "_stub.sh": 'record() { printf \'%s\\n\' "$*" >>"$FIXTURE_EVENTS"; }\n',
    "python3": 'record python3 "$*"\n',
    "pkill": 'record pkill "$*"\nexit 1\n',
    # Keep the absent-window cases fast, but give a real child time to publish
    # its launch event when the comparison expects a window.
    "sleep": ('record sleep "$*"\n'
              'if [ "$1" = 1 ] && [ "${FIXTURE_NO_REFERENCE:-0}" != 1 ] && '
              '[ "${FIXTURE_NO_PORT_WINDOW:-0}" != 1 ]; then "$FIXTURE_REAL_SLEEP" 0.1; fi\n'),
    "cargo": ('record cargo "$*"\nmkdir -p "$CARGO_TARGET_DIR/release"\n'
              'cp "$FIXTURE_BIN/reference-child" "$CARGO_TARGET_DIR/release/painting-reference"\n'
              'chmod +x "$CARGO_TARGET_DIR/release/painting-reference"\n'),
    "swiftc": ('record swiftc "$*"\noutput=""\n'
               'while [ $# -gt 0 ]; do\n'
               '    if [ "$1" = -o ]; then output=$2; shift 2; else shift; fi\n'
               'done\n'
               'cp "$FIXTURE_BIN/${output##*/}" "$output"\nchmod +x "$output"\n'),
    "screencapture": '''record capture "$*"
for last in "$@"; do :; done
owner=unknown
case "$*" in *" -l 4100 "*) owner=painting-reference ;; *" -l 4200 "*) owner=osprey-gpui ;; esac
printf 'pixels\\n' >"$last"
if grep -qx "drawn $owner" "$FIXTURE_EVENTS"; then printf 'drawn\\n' >>"$last"; fi
''',
    "sips": 'record sips "$*"\nprintf \'  pixelWidth: 1000\\n  pixelHeight: 700\\n\'\n',
    "swift": '''record diff "$*"
for last in "$@"; do :; done
: >"$last"
if [ -n "${FIXTURE_DIFF_OUTPUT:-}" ]; then
    echo "the pictures were compared somehow"
    exit "${FIXTURE_DIFF_EXIT:-0}"
fi
if [ -n "${FIXTURE_DIFF_DIFFERING:-}" ]; then
    echo "region 1488x1018 px: ${FIXTURE_DIFF_DIFFERING} differing pixels (0.0000%), largest channel delta 0"
    exit "${FIXTURE_DIFF_EXIT:-1}"
fi
if [ -n "${FIXTURE_DIFF_EXIT:-}" ]; then exit "$FIXTURE_DIFF_EXIT"; fi
if cmp -s "$2" "$3"; then
    echo "region 1488x1018 px: 0 differing pixels (0.0000%), largest channel delta 0"
    exit 0
fi
echo "region 1488x1018 px: 1 differing pixels (0.0100%), largest channel delta 255"
exit 1
''',
    "draw": ('record draw "$1"\n'
             'if [ "${FIXTURE_DRAW_INEFFECTIVE:-0}" != 1 ]; then record drawn "$1"; fi\n'),
    "window": WINDOW_STUB,
    "painting-run.sh": RUN_STUB,
    "reference-child": 'record reference-launch "$$"\nexec "$FIXTURE_REAL_SLEEP" 10\n',
    "host-child": ('record port-launch "$$" "${OSPREY_GPUI_WINDOW:-none}" '
                   '"${OSPREY_GPUI_APP_NAME:-none}"\nexec "$FIXTURE_REAL_SLEEP" 10\n'),
}


def write_stub(path, body):
    path.write_text(STUB_HEADER + body)
    path.chmod(0o755)


def build_compare_fixture(root):
    stubs = root / "bin"
    stubs.mkdir()
    (stubs / "_stub.sh").write_text(STUBS["_stub.sh"])
    for name, body in STUBS.items():
        if name != "_stub.sh":
            write_stub(stubs / name, body)
    repo = root / "repo"
    reference = repo / "src/osprey/examples/gpui/painting/reference"
    reference.mkdir(parents=True)
    shutil.copy(COMPARE, reference / "compare.sh")
    shutil.copy(stubs / "painting-run.sh", repo / PAINTING_RUN)
    host = repo / "src/toolchain/hosts/gpui/target/release/osprey-gpui"
    host.parent.mkdir(parents=True)
    shutil.copy(stubs / "host-child", host)
    (repo / "src/toolchain/hosts/gpui/dependencies.py").touch()
    return stubs


class CompareScriptTests(unittest.TestCase):
    """Runs a copy of the real compare.sh against stub builders, windows, capture and diff."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.stubs = build_compare_fixture(self.root)
        self.log = self.root / "events"
        self.log.touch()
        self.addCleanup(self.kill_owned)

    def events(self):
        return self.log.read_text().splitlines()

    def launched(self):
        return [int(line.split()[1]) for line in self.events()
                if line.split()[0] in ("reference-launch", "port-launch")]

    def bash_kill(self, flag, pid):
        command = [BASH, "-c", f'kill -{flag} "$1"', "_", str(pid)]
        return subprocess.run(command, capture_output=True, stdin=subprocess.DEVNULL,
                              timeout=30).returncode == 0

    def kill_owned(self):
        for pid in self.launched():
            self.bash_kill("KILL", pid)

    def run_compare(self, *arguments, trace=False, **environment):
        variables = {"PATH": f"{self.stubs}{os.pathsep}{os.environ['PATH']}", "FIXTURE_BIN": str(self.stubs),
                     "FIXTURE_EVENTS": str(self.log), "FIXTURE_HOST_CHILD": str(self.stubs / "host-child"),
                     "FIXTURE_REAL_SLEEP": shutil.which("sleep"), **environment}
        command = [BASH, *(["-x"] if trace else []),
                   str(self.root / "repo/src/osprey/examples/gpui/painting/reference/compare.sh"),
                   *arguments, str(self.root / "out")]
        # Files, not pipes: a leaked child must not hold the fixture's own output open.
        with (self.root / "stdout").open("w") as out, (self.root / "stderr").open("w") as err:
            code = subprocess.run(command, env={**os.environ, **variables}, stdout=out, stderr=err,
                                  stdin=subprocess.DEVNULL, timeout=120).returncode
        return subprocess.CompletedProcess(command, code, (self.root / "stdout").read_text(),
                                           (self.root / "stderr").read_text())

    def position(self, prefix):
        for index, line in enumerate(self.events()):
            if line.startswith(prefix):
                return index
        self.fail(f"no {prefix!r} event in {self.events()}")

    def output(self, name):
        # As compare.sh spells it, "$out/<name>": on Windows a Path would join with a backslash.
        return f"{self.root / 'out'}/{name}"

    def capture_line(self, window, name):
        return f"capture -x -o -l {window} {self.output(name)}"

    def assert_order(self, earlier, later):
        self.assertLess(self.position(earlier), self.position(later),
                        f"{earlier!r} must precede {later!r}: {self.events()}")

    def matching(self, prefix):
        return [line for line in self.events() if line.startswith(prefix)]

    def assert_reaped(self, pid):
        for _ in range(20):
            if not self.bash_kill("0", pid):
                return
            time.sleep(0.05)
        self.fail(f"child {pid} survived compare.sh")

    def assert_absent_window_fails(self, environment, process, forbidden_capture):
        result = self.run_compare(**environment)
        self.assertNotEqual(result.returncode, 0, f"an absent {process} window must fail")
        self.assertIn(f"no window for {process}", result.stderr)
        self.assertEqual(len(self.matching(f"probe {process}")), 120)
        self.assertEqual(len([line for line in self.events() if line == "sleep 1"]), 120)
        self.assertFalse(self.matching(forbidden_capture))
        self.assertFalse(self.matching("diff "))
        for pid in self.launched():
            self.assert_reaped(pid)

    def test_painting_compare_builds_the_port_before_the_first_capture(self):
        result = self.run_compare("--draw")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assert_order("port-build-end", self.capture_line(4100, "reference.png"))
        self.assertEqual(len(self.matching("port-build-start")), 1)
        self.assertEqual(self.matching("port-launch")[0].split()[2:], ["1000x672", "painting"])
        self.assert_order(self.capture_line(4100, "reference-before-draw.png"), "draw painting-reference")
        self.assert_order("draw painting-reference", self.capture_line(4100, "reference.png"))
        self.assert_order(self.capture_line(4200, "port-before-draw.png"), "draw osprey-gpui")
        self.assert_order("draw osprey-gpui", self.capture_line(4200, "port.png"))
        # The port may be missed by the first probe while it starts, so the invariant is one wait
        # sleep per miss, not a fixed count: every found window still sleeps once before its capture.
        waits = [line for line in self.events() if line == "sleep 1"]
        self.assertEqual(len(self.matching("sleep 2")), 2)
        self.assertGreaterEqual(len(waits), 2)
        self.assertEqual(len(self.matching("probe ")), len(waits))
        diff = self.matching("diff ")[-1].split()
        self.assertEqual(diff[4:], ["0", "56", "1000", "676", self.output("diff.png")])
        self.assertIn("px: 0 differing pixels", result.stdout)

    def test_painting_compare_requires_the_drawing_to_change_the_capture(self):
        result = self.run_compare("--draw")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertRegex(result.stderr, r"changed [1-9][0-9]* pixels")
        for name in ("reference", "port"):
            self.assertTrue((self.root / f"out/{name}-before-draw.png").exists(), name)
        checks = [line.split() for line in self.matching("diff ")]
        self.assertEqual(len(checks), 3, "two draw checks and the final comparison")
        self.assertEqual([line[4:8] for line in checks], [["0", "56", "1000", "676"]] * 3)
        self.assertIn("reference-draw-change.png", checks[0][8])
        self.assertIn("diff.png", checks[-1][8])

    def assert_draw_change_rejected(self, environment, diagnostic):
        start = len(self.events())
        result = self.run_compare("--draw", **environment)
        self.assertNotEqual(result.returncode, 0, diagnostic)
        self.assertIn(diagnostic, result.stderr)
        self.assertTrue((self.root / "out/reference-before-draw.png").exists())
        self.assertTrue((self.root / "out/reference-draw-change.png").exists(), "keep the change mask")
        comparisons = [line for line in self.events()[start:] if line.startswith("diff ")]
        self.assertEqual(len(comparisons), 1, "the final comparison must not run")

    def test_painting_compare_rejects_a_drawing_that_changed_nothing(self):
        self.assert_draw_change_rejected({"FIXTURE_DRAW_INEFFECTIVE": "1"}, "changed no content")

    def test_painting_compare_needs_a_usable_report_and_count_for_a_draw_change(self):
        self.assert_draw_change_rejected({"FIXTURE_DIFF_OUTPUT": "malformed", "FIXTURE_DIFF_EXIT": "1"},
                                         "reported no differing-pixel count")
        self.assert_draw_change_rejected({"FIXTURE_DIFF_DIFFERING": "0", "FIXTURE_DIFF_EXIT": "1"},
                                         "reported no differing pixels")
        self.assert_draw_change_rejected({"FIXTURE_DIFF_DIFFERING": "3", "FIXTURE_DIFF_EXIT": "0"},
                                         "changed no content")

    def test_painting_compare_fails_when_the_diff_finds_differing_pixels(self):
        result = self.run_compare(FIXTURE_DIFF_DIFFERING="3")
        self.assertNotEqual(result.returncode, 0, "a nonzero differing-pixel count must fail")
        self.assertIn("px: 3 differing pixels", result.stdout)
        self.assertIn("pixel comparison failed", result.stderr)
        self.assertTrue((self.root / "out/diff.png").exists(), "the diff mask must be kept")

    def test_painting_compare_fails_closed_on_a_report_that_does_not_condemn_itself(self):
        for environment, diagnostic in (
                ({"FIXTURE_DIFF_DIFFERING": "3", "FIXTURE_DIFF_EXIT": "0"}, "differs in 3 pixels"),
                ({"FIXTURE_DIFF_OUTPUT": "malformed"}, "no differing-pixel count")):
            with self.subTest(diagnostic=diagnostic):
                result = self.run_compare(**environment)
                self.assertNotEqual(result.returncode, 0, diagnostic)
                self.assertIn(diagnostic, result.stderr)
                self.assertTrue((self.root / "out/diff.png").exists(), "the diff mask must be kept")

    def test_painting_compare_fails_closed_when_the_diff_command_fails(self):
        result = self.run_compare(FIXTURE_DIFF_EXIT="7")
        self.assertNotEqual(result.returncode, 0, "a failing diff command must fail")
        self.assertIn("pixel comparison failed", result.stderr)
        self.assertTrue((self.root / "out/diff.png").exists(), "the diff mask must be kept")

    def test_painting_compare_fails_closed_when_the_port_build_fails(self):
        result = self.run_compare(FIXTURE_BUILD_FAILED="1")
        self.assertNotEqual(result.returncode, 0, "a failed port build must fail the comparison")
        self.assertIn("painting port build failed", result.stderr)
        self.assertRegex(result.stderr, "the painting port did not compile")
        self.assertTrue(self.matching("port-build-failed"))
        self.assertFalse(self.matching("capture "))
        self.assertFalse(self.matching("diff "))
        self.assertFalse(self.matching("probe "))
        self.assertFalse(self.matching("port-launch"))
        self.assertFalse(self.launched())

    def test_painting_compare_kills_only_its_own_children(self):
        result = self.run_compare()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(self.matching("pkill"), "compare.sh must not kill by process name")
        self.assertEqual(len(self.launched()), 2)
        self.assert_order("port-build-end", "port-launch")
        self.assert_order("port-launch", "diff ")
        for pid in self.launched():
            self.assert_reaped(pid)

    def test_painting_compare_signals_each_owned_child_exactly_once(self):
        result = self.run_compare(trace=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        signalled = [int(pid) for pid in re.findall(r"^\+ kill (\d+)$", result.stderr, re.MULTILINE)]
        self.assertEqual(sorted(signalled), sorted(self.launched()),
                         f"each owned child is signalled once, nothing else: {result.stderr}")

    def test_painting_compare_reports_a_missing_reference_window_and_reaps_it(self):
        self.assert_absent_window_fails({"FIXTURE_NO_REFERENCE": "1"}, "painting-reference", "capture ")
        self.assertEqual(len(self.launched()), 1)

    def test_painting_compare_waits_the_full_120_probes_for_the_port_window(self):
        self.assert_absent_window_fails({"FIXTURE_NO_PORT_WINDOW": "1"}, "osprey-gpui",
                                        "capture -x -o -l 4200")
        self.assertTrue(self.matching("capture -x -o -l 4100"), "the reference capture must still run")
        self.assertEqual(len(self.launched()), 2)
