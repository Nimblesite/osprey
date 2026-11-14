#!/usr/bin/env python3
"""Portable mobile tooling checks and an explicit macOS host ownership oracle."""

import os
import json
import shlex
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent.parent
WRAPPER = ROOT / "scripts/mobile/gradlew"
BANK_RUN = "src/osprey/examples/projects/modules/mobile/android/run.sh"
ANDROID_ENV = "scripts/mobile/android-env.sh"
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

# Compile the actual host; doubles isolate string ownership from platform services.
STORAGE_HOST_PROBE = r'''
struct NativeNode: Decodable { let kind: String }
final class DiskTree {
    struct Guard { let apps: Set<String> }
    static var current: DiskTree?
    static func measure(_ root: String, guarding: Guard, progress: (Int64, Int64) -> Void) -> DiskTree? { nil }
    func graft(_ tree: DiskTree, at: String) {}
}
enum PhotoLibrary { static func measure(_ done: @escaping () -> Void) { done() } }
func canonical(_ path: String) -> String { path }
func fullDiskAccess() -> Bool { false }
@MainActor enum HostProbe {
    static var mode = "valid"
    static var live = Set<UnsafeRawPointer>()
    static var releases = 0
    static func output() -> UnsafePointer<CChar>? {
        if mode == "nil" { return nil }
        let value = mode == "valid" ? #"{"model":"state","ui":{"kind":"ready"},"commands":[]}"# : "{broken"
        guard let pointer = strdup(value) else { fatalError("test allocation failed") }
        live.insert(UnsafeRawPointer(pointer))
        return UnsafePointer(pointer)
    }
}
@MainActor func osprey_main() -> Int32 { 0 }
@MainActor func osprey_storage_start(_ home: String, _ platform: String, _ full: Bool) -> UnsafePointer<CChar>? { HostProbe.output() }
@MainActor func osprey_storage_dispatch(_ model: String, _ event: String) -> UnsafePointer<CChar>? { HostProbe.output() }
@MainActor func osprey_release_string(_ pointer: UnsafePointer<CChar>?) {
    guard let pointer else { preconditionFailure("released null output") }
    precondition(HostProbe.live.remove(UnsafeRawPointer(pointer)) != nil, "released foreign or duplicate output")
    HostProbe.releases += 1
    free(UnsafeMutablePointer(mutating: pointer))
}
@main struct StorageOwnershipProbe {
    @MainActor static func validOutputs() {
        let store = StorageStore()
        store.start()
        precondition(store.ui?.kind == "ready" && store.failure == nil, "valid screen lost")
        precondition(HostProbe.live.isEmpty && HostProbe.releases == 1, "StorageStore leaked successful output")
        for index in 0..<100 { store.send(["index": index]) }
        precondition(HostProbe.live.isEmpty && HostProbe.releases == 101, "StorageStore dispatch leaked output")
    }
    @MainActor static func failedOutputs() {
        HostProbe.mode = "broken"
        let invalid = StorageStore()
        invalid.start()
        precondition(invalid.failure != nil && HostProbe.live.isEmpty && HostProbe.releases == 102, "StorageStore leaked malformed output")
        HostProbe.mode = "nil"
        let absent = StorageStore()
        absent.start()
        precondition(absent.failure != nil && HostProbe.live.isEmpty && HostProbe.releases == 102, "StorageStore mishandled null output")
    }
    @MainActor static func main() {
        validOutputs()
        failedOutputs()
        print("Storage host output ownership: PASS")
    }
}
'''

