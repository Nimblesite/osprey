#!/usr/bin/env python3
"""[CAST-MOBILE-DELIVERY] Real Android picker, file import, accessibility and restart."""
import json
import base64
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
from xml.etree import ElementTree

ROOT = Path(__file__).resolve().parents[3]
SDK = Path(os.environ.get("ANDROID_HOME", Path.home() / "Library/Android/sdk"))
SERIAL = os.environ["OSPREY_ANDROID_SERIAL"]
ADB = [str(SDK / "platform-tools/adb"), "-s", SERIAL]
PACKAGE = "org.ospreylang.casting"


def adb(*arguments, binary=False):
    return subprocess.check_output(ADB + list(arguments), text=not binary)


def nodes():
    response = adb("shell", "uiautomator", "dump", "/sdcard/casting-test-window.xml")
    if "dumped to:" not in response: return []
    return list(ElementTree.fromstring(adb("exec-out", "cat", "/sdcard/casting-test-window.xml")).iter("node"))


def find(label, timeout=20):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        matches = [node for node in nodes() if label in (node.get("text"), node.get("content-desc"))]
        # The drawer is the last layer; its Downloads row can share the toolbar's label.
        if matches: return matches[-1]
        time.sleep(0.2)
    raise AssertionError(f"Native control never appeared: {label}")


def tap(label):
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


def keyboard_picker():
    for _ in range(8):
        adb("shell", "input", "keyevent", "KEYCODE_TAB")
        focused = [item for item in nodes() if item.get("focused") == "true"]
        if any(item.get("content-desc") == "Open file…" for item in focused):
            adb("shell", "input", "keyevent", "KEYCODE_ENTER")
            find("Show roots")
            adb("shell", "input", "keyevent", "KEYCODE_BACK")
            find("Open file…")
            print("PASS: native Tab focus and Enter open the Android file picker")
            return
    raise AssertionError("Open file never received native keyboard focus")


def playback(output):
    tap("Play")
    find("Pause")
    assert find("Position").get("class") == "android.widget.SeekBar"
    assert find("Volume").get("class") == "android.widget.SeekBar"
    tap("Pause")
    tap("Resume")
    find("Pause")
    slide("Volume", 0.5)
    slide("Position", 0.25)
    tap("Speed: 1×")
    tap("1.5×")
    find("Speed: 1.5×")
    (output / "playing.png").write_bytes(adb("exec-out", "screencap", "-p", binary=True))
    tap("Stop")
    find("Play")
    tap("Play")
    find("Pause")
    tap("Stop")
    find("Play")


def launch():
    adb("shell", "am", "force-stop", PACKAGE)
    fixture = os.environ.get("CAST_TEST_RECEIVER", base64.b64encode(b"\n").decode())
    adb("shell", "am", "start", "-W", "-f", "0x10008000", "-n", PACKAGE + "/.CastingActivity", "--ez", "captureEnvelope", "true", "--es", "testReceiver", fixture)
    assert find("Open file…").get("class") == "android.widget.Button"
    assert find("Play").get("class") == "android.widget.Button"
    adb("shell", "input", "tap", "10", "150")
    if os.environ.get("CAST_TEST_RECEIVER"): find("Receiver: Test receiver")


def main():
    output = ROOT / "examples/casting/build/android-test"
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="casting-mobile-") as directory:
        track = Path(directory) / "Casting mobile test.mp3"
        subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "anullsrc=r=44100:cl=mono", "-t", "1", str(track)], check=True)
        adb("push", str(track), "/sdcard/Download/Casting mobile test.mp3")
        launch()
        (output / "ready.png").write_bytes(adb("exec-out", "screencap", "-p", binary=True))
        keyboard_picker()
        for _ in range(2):
            tap("Open file…")
            find("Show roots")
            adb("shell", "input", "keyevent", "KEYCODE_BACK")
            assert find("Open file…").get("clickable") == "true"
        tap("Open file…")
        tap("Show roots")
        tap("Downloads")
        tap("Casting mobile test.mp3")
        find("Open file…")
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


if __name__ == "__main__":
    main()
