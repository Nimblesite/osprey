#!/usr/bin/env python3
"""Regressions for the unfinished iOS and Android casting tools. [CAST-MOBILE-GPUI]

Mobile casting is not finished, so CI does not run these: `make -C src/toolchain/targets/tests casting-mobile-tools` does.
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
                result = subprocess.run(["/usr/bin/make", "-f", str(ROOT / "src/toolchain/targets/ios/tests/Makefile"), target],
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