COUNTER_HOST_PROBE = r'''
@_silgen_name("osp_arc_live_objects") func liveObjects() -> UInt64
@_silgen_name("osp_arc_live_bytes") func liveBytes() -> UInt64
@_cdecl("ios_host_log") func probeLog(_ value: UnsafePointer<CChar>?) -> Int64 {
    value.map { Int64(strlen($0)) } ?? -1
}
@main struct CounterOwnershipProbe {
    static func initializedCounts() -> (UInt64, UInt64) {
        precondition(setenv("OSPREY_ARC_DEBUG", "1", 1) == 0)
        precondition(osprey_main() == 0)
        let objects = liveObjects(), bytes = liveBytes()
        let control = osprey_summary(5)
        precondition(liveObjects() == objects + 1 && liveBytes() > bytes, "ARC allocation control failed")
        osprey_release_string(control)
        precondition(liveObjects() == objects && liveBytes() == bytes)
        return (objects, bytes)
    }
    static func main() {
        let (objects, bytes) = initializedCounts()
        for _ in 0..<1000 {
            precondition(ospreyString(osprey_summary(5)) == "Count: 5")
        }
        precondition(liveObjects() == objects && liveBytes() == bytes, "Counter leaked owned outputs")
        precondition(ospreyString(nil) == "Osprey returned a null string")
        print("Counter host output ownership: PASS")
    }
}
'''

def run_swift_probe(source, binary, *inputs):
    subprocess.run(["swiftc", "-parse-as-library", str(source), *inputs, "-o", str(binary)],
                   check=True, timeout=60)
    subprocess.run([str(binary)], check=True, timeout=10)

def storage_host_probe():
    host = ROOT / "src/osprey/examples/macos/OspreyStorage/StorageStore.swift"
    with tempfile.TemporaryDirectory(prefix="osprey-storage-ownership-") as scratch:
        source = Path(scratch) / "probe.swift"
        binary = Path(scratch) / "probe"
        source.write_text("import Foundation\nimport Darwin\n" + STORAGE_HOST_PROBE)
        run_swift_probe(source, binary, str(host))

def counter_host_probe():
    host = ROOT / "src/osprey/examples/ios/OspreyCounter/OspreyCounterApp.swift"
    compiler = os.environ.get("OSPREY_BIN", str(ROOT / "target/release/osprey"))
    with tempfile.TemporaryDirectory(prefix="osprey-counter-ownership-") as scratch:
        source, archive = Path(scratch) / "probe.swift", Path(scratch) / "counter.a"
        subprocess.run([compiler, str(ROOT / "src/osprey/examples/ios/app.osp"), "--compile", "--target=macos",
                        "--memory=arc", "--quiet", "--no-warnings", "-o", str(archive)],
                       check=True, timeout=60)
        helper = "func ospreyString(" + host.read_text().split("func ospreyString(", 1)[1]
        source.write_text("import Foundation\n" + helper + COUNTER_HOST_PROBE)
        run_swift_probe(source, Path(scratch) / "probe", "-import-objc-header",
                        str(archive.with_suffix(".h")), str(archive))

def corpus_jobs(target, override=None):
    harness = (ROOT / "src/toolchain/crates/run_test_corpus.sh").read_text()
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

def swift_packaging_result(root):
    host = root / "src/toolchain/hosts/swift"
    host.mkdir(parents=True)
    launcher = host / "run.sh"
    shutil.copyfile(ROOT / "src/toolchain/hosts/swift/run.sh", launcher)
    compiler = root / "src" / "toolchain" / "compiler"
    compiler.write_text("#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$CAPTURE\"\n"
                        "echo 'compile error sentinel' >&2\nexit 37\n")
    compiler.chmod(0o755)
    return subprocess.run(
        ["bash", str(launcher), "--build"], timeout=10,
        env=dict(os.environ, OSPREY_BIN=str(compiler),
                 OSPREY_SWIFT_APP=str(root / "app with spaces"),
                 CAPTURE=str(root / "arguments")), capture_output=True, text=True,
    )

