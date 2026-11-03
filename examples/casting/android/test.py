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
from mobile_test import BINDING_CHECK, CastingArtifactContracts, android_java, native_picker, stale_volume_popup, verify_android_package

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


def touch_regions(labels):
    density = int(re.findall(r"density: (\d+)", adb("shell", "wm", "density"))[-1]) / 160
    for label in labels:
        node = find(label)
        x1, y1, x2, y2 = map(int, re.findall(r"\d+", node.attrib["bounds"]))
        assert min(x2 - x1, y2 - y1) >= 48 * density, (label, node.attrib["bounds"], density)
    print("PASS: native controls meet the 48-dp minimum touch region", flush=True)


def index_audio(path):
    canonical = adb("shell", "readlink", "-f", path).strip()
    assert canonical.startswith("/"), f"Cannot resolve the audio fixture: {canonical}"
    result = adb("shell", "content", "call", "--uri", "content://media", "--method", "scan_file", "--arg", canonical)
    assert result.startswith("Result: Bundle[") and "android.intent.extra.STREAM=" in result and "android.intent.extra.STREAM=null" not in result, result
    print("PASS: Android MediaStore indexed the audio fixture before document selection", flush=True)



def packaged_assets(apk):
    with zipfile.ZipFile(io.BytesIO(adb("exec-out", "cat", apk, binary=True))) as archive:
        verify_android_package(archive)


def bindings():
    installed = adb("shell", "pm", "path", PACKAGE).strip().splitlines()
    assert len(installed) == 1 and installed[0].startswith("package:"), installed
    packaged_assets(installed[0][8:])
    with tempfile.TemporaryDirectory(prefix="casting-bindings-") as directory:
        with android_java(SDK, ADB, BINDING_CHECK, "CastingBindingCheck", Path(directory)) as remote:
            print(adb("shell", "env", "CLASSPATH=" + installed[0][8:] + ":" + remote, "app_process", "/", "CastingBindingCheck"), flush=True)


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
    touch_regions(["Stop", "Position", "Volume", "Speed: 1×"])
    tap("Pause")
    tap("Resume")
    find("Pause")
    previous = adb("shell", "wm", "user-rotation").strip().split()
    assert previous and previous[0] in ("free", "lock"), previous
    try:
        adb("shell", "wm", "user-rotation", "lock", "1")
        wait_rotation(1)
        touch_regions(["Pause", "Stop", "Position", "Volume", "Open file…", "Speed: 1×"])
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


def launch(skin="sherbet", fixture=None, native=False):
    adb("shell", "am", "force-stop", PACKAGE)
    fixture = fixture if fixture is not None else os.environ.get("CAST_TEST_RECEIVER", base64.b64encode(b"\n").decode())
    arguments = ["shell", "am", "start", "-W", "-f", "0x10008000", "-n", PACKAGE + "/.CastingActivity", "--ez", "captureEnvelope", "true", "--es", "skin", skin]
    if not native: arguments += ["--es", "testReceiver", fixture]
    adb(*arguments)
    assert find("Open file…").get("class") == "android.widget.Button"
    assert find("Play").get("class") == "android.widget.Button"
    assert "files/player/skins/skins" not in adb("exec-out", "run-as", PACKAGE, "find", "files/player/skins", "-type", "d").splitlines()
    adb("shell", "input", "tap", "10", "150")
    if os.environ.get("CAST_TEST_RECEIVER") and not native: find("Receiver: Test receiver" if skin == "sherbet" else "Receiver")


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
    tap("Open file…")
    open_roots()
    select_root("Downloads")
    tap("Casting mobile test.mp3")
    find("Open file…")


def alternate_playback(output):
    tap("Skin"); tap("Midnight Mint")
    import_track()
    tap("Play"); find("Pause")
    touch_regions(["Pause", "Stop", "Speed", "Volume 100%", "Forward 10 seconds", "Back 10 seconds"])
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
    tap("Receiver"); tap("Unavailable receiver")
    tap("Play")
    failure = "127.0.0.1:1: connection refused or timed out"
    find(failure); find("Play")
    tap("Skin"); tap("Midnight Mint"); find(failure)
    (output / "connection-refused.png").write_bytes(adb("exec-out", "screencap", "-p", binary=True))
    tap("Receiver"); tap("Test receiver")
    tap("Play"); find("Pause")
    tap("Stop"); find("Play")
    print("PASS: native connection failure remains visible across skins and a reachable receiver recovers playback", flush=True)


def receiver_selection():
    choices = native_picker(json.loads(adb("exec-out", "run-as", PACKAGE, "cat", "files/test-scene.json")), "Receiver")
    assert len(choices) == 1, choices
    return choices[0]["selected"]


