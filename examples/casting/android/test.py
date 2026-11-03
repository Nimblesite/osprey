#!/usr/bin/env python3
"""[CAST-MOBILE-DELIVERY] Real Android picker, file import, accessibility and restart."""
import json
import io
import zipfile
import base64
import os
from pathlib import Path
import re
import subprocess
import shlex
import sys
import unittest
from unittest import mock
import tempfile
import time
from xml.etree import ElementTree

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from mobile_test import BINDING_CHECK, CastingArtifactContracts, android_java, invalid_appearance, native_picker, stale_volume_popup, verify_android_package, wait_native_skin

ROOT = Path(__file__).resolve().parents[3]
SDK = Path(os.environ.get("ANDROID_HOME", Path.home() / "Library/Android/sdk"))
SERIAL = os.environ.get("OSPREY_ANDROID_SERIAL", "")
ADB = [str(SDK / "platform-tools/adb"), "-s", SERIAL]
PACKAGE = "org.ospreylang.casting"
WINDOW_PATH = "/sdcard/casting-test-window.xml"


def adb(*arguments, binary=False):
    if not SERIAL: raise ValueError("Set OSPREY_ANDROID_SERIAL to the Android device under test")
    if arguments[:1] == ("shell",): arguments = ("shell", shlex.join(arguments[1:]))
    return subprocess.check_output(ADB + list(arguments), text=not binary)


def index_audio(path):
    canonical = adb("shell", "readlink", "-f", path).strip()
    assert canonical.startswith("/"), f"Cannot resolve the audio fixture: {canonical}"
    result = adb("shell", "content", "call", "--uri", "content://media", "--method", "scan_file", "--arg", canonical)
    assert result.startswith("Result: Bundle[") and "android.intent.extra.STREAM=" in result and "android.intent.extra.STREAM=null" not in result, result
    print("PASS: Android MediaStore indexed the audio fixture before document selection", flush=True)


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


def read_window():
    for attempt in range(3):
        try: return window()
        except subprocess.CalledProcessError as error:
            if error.returncode != 137 or attempt == 2: raise
            print("RETRY: Android automation stopped; requesting a fresh hierarchy", file=sys.stderr, flush=True)


def picker_drawer(tree):
    return None if tree is None else next((node for node in tree.iter("node") if node.get("resource-id", "").endswith(":id/drawer_roots")), None)


def nodes(interactive=False):
    tree = read_window()
    if tree is None: return []
    drawer = picker_drawer(tree) if interactive else None
    return list((tree if drawer is None else drawer).iter("node"))


def wait_rotation(expected):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        tree = read_window()
        if tree is not None and tree.get("rotation") == str(expected): return
    raise AssertionError(f"Native display did not rotate to {expected}")


def find(label, timeout=20, interactive=False):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        matches = [node for node in nodes(interactive) if label in (node.get("text"), node.get("content-desc"))]
        # The drawer is the last layer; its Downloads row can share the toolbar's label.
        if matches: return matches[-1]
        time.sleep(0.2)
    raise AssertionError(f"Native control never appeared: {label}")


def tap(label):
    print(f"TAP: {label}", flush=True)
    tap_node(find(label, interactive=True))