class BuildFragments(unittest.TestCase):
    def test_swift_packaging_skips_advice_and_preserves_compile_errors(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            result = swift_packaging_result(root)
            host, app = root / "src/toolchain/hosts/swift", root / "app with spaces"
            self.assertEqual(result.returncode, 37, result.stderr)
            self.assertIn("compile error sentinel", result.stderr)
            self.assertEqual((root / "arguments").read_text().splitlines(), [
                str(app), "--compile", "--target=macos", "--memory=arc", "--quiet",
                "--no-warnings", "-o", str(host / "build/app/libOspreyApp.a"),
            ])
            self.assertFalse((host / "build/app with spaces.app").exists())

    def test_task_panels_list_every_public_target_and_no_variable(self):
        fragments = [*ROOT.glob("scripts/make/*.mk")]
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
                    (root / "website.mk").write_text(f"site: {prerequisite}\n\tcd src/toolchain/website && npm run build\n")
                    result = subprocess.run(
                        ["node", str(ROOT / "scripts/ci/verify-node-deps-guard.mjs")],
                        cwd=root, capture_output=True, text=True,
                    )
                    self.assertEqual(result.returncode == 0, succeeds, result.stderr)
                    self.assertIn("1 node-tool recipes" if succeeds else "site: runs a node tool in src/toolchain/website/", result.stderr)

class CorpusScheduling(unittest.TestCase):
    def test_native_worker_isolates_writes_and_preserves_failure_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            compiler = root / "target/release/osprey"
            compiler.parent.mkdir(parents=True)
            compiler.write_text('#!/bin/sh\nprintf payload > worker-artifact\nprintf failed\nexit 7\n')
            compiler.chmod(0o755)
            result = subprocess.run(
                ["zsh", str(ROOT / "src/toolchain/crates/run_test_corpus.sh"), "--worker", "default", str(root), "1", "unused.osp"],
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

class SimulatorSelection(unittest.TestCase):
    def test_renamed_iphones_are_selected_by_device_type_not_display_name(self):
        phone = {"name": "ci-prep-iphone", "udid": "phone", "state": "Booted",
                 "deviceTypeIdentifier": "com.apple.CoreSimulator.SimDeviceType.iPhone-17"}
        tablet = {"name": "iPhone misleading name", "udid": "tablet", "state": "Booted",
                  "deviceTypeIdentifier": "com.apple.CoreSimulator.SimDeviceType.iPad-Pro"}
        for devices, requested, expected in [([phone], None, "phone"), ([tablet, phone], None, "phone"),
                                              ([tablet, phone], "tablet", "tablet"), ([tablet], None, None)]:
            with self.subTest(devices=devices, requested=requested), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                fixture = root / "devices.json"
                fixture.write_text(json.dumps({"devices": {"com.apple.CoreSimulator.SimRuntime.iOS-27-0": devices}}))
                stub = root / "xcrun"
                stub.write_text("#!/bin/sh\ncase \"$*\" in\n  *'list devices'*) cat " + shlex.quote(str(fixture)) + ";;\nesac\n")
                stub.chmod(0o755)
                developer = root / "Selected Xcode.app/Contents/Developer"
                developer.mkdir(parents=True)
                (root / "xcode-select").write_text('#!/bin/sh\nprintf "%s\\n" "$TEST_XCODE_DEVELOPER"\n')
                (root / "open").write_text('#!/bin/sh\nprintf "%s\\n" "$@" > "$TEST_OPEN_LOG"\n')
                for name in ["xcode-select", "open"]:
                    (root / name).chmod(0o755)
                env = dict(os.environ, PATH=folder + os.pathsep + os.environ["PATH"])
                env.update(TEST_XCODE_DEVELOPER=str(developer), TEST_OPEN_LOG=str(root / "opened"))
                env.pop("OSPREY_SIMULATOR_UDID", None)
                if requested:
                    env["OSPREY_SIMULATOR_UDID"] = requested
                result = subprocess.run(["bash", str(ROOT / "scripts/mobile/ios-simulator.sh")], env=env,
                                        text=True, capture_output=True, timeout=10)
                if expected is None:
                    self.assertNotEqual(result.returncode, 0, result.stdout)
                    self.assertIn("No matching iPhone simulator", result.stderr)
                else:
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stdout.strip(), expected)
                    self.assertFalse((root / "opened").exists(), "Headless selection opened a window")
                    missing = subprocess.run(["bash", str(ROOT / "scripts/mobile/ios-simulator.sh")],
                        env=dict(env, OSPREY_SIMULATOR_VISIBLE="1"), text=True, capture_output=True, timeout=10)
                    self.assertNotEqual(missing.returncode, 0)
                    self.assertIn("No simulator viewer", missing.stderr)
                    for viewer in ["DeviceHub.app", "Simulator.app"]:
                        app = developer.parent / "Applications" / viewer if viewer == "DeviceHub.app" else developer / "Applications" / viewer
                        app.mkdir(parents=True)
                        result = subprocess.run(["bash", str(ROOT / "scripts/mobile/ios-simulator.sh")],
                            env=dict(env, OSPREY_SIMULATOR_VISIBLE="1"), text=True, capture_output=True, timeout=10)
                        self.assertEqual(result.returncode, 0, result.stderr)
                        expected_open = ["-a", str(app), "devices://device/open?id=" + expected] if viewer == "DeviceHub.app" else ["-a", str(app), "--args", "-CurrentDeviceUDID", expected]
                        self.assertEqual((root / "opened").read_text().splitlines(), expected_open)

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

GOLDEN = ROOT / "src/osprey/examples/tally/goldens/android/1080-ready.png"
GOLDENS = ROOT / "scripts/mobile/goldens.py"


def goldens_module():
    """scripts/mobile/goldens.py, imported by path."""
    import importlib.util
    spec = importlib.util.spec_from_file_location("goldens", GOLDENS)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class PhoneGoldens(unittest.TestCase):
    """[NATIVE-HOSTS-MOBILE] screenshots are compared with nothing but Python, on every runner."""

    def test_malformed_png_cannot_pass_a_golden(self):
        goldens = goldens_module()
        def chunk(kind, body):
            return goldens.struct.pack(">I", len(body)) + kind + body + goldens.struct.pack(">I", goldens.zlib.crc32(kind + body))
        def picture(raw, width=2, height=2, compression=0, filtering=0):
            head = goldens.struct.pack(">IIBBBBB", width, height, 8, 2, compression, filtering, 0)
            return goldens.SIGNATURE + chunk(b"IHDR", head) + chunk(b"IDAT", goldens.zlib.compress(raw)) + chunk(b"IEND", b"")
        raw = (b"\0" + bytes(6)) * 2
        valid = picture(raw)
        cases = {"short row": picture(raw[:-3]), "extra bytes": picture(raw + b"x"),
                 "unknown filter": picture(b"\x05" + raw[1:]), "empty width": picture(b"\0\0", width=0),
                 "compression": picture(raw, compression=1), "filter method": picture(raw, filtering=1),
                 "checksum": valid[:29] + bytes([valid[29] ^ 1]) + valid[30:],
                 "missing end": valid[:-12], "truncated chunk": valid[:-1], "trailing data": valid + b"x",
                 "short header": goldens.SIGNATURE, "oversized dimensions": picture(raw, width=1 << 31, height=1 << 31),
                 "invalid zlib": valid[:33] + chunk(b"IDAT", b"not zlib") + chunk(b"IEND", b"")}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "screen.png"
            (Path(directory) / "2-ready.png").write_bytes(valid)
            path.write_bytes(valid)
            self.assertEqual(self.compare(path, Path(directory)).returncode, 0)
            for reason, data in cases.items():
                with self.subTest(reason=reason):
                    path.write_bytes(data)
                    with self.assertRaises(AssertionError):
                        goldens.png_pixels(path)
                    refused = self.compare(path, Path(directory))
                    self.assertEqual(refused.returncode, 1, reason)
                    self.assertIn("FAIL:", refused.stderr)
            compressed = goldens.zlib.compress(raw)
            path.write_bytes(valid[:33] + chunk(b"IDAT", compressed[:5]) + chunk(b"IDAT", compressed[5:]) + chunk(b"IEND", b""))
            self.assertEqual(goldens.png_pixels(path), (2, 2, bytes(12)), "split IDAT is a valid screen")
            pixels = bytes(range(10, 130, 10))
            residuals = [pixels,
                         bytes([10, 20, 30, 30, 30, 30, 70, 80, 90, 30, 30, 30]),
                         bytes([10, 20, 30, 40, 50, 60, 60, 60, 60, 60, 60, 60]),
                         bytes([10, 20, 30, 35, 40, 45, 65, 70, 75, 45, 45, 45]),
                         bytes([10, 20, 30, 30, 30, 30, 60, 60, 60, 30, 30, 30])]
            for kind, residual in enumerate(residuals):
                with self.subTest(filter=kind):
                    path.write_bytes(picture(bytes([kind]) + residual[:6] + bytes([kind]) + residual[6:]))
                    self.assertEqual(goldens.png_pixels(path), (2, 2, pixels), "every filter reconstructs known RGB bytes")
        for pixels in (bytes(9), bytes(15)):
            with self.subTest(length=len(pixels)):
                with self.assertRaises(AssertionError):
                    goldens.png_encode(2, 2, pixels)
                with self.assertRaises(AssertionError):
                    goldens.picture_delta((2, 2, pixels), (2, 2, bytes(12)))

    def compare(self, actual, folder):
        return subprocess.run([sys.executable, str(GOLDENS), str(actual), str(folder), "--top", "0", "--bottom", "0"],
                              env={"PATH": BASE_PATH}, capture_output=True, text=True, check=False)

    @unittest.skipUnless(shutil.which("ffmpeg"), "ffmpeg cross-checks the decoder where it is installed")
    def test_decoder_reads_the_same_pixels_ffmpeg_does(self):
        width, height, pixels = goldens_module().png_pixels(GOLDEN)
        decoded = subprocess.run(["ffmpeg", "-v", "error", "-i", str(GOLDEN), "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"],
                                 check=True, stdout=subprocess.PIPE).stdout
        self.assertEqual((width, height), (1080, 2088))
        self.assertEqual(pixels, decoded)

    def test_screens_compare_without_ffmpeg_and_a_moved_control_fails(self):
        goldens = goldens_module()
        width, height, pixels = goldens.png_pixels(GOLDEN)
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            shutil.copy(GOLDEN, root / f"{width}-ready.png")
            rgba = b"".join(pixels[i:i + 3] + bytes([(i // 3) % 256]) for i in range(0, len(pixels), 3))
            (root / "same.png").write_bytes(goldens.png_encode(width, height, rgba, alpha=True))
            same = self.compare(root / "same.png", root)
            self.assertEqual(same.returncode, 0, same.stderr)
            self.assertIn("(0.000% differing pixels)", same.stdout)
            self.assertEqual(goldens.png_pixels(root / "same-content.png"), (width, height, pixels), "the review crop is the screen")
            moved = bytearray(pixels)
            for row in range(400, 480):
                start = (row * width + 200) * 3
                moved[start:start + 80 * 3] = bytes(255 - value for value in moved[start:start + 80 * 3])
            goldens.png_write(root / "moved.png", width, height, bytes(moved))
            failed = self.compare(root / "moved.png", root)
            self.assertEqual(failed.returncode, 1, failed.stdout)
            self.assertRegex(failed.stderr, r"FAIL: .*of pixels differ, worst tile")
            (root / "text.png").write_bytes(b"not a PNG")
            refused = self.compare(root / "text.png", root)
            self.assertNotEqual(refused.returncode, 0)
            self.assertIn("not a PNG", refused.stderr)

if __name__ == "__main__":
    if sys.argv[1:] == ["--storage-host"]:
        storage_host_probe()
    elif sys.argv[1:] == ["--counter-host"]:
        counter_host_probe()
    else:
        unittest.main()
