#!/usr/bin/env python3
"""[CAST-MOBILE-DELIVERY] Real Android picker, file import, accessibility and restart."""
import json
import base64
import os
from pathlib import Path
import re
import subprocess
import sys
import unittest
from unittest import mock
import tempfile
import time
from xml.etree import ElementTree

ROOT = Path(__file__).resolve().parents[3]
SDK = Path(os.environ.get("ANDROID_HOME", Path.home() / "Library/Android/sdk"))
SERIAL = os.environ.get("OSPREY_ANDROID_SERIAL", "")
ADB = [str(SDK / "platform-tools/adb"), "-s", SERIAL]
PACKAGE = "org.ospreylang.casting"
WINDOW_PATH = "/sdcard/casting-test-window.xml"


# Isolate the event-binding guard on Android's real JSON/runtime implementation.
BINDING_CHECK = r"""
import java.lang.reflect.Method;
import org.json.JSONArray;
import org.json.JSONObject;
public final class CastingBindingCheck {
    private static void require(boolean ok, String message) {
        if (!ok) throw new AssertionError(message);
    }
    public static void main(String[] arguments) throws Exception {
        Method matches = Class.forName("org.ospreylang.casting.Resident")
            .getDeclaredMethod("containsBinding", Object.class, String.class);
        matches.setAccessible(true);
        JSONObject expected = new JSONObject("{\"kind\":\"picker\",\"label\":\"Speed\",\"selected\":\"1\",\"event\":8,\"enabled\":true,\"choices\":[{\"key\":\"1.5\",\"label\":\"1.5×\"}]}");
        String descriptor = expected.toString();
        JSONObject tree = new JSONObject().put("body", new JSONObject().put("children", new JSONArray().put(expected)));
        require(Boolean.TRUE.equals(matches.invoke(null, tree, descriptor)), "unchanged nested binding");
        require(Boolean.TRUE.equals(matches.invoke(null, new JSONObject(tree.toString()), descriptor)), "independent frame snapshot");
        String[] fields = {"event", "label", "selected", "enabled", "choices", "kind"};
        Object[] values = {9, "Volume", "1.5", false, new JSONArray(), "button"};
        for (int i = 0; i < fields.length; i++) {
            JSONObject changed = new JSONObject(descriptor).put(fields[i], values[i]);
            require(Boolean.FALSE.equals(matches.invoke(null, changed, descriptor)), "changed " + fields[i]);
        }
        require(Boolean.FALSE.equals(matches.invoke(null, JSONObject.NULL, descriptor)), "removed binding");
        System.out.println("PASS: Android binding snapshots reject changed ordinals, labels, selection, enabled state, choices, kind and removal");
    }
}
"""


def adb(*arguments, binary=False):
    if not SERIAL: raise ValueError("Set OSPREY_ANDROID_SERIAL to the Android device under test")
    return subprocess.check_output(ADB + list(arguments), text=not binary)


def sdk_tool(pattern):
    paths = list(SDK.glob(pattern))
    assert paths, f"Missing Android SDK tool: {pattern}"
    return max(paths, key=lambda path: tuple(map(int, re.findall(r"\d+", path.parent.name))))


def bindings():
    platform = sdk_tool("platforms/*/android.jar")
    compiler = sdk_tool("build-tools/*/d8")
    installed = adb("shell", "pm", "path", PACKAGE).strip().splitlines()
    assert len(installed) == 1 and installed[0].startswith("package:"), installed
    remote = "/data/local/tmp/osprey-casting-bindings.jar"
    with tempfile.TemporaryDirectory(prefix="casting-bindings-") as directory:
        work = Path(directory)
        source = work / "CastingBindingCheck.java"; source.write_text(BINDING_CHECK)
        subprocess.run(["javac", "-Xlint:all", "-Werror", "-classpath", str(platform), "-d", str(work), str(source)], check=True)
        archive = work / "check.jar"
        subprocess.run([str(compiler), "--lib", str(platform), "--output", str(archive), str(work / "CastingBindingCheck.class")], check=True)
        adb("push", str(archive), remote)
        try:
            adb("shell", "chmod", "444", remote)
            print(adb("shell", "env", "CLASSPATH=" + installed[0][8:] + ":" + remote, "app_process", "/", "CastingBindingCheck"), flush=True)
        finally:
            adb("shell", "rm", "-f", remote)