def tap_node(node):
    x1, y1, x2, y2 = map(int, re.findall(r"\d+", node.attrib["bounds"]))
    assert x2 > x1 and y2 > y1, node.attrib
    print(f"POINTER: {node.get('bounds')} {node.get('resource-id', '')}", flush=True)
    adb("shell", "input", "tap", str((x1 + x2) // 2), str((y1 + y2) // 2))


def open_roots():
    deadline, requested = time.monotonic() + 20, False
    while time.monotonic() < deadline:
        tree = read_window()
        if picker_drawer(tree) is not None: return
        if tree is not None and not requested:
            controls = [node for node in tree.iter("node") if node.get("content-desc") == "Show roots"]
            if controls:
                print("TAP: Show roots", flush=True)
                tap_node(controls[-1]); requested = True
        time.sleep(0.2)
    raise AssertionError("The native picker roots drawer never opened")


def select_root(label):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        tree = read_window()
        if tree is None: continue
        drawer = picker_drawer(tree)
        if drawer is None:
            assert any(node.get("text") == label for node in tree.iter("node")), "Picker opened the wrong directory"
            return
        choices = [node for node in drawer.iter("node") if node.get("text") == label and node.get("enabled", "true") == "true"]
        if choices: tap_node(choices[-1])
        time.sleep(0.2)
    raise AssertionError(f"The native picker did not open {label}")


def import_track():
    scene_tap("Open file\u2026")
    open_roots()
    select_root("Downloads")
    tap("Casting mobile test.mp3")
    wait_focus()


APP = ROOT / "examples/casting"
OUTPUT = APP / "build/android-test"
LISTENERS = json.loads((APP / "player.machine.json").read_text())["skins"]["sherbet"]["canvas"]
RATES = ["0.75", "1", "1.25", "1.5", "2"]
SCENE_WIDTH, SCENE_HEIGHT, SCENE_ZOOM = LISTENERS["width"], LISTENERS["height"], LISTENERS["zoom"]


def listener(label):
    return next(item for item in LISTENERS["listeners"] if item["label"].startswith(label))


def screen():
    width, height = map(int, re.search(r"(\d+)x(\d+)", adb("shell", "wm", "size")).groups())
    rotation = re.search(r"mRotation=(\d)", adb("shell", "dumpsys", "window", "displays")).group(1)
    return (height, width) if rotation in ("1", "3") else (width, height)


def safe_area():
    """The last safe area the GPUI backend reported, top and bottom, in device pixels."""
    lines = re.findall(r"safe area top (\d+) bottom (\d+) px", adb("logcat", "-d"))
    return tuple(map(int, lines[-1])) if lines else (0, 0)


def fit():
    """The scale and spare scene units hosts/gpui/src/scene.rs draws with, and the safe top."""
    width, height = screen()
    top, bottom = safe_area()
    density = int(re.search(r"(\d+)", adb("shell", "wm", "density")).group(1)) / 160
    usable = height - top - bottom
    scale = min(SCENE_ZOOM * density, width / SCENE_WIDTH, usable / SCENE_HEIGHT)
    return scale, {"spareWidth": width / scale - SCENE_WIDTH, "spareHeight": usable / scale - SCENE_HEIGHT}, top


def measure(value, spare):
    """A listener bound: a number, or a number and `{{spareHeight|scale:0.7}}` terms to add."""
    if isinstance(value, (int, float)): return value
    terms = re.findall(r"\{\{(\w+)\|scale:([\d.]+)\}\}", value)
    plain = re.sub(r"\{\{.*?\}\}", "", value).strip()
    return (float(plain) if plain else 0) + sum(spare[name] * float(factor) for name, factor in terms)


def scene_tap(label, item=None, step=0):
    item = item or listener(label)
    scale, spare, top = fit()
    x = measure(item["x"], spare) + item["w"] / 2
    y = measure(item["y"], spare) + step + item["h"] / 2
    print("TAP:", label, flush=True)
    adb("shell", "input", "tap", str(round(x * scale)), str(round(top + y * scale)))


def scene_slide(label, fraction):
    item = listener(label)
    scale, spare, top = fit()
    x = round((measure(item["x"], spare) + item["w"] * fraction) * scale)
    y = round(top + (measure(item["y"], spare) + item["h"] / 2) * scale)
    print("SLIDE:", label, fraction, flush=True)
    adb("shell", "input", "swipe", str(x), str(y), str(x + 1), str(y), "150")


def records():
    return [json.loads(line) for line in (OUTPUT / "receiver.jsonl").read_text().splitlines()]


def sent(kind):
    return [record["payload"] for record in records() if record.get("event") == "recv" and record["payload"].get("type") == kind]


def wait_sent(kind, count, timeout=25):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if len(sent(kind)) >= count: return sent(kind)[-1]
        time.sleep(0.2)
    raise AssertionError(f"The receiver never got {count} {kind}: {[record for record in records() if record.get('event') == 'recv'][-5:]}")


def wait_focus(timeout=20):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if PACKAGE + "/" in adb("shell", "dumpsys", "window").split("mCurrentFocus", 1)[1].split("\n", 1)[0]: return
        time.sleep(0.2)
    raise AssertionError("The player did not come back to the front")


def shot(name):
    (OUTPUT / name).write_bytes(adb("exec-out", "screencap", "-p", binary=True))


def launch():
    # Always the fixture receiver: a journey must never reach a real TV on this network.
    adb("shell", "am", "force-stop", PACKAGE)
    adb("logcat", "-c")
    adb("shell", "am", "start", "-W", "-n", PACKAGE + "/.CastingActivity", "--es", "testReceiver", os.environ["CAST_TEST_RECEIVER"])
    wait_focus()
    deadline = time.monotonic() + 30
    while safe_area()[0] == 0 and time.monotonic() < deadline: time.sleep(0.3)
    assert safe_area()[0] > 0, "The GPUI backend never reported the safe area"
    time.sleep(5)


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="casting-mobile-") as directory:
        track = Path(directory) / "Casting mobile test.mp3"
        subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "anullsrc=r=44100:cl=mono", "-t", "1", str(track)], check=True)
        adb("push", str(track), "/sdcard/Download/Casting mobile test.mp3")
        index_audio("/sdcard/Download/Casting mobile test.mp3")
    launch()
    shot("ready.png")
    for _ in range(2):
        scene_tap("Open file\u2026")
        find("Show roots")
        adb("shell", "input", "keyevent", "KEYCODE_BACK")
        wait_focus()
        # The host answers the cancelled pick on its background thread before a press means anything.
        time.sleep(2)
    import_track()
    time.sleep(1)
    shot("imported.png")
    scene_tap("Play")
    wait_sent("LOAD", 1)
    time.sleep(1)
    scene_tap("Pause")
    wait_sent("PAUSE", 1)
    time.sleep(1)
    scene_tap("Resume")
    wait_sent("PLAY", 1)
    time.sleep(1)
    volumes = len(sent("SET_VOLUME"))
    scene_slide("Volume", 0.5)
    level = wait_sent("SET_VOLUME", volumes + 1)["volume"]["level"]
    # The listener is wider than the track its thumb travels, so its middle is close to, not at, half.
    assert abs(level - 0.5) < 0.05, level
    seeks = len(sent("SEEK"))
    scene_slide("Position", 0.25)
    position = wait_sent("SEEK", seeks + 1)["currentTime"]
    assert abs(position - 50) < 10, position
    scene_tap("Speed")
    time.sleep(1)
    rate = next(item for item in LISTENERS["listeners"] if item.get("each", {}).get("in") == "rates")
    scene_tap("1.5\u00d7", rate, rate["each"]["dy"] * RATES.index("1.5"))
    assert wait_sent("SET_PLAYBACK_RATE", 1)["playbackRate"] == 1.5
    shot("playing.png")
    previous = adb("shell", "wm", "user-rotation").strip().split()
    try:
        adb("shell", "wm", "user-rotation", "lock", "1")
        time.sleep(4)
        shot("landscape-playing.png")
        scene_tap("Pause")
        wait_sent("PAUSE", 2)
    finally:
        adb("shell", "wm", "user-rotation", *previous)
    time.sleep(3)
    scene_tap("Resume")
    wait_sent("PLAY", 2)
    time.sleep(1)
    scene_tap("Stop")
    wait_sent("STOP", 1)
    time.sleep(1)
    scene_tap("Play")
    wait_sent("LOAD", 2)
    time.sleep(1)
    scene_tap("Stop")
    wait_sent("STOP", 2)
    launch()
    shot("restarted.png")
    print("PASS: Android GPUI player: two picker cancellations, MP3 import, Cast playback, rotation, restart")