def native_discovery(output):
    # [CAST-MOBILE-DELIVERY] No testReceiver extra: real NSD and service-info callbacks.
    launch("midnight", native=True)
    tap("Receiver"); tap("Android native test receiver")
    find("Receiver: Android native test receiver")
    key = receiver_selection()
    assert key.endswith(":" + os.environ["CAST_TEST_NATIVE_PORT"]), key
    tap("Search again"); tap("Receiver")
    find("Android native test receiver")
    assert receiver_selection() == key, "Native NSD rescan lost the selected endpoint"
    tap("Android native test receiver")
    import_track()
    tap("Play"); find("Pause")
    stale_volume_popup(tap, find, lambda: json.loads(adb("exec-out", "run-as", PACKAGE, "cat", "files/test-scene.json")))
    (output / "native-nsd-playing.png").write_bytes(adb("exec-out", "screencap", "-p", binary=True))
    tap("Stop"); find("Play")
    print("PASS: real Android NSD discovery, service resolution, rescan and selected-receiver playback", flush=True)


def duplicate_receivers():
    # [CAST-MOBILE-RECEIVER-IDENTITY] The first equal-name device is unavailable.
    port = os.environ["CAST_TEST_NATIVE_PORT"]
    launch("midnight", os.environ["CAST_TEST_DUPLICATE_RECEIVER"])
    for attempt in range(2):
        tap("Receiver")
        find("Same name receiver (127.0.0.1:1)")
        if attempt == 1:
            snapshot = adb("exec-out", "run-as", PACKAGE, "cat", "files/test-scene.json")
            assert f'"selected":"127.0.0.1:{port}"' in snapshot, "Rescan lost the selected endpoint"
        tap(f"Same name receiver (127.0.0.1:{port})")
        if attempt == 0: tap("Search again")
    import_track()
    tap("Play"); find("Pause")
    tap("Stop"); find("Play")
    print("PASS: equal receiver names retain distinct endpoints across rescan and play on the selected receiver", flush=True)


def main():
    output = ROOT / "examples/casting/build/android-test"
    output.mkdir(parents=True, exist_ok=True)
    bindings()
    with tempfile.TemporaryDirectory(prefix="casting-mobile-") as directory:
        track = Path(directory) / "Casting mobile test.mp3"
        subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "anullsrc=r=44100:cl=mono", "-t", "1", str(track)], check=True)
        adb("push", str(track), "/sdcard/Download/Casting mobile test.mp3")
        index_audio("/sdcard/Download/Casting mobile test.mp3")
        if os.environ.get("CAST_TEST_RECEIVER"): native_discovery(output)
        if os.environ.get("CAST_TEST_RECEIVER"): duplicate_receivers()
        launch()
        touch_regions(["Play", "Open file…", "Receiver: Test receiver", "Appearance"])
        tap("Appearance"); tap("Midnight Mint")
        find("Skin")
        touch_regions(["Play", "Open file…", "Receiver", "Skin", "Appearance"])
        snapshot = json.loads(adb("exec-out", "run-as", PACKAGE, "cat", "files/test-scene.json"))
        assert snapshot["node"]["kind"] == "themed", snapshot
        tap("Skin")
        tap("Poolside Pop")
        find("Skin")
        touch_regions(["Play", "Open file…", "Receiver", "Skin"])
        snapshot = json.loads(adb("exec-out", "run-as", PACKAGE, "cat", "files/test-scene.json"))
        assert any(token["name"] == "background" and token["light"] == "#D9EEFF" for token in snapshot["node"]["theme"]["tokens"]), snapshot
        (output / "poolside-ready.png").write_bytes(adb("exec-out", "screencap", "-p", binary=True))
        if os.environ.get("CAST_TEST_RECEIVER"): alternate_playback(output)
        launch()
        (output / "ready.png").write_bytes(adb("exec-out", "screencap", "-p", binary=True))
        keyboard_picker()
        for _ in range(2):
            node = find("Open file…")
            x1, y1, x2, y2 = map(int, re.findall(r"\d+", node.attrib["bounds"]))
            adb("shell", "input", "tap", str((x1 + x2) // 2), str(round(y1 + 0.85 * (y2 - y1))))
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
        with mock.patch(__name__ + ".open_roots"), mock.patch(__name__ + ".find"), mock.patch(__name__ + ".tap", side_effect=tap_label), mock.patch(__name__ + ".tap_node", side_effect=select), mock.patch(__name__ + ".window", side_effect=snapshot), mock.patch(__name__ + ".time.sleep"):
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