def window():
    # A completed dump can precede UiAutomation's Binder teardown crash. Never
    # accept a previous file, an incomplete dump or another command failure.
    adb("shell", "rm", "-f", WINDOW_PATH)
    completed = "dumped to: " + WINDOW_PATH
    try:
        response = adb("shell", "uiautomator", "dump", WINDOW_PATH)
    except subprocess.CalledProcessError as error:
        if error.returncode != 137 or completed not in (error.output or ""): raise
        response = error.output
        print("RECOVERED: UiAutomation exited 137 after completing a fresh hierarchy dump", file=sys.stderr, flush=True)
    if completed not in response: return None
    return ElementTree.fromstring(adb("exec-out", "cat", WINDOW_PATH))


def nodes():
    tree = window()
    return list(tree.iter("node")) if tree is not None else []


def wait_rotation(expected):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        tree = window()
        if tree is not None and tree.get("rotation") == str(expected): return
    raise AssertionError(f"Native display did not rotate to {expected}")


def find(label, timeout=20):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        matches = [node for node in nodes() if label in (node.get("text"), node.get("content-desc"))]
        # The drawer is the last layer; its Downloads row can share the toolbar's label.
        if matches: return matches[-1]
        time.sleep(0.2)
    raise AssertionError(f"Native control never appeared: {label}")