class NativeReadContracts(unittest.TestCase):
    def test_native_http_preserves_binary_response_and_quotes_shell_arguments(self):
        import mobile_receiver as receiver
        data = b"HTTP/1.1 206 Partial Content\r\nContent-Length: 4\r\nContent-Type: audio/mpeg\r\n\r\n\x00\xff\r\n"
        request = receiver.urllib.request.Request("http://10.0.2.16:8000/media?x=$(private)", headers={"Range": "bytes=0-3"})
        result = mock.Mock(returncode=0, stdout=data, stderr=b"")
        with mock.patch.dict(os.environ, {"CAST_TEST_NATIVE_JAR": "/tmp/fixture's path.jar"}), mock.patch.object(receiver.subprocess, "run", return_value=result) as run:
            with receiver.native_http("stub", request, 3) as response:
                self.assertEqual((response.status, response.headers.get_content_type(), response.read()), (206, "audio/mpeg", b"\x00\xff\r\n"))
            expected = ["env", "CLASSPATH=/tmp/fixture's path.jar", "app_process", "/", "CastingNsdPublisher$MediaCheck", "GET", request.full_url, "bytes=0-3"]
            self.assertEqual(shlex.split(run.call_args.args[0][-1]), expected)
            result.stdout = b"invalid HTTP"
            with self.assertRaises(receiver.http.client.HTTPException): receiver.native_http("stub", request, 3)

    def test_transient_automation_failure_requests_a_new_snapshot(self):
        failure = subprocess.CalledProcessError(137, "uiautomator", output="interrupted before dump")
        good = ElementTree.fromstring('<hierarchy><node text="Play"/></hierarchy>')
        with mock.patch(__name__ + ".window", side_effect=[failure, good]) as query:
            self.assertEqual([node.get("text") for node in nodes()], ["Play"])
            self.assertEqual(query.call_count, 2)
        for code, count in [(1, 1), (137, 3)]:
            with self.subTest(code=code), mock.patch(__name__ + ".window", side_effect=subprocess.CalledProcessError(code, "uiautomator")) as query:
                with self.assertRaises(subprocess.CalledProcessError): nodes()
                self.assertEqual(query.call_count, count)

    def test_import_waits_until_the_root_drawer_really_closes(self):
        drawer = ElementTree.fromstring('<hierarchy><node resource-id="picker:id/drawer_roots"><node text="Downloads" bounds="[0,100][200,200]"/></node></hierarchy>')
        directory = ElementTree.fromstring('<hierarchy><node text="Downloads"/></hierarchy>')
        selections = []
        def select(_): selections.append("Downloads")
        def tap_label(label):
            if label == "Downloads": select(None)
            if label == "Casting mobile test.mp3": self.assertEqual(len(selections), 2, "File is still covered by the roots drawer")
        def snapshot(): return drawer if len(selections) < 2 else directory
        with mock.patch(__name__ + ".scene_tap"), mock.patch(__name__ + ".wait_focus"), mock.patch(__name__ + ".open_roots"), mock.patch(__name__ + ".find"), mock.patch(__name__ + ".tap", side_effect=tap_label), mock.patch(__name__ + ".tap_node", side_effect=select), mock.patch(__name__ + ".window", side_effect=snapshot), mock.patch(__name__ + ".time.sleep"):
            import_track()

    def test_shell_keeps_audio_paths_as_single_arguments(self):
        for path in ("clip.mp3", "/sdcard/Download/Now playing 'mix'.mp3", "/sdcard/Download/海 🌊.mp3"):
            with self.subTest(path=path), mock.patch(__name__ + ".SERIAL", "device"), mock.patch(__name__ + ".ADB", ["adb", "-s", "device"]), mock.patch(__name__ + ".subprocess.check_output", return_value="") as command:
                adb("shell", "content", "call", "--arg", path)
                arguments = command.call_args.args[0]
                self.assertEqual(arguments[:4], ["adb", "-s", "device", "shell"])
                self.assertEqual(shlex.split(" ".join(arguments[4:])), ["content", "call", "--arg", path])

    def test_picker_waits_for_the_drawer_not_the_directory_toolbar(self):
        toolbar = ElementTree.fromstring('<hierarchy><node text="Downloads"/><node content-desc="Show roots" bounds="[0,0][100,100]"/></hierarchy>')
        drawer = ElementTree.fromstring('<hierarchy><node resource-id="com.google.android.documentsui:id/drawer_roots"><node text="Downloads"/></node></hierarchy>')
        with mock.patch(__name__ + ".window", side_effect=[None, toolbar, drawer]) as snapshots, mock.patch(__name__ + ".adb") as commands, mock.patch(__name__ + ".time.sleep"):
            open_roots()
            self.assertEqual(snapshots.call_count, 3)
            commands.assert_called_once_with("shell", "input", "tap", "50", "50")
        with mock.patch(__name__ + ".window", return_value=drawer), mock.patch(__name__ + ".adb") as commands:
            open_roots(); commands.assert_not_called()

    def test_pointer_never_selects_a_file_behind_the_picker_drawer(self):
        covered = ElementTree.fromstring('<hierarchy><node text="track.mp3" bounds="[10,10][60,60]"/><node resource-id="com.google.android.documentsui:id/drawer_roots"><node text="Downloads" bounds="[0,0][100,100]"/></node></hierarchy>')
        visible = ElementTree.fromstring('<hierarchy><node text="track.mp3" bounds="[100,100][200,200]"/></hierarchy>')
        with mock.patch(__name__ + ".window", side_effect=[covered, visible]) as snapshots, mock.patch(__name__ + ".adb") as commands, mock.patch(__name__ + ".time.sleep"):
            tap("track.mp3")
            self.assertEqual(snapshots.call_count, 2)
            commands.assert_called_once_with("shell", "input", "tap", "150", "150")

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
