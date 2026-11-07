#!/usr/bin/env python3
"""Regressions for the unfinished iOS and Android casting tools. [CAST-MOBILE-GPUI]

Mobile casting is not finished, so CI does not run these: `make -C tests casting-mobile-tools` does.
"""

import importlib.util
import io
import os
import shutil
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
from xml.etree import ElementTree

ROOT = Path(__file__).resolve().parent.parent.parent.parent.parent
# What every runner has: no Homebrew, no cargo bin, no developer extras.
BASE_PATH = "/usr/bin:/bin"

def casting_tool(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / f"src/osprey/examples/casting/{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

class CastingGoldenVerdict(unittest.TestCase):
    def test_casting_windowed_target_requests_selected_device_viewer(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            stubs = {"open": '#!/bin/sh\nexit 0\n', "xcode-select": '#!/bin/sh\necho unused\n',
                     "bash": '#!/bin/sh\n[ "${OSPREY_SIMULATOR_VISIBLE:-0}" = "$EXPECTED_VISIBLE" ]\n'}
            for name, script in stubs.items():
                tool = root / name
                tool.write_text(script)
                tool.chmod(0o755)
            for target, visible in [("casting", "1"), ("casting-headless", "0")]:
                env = dict(os.environ, PATH=str(root) + ":" + BASE_PATH, EXPECTED_VISIBLE=visible)
                env.pop("OSPREY_SIMULATOR_VISIBLE", None)
                if target == "casting-headless": env["OSPREY_SIMULATOR_VISIBLE"] = "1"
                result = subprocess.run(["/usr/bin/make", "-f", str(ROOT / "ios/tests/Makefile"), target],
                                        cwd=root, env=env, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_android_packager_uses_shared_ndk_selection(self):
        # [CAST-MOBILE-GPUI] Exercise the real packaging entry point up to C compilation.
        with tempfile.TemporaryDirectory(prefix="casting sdk ") as directory:
            root = Path(directory)
            script = root / "src/osprey/examples/casting/android/build.sh"
            script.parent.mkdir(parents=True)
            shutil.copyfile(ROOT / script.relative_to(root), script)
            common = root / "scripts/mobile/android-env.sh"
            common.parent.mkdir(parents=True)
            shutil.copyfile(ROOT / common.relative_to(root), common)
            sdk, ndk = root / "sdk", root / "custom ndk"
            (sdk / "build-tools/37.0.0").mkdir(parents=True)
            host = "darwin-x86_64" if sys.platform == "darwin" else "linux-x86_64"
            tools = ndk / "toolchains/llvm/prebuilt" / host / "bin"
            tools.mkdir(parents=True)
            clang = tools / "clang"
            clang.write_text('#!/bin/sh\nprintf "%s\\n" "$@" > "$CAPTURE"\nexit 47\n')
            clang.chmod(0o755)
            output = root / "src/osprey/examples/casting/build/android-arm64"
            (output / "openssl/lib").mkdir(parents=True)
            (output / "openssl/lib/libssl.a").touch()
            capture = root / "clang-arguments"
            env = dict(os.environ, ANDROID_HOME=str(sdk), ANDROID_NDK_HOME=str(ndk),
                       OSPREY_MOBILE_SKIP_LIBRARY="1", CAPTURE=str(capture))
            result = subprocess.run(["bash", str(script)], cwd=root, env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 47, result.stdout + result.stderr)
            arguments = capture.read_text().splitlines()
            self.assertIn("--target=aarch64-linux-android26", arguments)
            self.assertIn("-Isrc/toolchain/compiler/runtime", arguments)
            self.assertIn("src/toolchain/compiler/runtime/tls_client_runtime.c", arguments)

    def test_artboard_crop_keeps_bottom_controls(self):
        import mobile_test
        width, height = 100, 200
        raw = bytearray(bytes((255, 248, 239)) * width * 190 + bytes(3 * width * 10))
        marker = (189 * width + 50) * 3
        raw[marker:marker + 3] = bytes((30, 30, 30))
        with mock.patch.object(mobile_test, "png_pixels", return_value=(width, height, bytes(raw))):
            w, h, pixels = mobile_test.player_pixels(Path("fixture.png"))
        self.assertEqual((w, h), (100, 180), "Only the upper system-icon strip is excluded")
        bottom = len(pixels) - width * 3 + 50 * 3
        self.assertEqual(pixels[bottom:bottom + 3], bytes((30, 30, 30)), "The lowest player control must be compared")

    def test_android_package_upgrades_the_archived_renderer(self):
        manifest = ElementTree.parse(ROOT / "src/osprey/examples/casting/android/app/src/main/AndroidManifest.xml").getroot()
        android = "{http://schemas.android.com/apk/res/android}"
        self.assertGreaterEqual(int(manifest.get(android + "versionCode", "0")), 1,
                                "The archived native renderer shipped versionCode=1; GPUI must install over it")
        self.assertTrue(manifest.get(android + "versionName"), "Android must publish its version")
        activity = manifest.find("application/activity")
        self.assertEqual(activity.get(android + "name"), ".CastingActivity")
        self.assertEqual(activity.find("meta-data").get(android + "value"), "osprey_gpui")

    def test_transport_cleanup_preserves_failure_and_reports_teardown_errors(self):
        source = (ROOT / "src/osprey/examples/casting/mobile-transport-test.sh").read_text()
        cleanup = source[source.index("cleanup() {"):source.index("trap cleanup EXIT")]
        for initial, android, expected in [(0, False, 0), (47, False, 47), (0, True, 1), (47, True, 47)]:
            with self.subTest(initial=initial, android=android), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                work = root / "work"; work.mkdir()
                adb = root / "adb"
                adb.write_text('#!/bin/sh\nexit 9\n'); adb.chmod(0o755)
                setup = 'set -euo pipefail\nserver_pid=; remote=; mapped_ports=()\n'
                if android:
                    setup += 'adb="$FAKE_ADB"; serial=test; remote=/probe; mapped_ports=(1234)\n'
                script = setup + cleanup + 'trap cleanup EXIT\nexit "$INITIAL_STATUS"\n'
                env = dict(os.environ, work=str(work), FAKE_ADB=str(adb), INITIAL_STATUS=str(initial))
                result = subprocess.run(["bash", "-c", script], env=env, capture_output=True, text=True)
                self.assertEqual(result.returncode, expected, result.stderr)
                self.assertFalse(work.exists(), "Local probe files must be removed on every path")

    def test_receiver_rejects_malformed_payloads(self):
        module = casting_tool("mobile_receiver")
        for payload in ("", "[]", "true", "null", '\"bad\"'):
            log = io.StringIO()
            frame = module.build_frame("sender", "receiver", "urn:x-cast:test", payload)
            self.assertFalse(module.deliver({}, frame[4:], mock.Mock(), log), payload)
            self.assertIn('"event": "error"', log.getvalue())
        frame = module.build_frame("sender", "receiver", module.NS_PREFIX + "tp.heartbeat", '{"type":"PING"}')
        self.assertTrue(module.deliver({}, frame[4:], mock.Mock(), io.StringIO()))

    def test_receiver_cannot_hide_failed_port_cleanup(self):
        module = casting_tool("mobile_receiver")
        with mock.patch.object(module.subprocess, "run", return_value=mock.Mock(returncode=0, stdout="", stderr="")):
            self.assertEqual(module.adb("stub", ["forward", "--remove", "tcp:12345"]), ("", None))
        with mock.patch.dict(os.environ, {"OSPREY_ANDROID_SERIAL": "stub"}), \
             mock.patch.object(module, "forward_port", return_value=(12345, None)), \
             mock.patch.object(module, "http_checks", return_value=None), \
             mock.patch.object(module, "adb", return_value=("", "cannot remove forward")) as adb:
            self.assertEqual(module.verify_media("http://127.0.0.1:8000/media"), "cannot remove forward")
            adb.assert_called_once_with("stub", ["forward", "--remove", "tcp:12345"])
            module.http_checks.return_value = "bad media"
            self.assertEqual(module.verify_media("http://127.0.0.1:8000/media"), "bad media; cannot remove forward")
            self.assertEqual(adb.call_count, 2)
            module.forward_port.reset_mock()
            self.assertEqual(module.verify_media("http://10.0.2.16:8000/media"), "bad media"); module.forward_port.assert_not_called()
    def test_golden_gate_rejects_a_missing_small_control(self):
        module = casting_tool("mobile_test")
        baseline = (256, 256, bytes([250, 245, 240]) * 65536)
        pixels = bytearray(baseline[2])
        for y in range(10):
            for x in range(10): pixels[(y * 256 + x) * 3:(y * 256 + x) * 3 + 3] = bytes([0, 0, 0])
        changed = (256, 256, pixels)
        self.assertLess(module.picture_delta(changed, baseline), 0.005)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            references = root / "goldens/android"
            references.mkdir(parents=True)
            (references / "256-ready.png").touch()
            with mock.patch.object(module, "APP", root), mock.patch.object(module, "player_pixels", return_value=changed), \
                 mock.patch.object(module, "png_pixels", return_value=baseline), mock.patch.object(module, "png_write"), \
                 mock.patch.dict(os.environ, {"OSPREY_UPDATE_GOLDENS": "0"}):
                with self.assertRaises(AssertionError): module.verify_pictures("android", root)

    def test_appearance_seed_writes_exact_utf8_to_shared_gpui_file(self):
        import base64
        import shlex
        module = casting_tool("mobile_test")
        path = "files/.local/share/osprey/osprey/casting-appearance.txt"
        for skin in ("removed-skin", "", "poolside & <v2>' e\u0301"):
            expected, calls = skin.encode("utf-8"), []
            def adb(*args, binary=False):
                calls.append(args)
                if args == ("exec-out", "run-as", "PACKAGE", "cat", path):
                    self.assertTrue(binary)
                    return expected
                return ""
            self.assertIsNone(module.seed_appearance(adb, "PACKAGE", skin))
            self.assertEqual(calls[0], ("shell", "am", "force-stop", "PACKAGE"))
            self.assertTrue(all("shared_prefs" not in arg and "casting.selected-skin" not in arg
                                for call in calls for arg in call), calls)
            writes = [call for call in calls if call[:3] == ("shell", "run-as", "PACKAGE") and call[3:5] == ("sh", "-c")]
            self.assertEqual(len(writes), 1, calls)
            self.assertEqual(writes[0][3:5], ("sh", "-c"))
            command = writes[0][5]
            self.assertIn(path, command)
            encoded = base64.b64encode(expected).decode()
            self.assertIn(encoded, shlex.split(command))
            self.assertEqual(base64.b64decode(encoded), expected)
            self.assertEqual(calls[-1], ("exec-out", "run-as", "PACKAGE", "cat", path))

    def test_pixel_threshold_rejects_changed_controls_and_wrong_dimensions(self):
        # [CAST-MOBILE-DELIVERY] Isolate the pixel oracle from simulator/ffmpeg availability.
        module = casting_tool("mobile_test")
        baseline = (20, 10, bytes([250, 245, 240]) * 200)
        self.assertEqual(module.picture_delta(baseline, baseline), 0)
        for channel in range(3):
            for delta, expected in [(24, 0), (25, 0.005)]:
                pixels = bytearray(baseline[2])
                pixels[channel] -= delta
                with self.subTest(channel=channel, delta=delta):
                    self.assertEqual(module.picture_delta((20, 10, pixels), baseline), expected)
            pixels = bytearray(baseline[2])
            for at in range(channel, 30, 3): pixels[at] = 0
            self.assertGreater(module.picture_delta((20, 10, pixels), baseline), 0.005)
        with self.assertRaises(AssertionError): module.picture_delta((10, 20, baseline[2]), baseline)

if __name__ == "__main__":
    subprocess.run([sys.executable, str(ROOT / "src/osprey/examples/casting/android/test.py"), "--reader-contracts"], check=True)
    unittest.main()