def tap(label):
    print(f"TAP: {label}", flush=True)
    node = find(label)
    x1, y1, x2, y2 = map(int, re.findall(r"\d+", node.attrib["bounds"]))
    assert x2 > x1 and y2 > y1, node.attrib
    adb("shell", "input", "tap", str((x1 + x2) // 2), str((y1 + y2) // 2))


def slide(label, fraction):
    node = find(label)
    assert node.get("class") == "android.widget.SeekBar", node.attrib
    x1, y1, x2, y2 = map(int, re.findall(r"\d+", node.attrib["bounds"]))
    x = round(x1 + (x2 - x1) * fraction)
    y = (y1 + y2) // 2
    adb("shell", "input", "swipe", str((x1 + x2) // 2), str(y), str(x), str(y), "150")


def focus(label):
    for _ in range(12):
        adb("shell", "input", "keyevent", "KEYCODE_TAB")
        focused = [item for item in nodes() if item.get("focused") == "true"]
        if any(item.get("content-desc") == label for item in focused): return
    raise AssertionError(f"{label} never received native keyboard focus")


def keyboard_picker():
    focus("Open file…")
    adb("shell", "input", "keyevent", "KEYCODE_ENTER")
    find("Show roots")
    adb("shell", "input", "keyevent", "KEYCODE_BACK")
    find("Open file…")
    print("PASS: native Tab focus and Enter open the Android file picker", flush=True)


def playback(output):
    tap("Play")
    find("Pause")
    assert find("Position").get("class") == "android.widget.SeekBar"
    assert find("Volume").get("class") == "android.widget.SeekBar"
    tap("Pause")
    tap("Resume")
    find("Pause")
    previous = adb("shell", "wm", "user-rotation").strip().split()
    assert previous and previous[0] in ("free", "lock"), previous
    try:
        adb("shell", "wm", "user-rotation", "lock", "1")
        wait_rotation(1)
        find("Pause")
        assert find("Volume").get("class") == "android.widget.SeekBar"
        tap("Pause")
        tap("Resume")
        find("Pause")
        slide("Volume", 0.5)
        (output / "landscape-playing.png").write_bytes(adb("exec-out", "screencap", "-p", binary=True))
    finally:
        adb("shell", "wm", "user-rotation", *previous)
    wait_rotation(0)
    find("Pause")
    slide("Position", 0.25)
    tap("Speed: 1×")
    tap("1.5×")
    find("Speed: 1.5×")
    (output / "playing.png").write_bytes(adb("exec-out", "screencap", "-p", binary=True))
    focus("Volume")
    adb("shell", "input", "keyevent", "KEYCODE_DPAD_RIGHT")
    (output / "keyboard-volume.png").write_bytes(adb("exec-out", "screencap", "-p", binary=True))
    tap("Stop")
    find("Play")
    tap("Play")
    find("Pause")
    tap("Stop")
    find("Play")


def launch(skin="sherbet"):
    adb("shell", "am", "force-stop", PACKAGE)
    fixture = os.environ.get("CAST_TEST_RECEIVER", base64.b64encode(b"\n").decode())
    adb("shell", "am", "start", "-W", "-f", "0x10008000", "-n", PACKAGE + "/.CastingActivity", "--ez", "captureEnvelope", "true", "--es", "testReceiver", fixture, "--es", "skin", skin)
    assert find("Open file…").get("class") == "android.widget.Button"
    assert find("Play").get("class") == "android.widget.Button"
    adb("shell", "input", "tap", "10", "150")
    if os.environ.get("CAST_TEST_RECEIVER"): find("Receiver: Test receiver" if skin == "sherbet" else "Receiver")


def import_track():
    tap("Open file…")
    tap("Show roots")
    tap("Downloads")
    tap("Casting mobile test.mp3")
    find("Open file…")


def alternate_playback(output):
    tap("Skin"); tap("Midnight Mint")
    import_track()
    tap("Play"); find("Pause")
    tap("Pause"); find("Resume")
    tap("Forward 10 seconds")
    tap("Volume 100%"); tap("50%")
    tap("Speed"); tap("1.5×")
    tap("Skin"); tap("Poolside Pop")
    find("Resume")
    find("Casting mobile test")
    tap("Back 10 seconds")
    tap("Volume 50%"); tap("25%")
    tap("Speed"); tap("0.75×")
    tap("Resume"); find("Pause")
    tap("Appearance"); tap("Sherbet Signal"); find("Pause")
    tap("Appearance"); tap("Poolside Pop"); find("Pause")
    tap("Speed"); tap("1.25×")
    tap("Pause"); find("Resume")
    (output / "poolside-paused.png").write_bytes(adb("exec-out", "screencap", "-p", binary=True))
    tap("Stop"); find("Play")
    tap("Play"); find("Pause")
    tap("Stop"); find("Play")
    print("PASS: native alternate skins preserve playback, seek, volume, rate and replay", flush=True)


def main():
    output = ROOT / "examples/casting/build/android-test"
    output.mkdir(parents=True, exist_ok=True)
    bindings()
    with tempfile.TemporaryDirectory(prefix="casting-mobile-") as directory:
        track = Path(directory) / "Casting mobile test.mp3"
        subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "anullsrc=r=44100:cl=mono", "-t", "1", str(track)], check=True)
        adb("push", str(track), "/sdcard/Download/Casting mobile test.mp3")
        launch()
        tap("Appearance"); tap("Midnight Mint")
        find("Skin")
        snapshot = json.loads(adb("exec-out", "run-as", PACKAGE, "cat", "files/test-scene.json"))
        assert snapshot["node"]["kind"] == "themed", snapshot
        tap("Skin")
        tap("Poolside Pop")
        find("Skin")
        snapshot = json.loads(adb("exec-out", "run-as", PACKAGE, "cat", "files/test-scene.json"))
        assert any(token["name"] == "background" and token["light"] == "#D9EEFF" for token in snapshot["node"]["theme"]["tokens"]), snapshot
        (output / "poolside-ready.png").write_bytes(adb("exec-out", "screencap", "-p", binary=True))
        if os.environ.get("CAST_TEST_RECEIVER"): alternate_playback(output)
        launch()
        (output / "ready.png").write_bytes(adb("exec-out", "screencap", "-p", binary=True))
        keyboard_picker()
        for _ in range(2):
            tap("Open file…")
            find("Show roots")
            adb("shell", "input", "keyevent", "KEYCODE_BACK")
            assert find("Open file…").get("clickable") == "true"
        import_track()
        snapshot = json.loads(adb("exec-out", "run-as", PACKAGE, "cat", "files/test-scene.json"))
        text = json.dumps(snapshot, ensure_ascii=False)
        assert "Casting mobile test" in text, text[:1000]
        assert snapshot["node"]["kind"] == "scene"
        controls = snapshot["node"]["access"]
        assert any(item["label"] == "Play" for item in controls), controls
        (output / "imported.png").write_bytes(adb("exec-out", "screencap", "-p", binary=True))
        if os.environ.get("CAST_TEST_RECEIVER"):
            playback(output)
        launch()
        assert find("Open file…").get("clickable") == "true"
        (output / "restarted.png").write_bytes(adb("exec-out", "screencap", "-p", binary=True))
    print("PASS: Android native controls, two picker cancellations, MP3 import, scene title and restart")


class NativeReadContracts(unittest.TestCase):
    def queries(self, completed, xml="broken XML"):
        def answer(*arguments):
            if arguments[:2] == ("shell", "rm"): return ""
            if arguments[0] == "exec-out":
                if isinstance(xml, Exception): raise xml
                return xml
            if isinstance(completed, Exception): raise completed
            return completed
        return mock.patch(__name__ + ".adb", side_effect=answer)

    def test_completed_snapshot_survives_android_automation_teardown_crash(self):
        message = "UI hierchary dumped to: /sdcard/casting-test-window.xml"
        xml = '<hierarchy rotation="0"><node text="Pause" /></hierarchy>'
        for completed in [message, subprocess.CalledProcessError(137, "uiautomator", output=message)]:
            with self.subTest(completed=completed), self.queries(completed, xml) as read:
                result = window()
                self.assertEqual(result.find("node").get("text"), "Pause")
                self.assertEqual(read.call_args_list, [mock.call("shell", "rm", "-f", "/sdcard/casting-test-window.xml"), mock.call("shell", "uiautomator", "dump", "/sdcard/casting-test-window.xml"), mock.call("exec-out", "cat", "/sdcard/casting-test-window.xml")])

    def test_incomplete_or_failed_queries_never_use_a_previous_snapshot(self):
        for code, output in [(137, "null root"), (137, "dumped to: /sdcard/other.xml"), (1, "dumped to:"), (139, "dumped to:")]:
            failure = subprocess.CalledProcessError(code, "uiautomator", output=output)
            with self.subTest(code=code), self.queries(failure) as read:
                with self.assertRaises(subprocess.CalledProcessError): window()
                self.assertEqual(read.call_count, 2)
        with self.queries("null root") as read:
            self.assertIsNone(window()); self.assertEqual(read.call_count, 2)
        with self.queries("dumped to: " + WINDOW_PATH):
            with self.assertRaises(ElementTree.ParseError): window()
        failure = subprocess.CalledProcessError(1, "cat", output="missing snapshot")
        with self.queries("dumped to: " + WINDOW_PATH, failure):
            with self.assertRaises(subprocess.CalledProcessError): window()


if __name__ == "__main__":
    if sys.argv[1:] == ["--reader-contracts"]:
        unittest.main(argv=[sys.argv[0]])
    try:
        main()
    except Exception:
        output = ROOT / "examples/casting/build/android-test"
        (output / "failed.png").write_bytes(adb("exec-out", "screencap", "-p", binary=True))
        (output / "failed-envelope.json").write_text(adb("exec-out", "run-as", PACKAGE, "cat", "files/test-scene.json"))
        raise
