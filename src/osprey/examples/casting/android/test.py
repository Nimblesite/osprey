#!/usr/bin/env python3
"""[CAST-MOBILE-DELIVERY] Real Android picker, file import, accessibility and restart."""
import base64
import io
import json
import os
import re
import shlex
import subprocess
import sys
import tempfile
import time
import unittest
import zipfile
from pathlib import Path
from unittest import mock
from xml.etree import ElementTree
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from mobile_test import BINDING_CHECK, CastingArtifactContracts, android_java, invalid_appearance, native_picker, stale_volume_popup, verify_android_package, wait_native_skin
ROOT = Path(__file__).resolve().parents[5]
SDK = Path(os.environ.get("ANDROID_HOME", Path.home() / "Library/Android/sdk"))
SERIAL = os.environ.get("OSPREY_ANDROID_SERIAL", "")
ADB = [str(SDK / "platform-tools/adb"), "-s", SERIAL]
PACKAGE = "org.ospreylang.casting"
WINDOW_PATH = "/sdcard/casting-test-window.xml"
APK_TARGETS = {"arm64-v8a": "android-arm64", "x86_64": "android-x64"}
GPUI_LIBRARY = "libosprey_gpui.so"


def adb(*arguments, binary=False):
    if not SERIAL: raise ValueError("Set OSPREY_ANDROID_SERIAL to the Android device under test")
    if arguments[:1] == ("shell",): arguments = ("shell", shlex.join(arguments[1:]))
    return subprocess.check_output(ADB + list(arguments), text=not binary)


def installed_apk_path():
    # [CAST-MOBILE-PROVENANCE] Only the installed base.apk can prove provenance; splits and retired receipts cannot.
    lines = [line.strip() for line in adb("shell", "pm", "path", PACKAGE).splitlines() if line.strip()]
    assert len(lines) == 1, f"Expected exactly one installed path for {PACKAGE}: {lines}"
    assert re.fullmatch(r"package:/.*/base\.apk", lines[0]), f"Installed package path is not an absolute base.apk: {lines[0]!r}"
    return lines[0][len("package:"):]


def verify_installed_package():
    abi = adb("shell", "getprop", "ro.product.cpu.abi").strip()
    assert abi in APK_TARGETS, f"Unsupported device ABI {abi!r}; expected one of {sorted(APK_TARGETS)}"
    path = installed_apk_path(); installed = adb("exec-out", "cat", path, binary=True)
    expected = APP / "build" / APK_TARGETS[abi] / "Casting.apk"
    assert installed == expected.read_bytes(), f"Installed {PACKAGE} is not the signed APK built at {expected}"
    with zipfile.ZipFile(io.BytesIO(installed)) as archive:
        assert f"lib/{abi}/{GPUI_LIBRARY}" in archive.namelist(), f"Installed APK lacks the shared GPUI renderer lib/{abi}/{GPUI_LIBRARY}"
    print(f"PASS: {PACKAGE} is the signed {APK_TARGETS[abi]} build with lib/{abi}/{GPUI_LIBRARY}", flush=True)


def index_audio(path):
    canonical = adb("shell", "readlink", "-f", path).strip()
    assert canonical.startswith("/"), f"Cannot resolve the audio fixture: {canonical}"
    result = adb("shell", "content", "call", "--uri", "content://media", "--method", "scan_file", "--arg", canonical)
    assert result.startswith("Result: Bundle[") and "android.intent.extra.STREAM=" in result and "android.intent.extra.STREAM=null" not in result, result
    print("PASS: Android MediaStore indexed the audio fixture before document selection", flush=True)


def window():
    # A completed dump can precede UiAutomation's Binder teardown crash; never accept a previous file, an incomplete dump or another failure.
    adb("shell", "rm", "-f", WINDOW_PATH); completed = "dumped to: " + WINDOW_PATH
    try: response = adb("shell", "uiautomator", "dump", WINDOW_PATH)
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
        if matches: return matches[-1]  # The drawer is the last layer; its Downloads row can share the toolbar's label.
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
        controls = [] if requested or tree is None else [node for node in tree.iter("node") if node.get("content-desc") == "Show roots"]
        if controls: print("TAP: Show roots", flush=True); tap_node(controls[-1]); requested = True
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


APP = ROOT / "src/osprey/examples/casting"
OUTPUT = APP / "build/android-test"
BOUNDS = re.compile(r"(?:^|\s)bounds=([+-]?\d+)\s+([+-]?\d+)\s+([+-]?\d+)\s+([+-]?\d+)(?=\s|$)")


def parse_bounds(answer, label="control"):
    """The live screen rectangle from the bounded instrumentation result."""
    assert "pressed=true" in answer, f"Accessibility bounds unavailable for {label}: {answer}"
    match = BOUNDS.search(answer)
    assert match, f"Accessibility bounds missing or malformed for {label}: {answer}"
    left, top, right, bottom = map(int, match.groups())
    assert right > left and bottom > top, f"Accessibility bounds are not a positive area for {label}: {match.group(0)}"
    return left, top, right, bottom


def scene_bounds(label, prefix=False):
    print("BOUNDS:", label, flush=True)
    answer = adb("shell", "am", "instrument", "-w", "-e", "label", label, "-e", "bounds", "1", "-e", "match", "prefix" if prefix else "exact", "org.ospreylang.casting.press/.Press")
    return parse_bounds(answer, label)


def scene_tap(label, prefix=False):
    left, top, right, bottom = scene_bounds(label, prefix=prefix)
    print("TAP:", label, "bounds", (left, top, right, bottom), flush=True)
    adb("shell", "input", "tap", str((left + right) // 2), str((top + bottom) // 2))


def scene_slide(label, fraction):
    left, top, right, bottom = scene_bounds(label)
    x = round(left + (right - left) * fraction); y = (top + bottom) // 2
    print("SLIDE:", label, fraction, "bounds", (left, top, right, bottom), flush=True)
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
    adb("shell", "am", "force-stop", PACKAGE); adb("logcat", "-c")
    adb("shell", "am", "start", "-W", "-n", PACKAGE + "/.CastingActivity", "--es", "testReceiver", os.environ["CAST_TEST_RECEIVER"])
    wait_focus(); wait_accessible("Open file", "Button")
    scene_bounds("Open file…")


def install_press():
    """The helper that presses through the accessibility action a screen reader sends."""
    source = APP / "android/tests/press"
    tools = sorted((SDK / "build-tools").iterdir(), key=lambda path: [int(part) for part in path.name.split(".") if part.isdigit()])[-1]
    platform = sorted((SDK / "platforms").glob("android-*"), key=lambda path: float(path.name.split("-")[1]))[-1] / "android.jar"
    java_home = os.environ.get("JAVA_HOME", "/Applications/Android Studio.app/Contents/jbr/Contents/Home")
    environment = dict(os.environ, JAVA_HOME=java_home, PATH=f"{java_home}/bin:{os.environ['PATH']}")
    with tempfile.TemporaryDirectory(prefix="casting-press-") as folder:
        work = Path(folder)
        run = lambda *arguments: subprocess.run(arguments, check=True, env=environment, capture_output=True)
        run(str(tools / "aapt2"), "link", "-o", str(work / "base.apk"), "--manifest", str(source / "AndroidManifest.xml"), "-I", str(platform), "--min-sdk-version", "34", "--target-sdk-version", "37")
        run("javac", "-nowarn", "--release", "11", "-classpath", str(platform), "-d", str(work), str(source / "java/org/ospreylang/casting/press/Press.java"))
        run(str(tools / "d8"), "--lib", str(platform), "--min-api", "34", "--output", str(work), str(work / "org/ospreylang/casting/press/Press.class"))
        run("zip", "-j", "-q", str(work / "base.apk"), str(work / "classes.dex"))
        run(str(tools / "apksigner"), "sign", "--ks", str(Path.home() / ".android/debug.keystore"), "--ks-pass", "pass:android", "--key-pass", "pass:android", "--min-sdk-version", "34", "--out", str(work / "press.apk"), str(work / "base.apk"))
        adb("install", "-r", str(work / "press.apk"))


def press(label):
    print("PRESS:", label, flush=True)
    answer = adb("shell", "am", "instrument", "-w", "-e", "label", label, "org.ospreylang.casting.press/.Press")
    time.sleep(0.5)
    assert "pressed=true" in answer, (label, answer)


def wait_accessible(label, role):
    """A screen reader's view of the live window; uiautomator cannot snapshot a spinning disc."""
    print("LOOK:", label, role, flush=True)
    answer = adb("shell", "am", "instrument", "-w", "-e", "label", label, "-e", "role", role, "-e", "look", "1", "org.ospreylang.casting.press/.Press")
    assert "pressed=true" in answer, (label, role, answer)


def main():
    verify_installed_package()
    OUTPUT.mkdir(parents=True, exist_ok=True); install_press()
    with tempfile.TemporaryDirectory(prefix="casting-mobile-") as directory:
        track = Path(directory) / "Casting mobile test.mp3"
        subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "anullsrc=r=44100:cl=mono", "-t", "1", str(track)], check=True)
        adb("push", str(track), "/sdcard/Download/Casting mobile test.mp3"); index_audio("/sdcard/Download/Casting mobile test.mp3")
    launch(); shot("ready.png")
    for _ in range(2):
        scene_tap("Open file\u2026"); find("Show roots"); adb("shell", "input", "keyevent", "KEYCODE_BACK"); wait_focus()
        # The host answers the cancelled pick on its background thread before a press means anything.
        time.sleep(2)
    import_track(); time.sleep(1); shot("imported.png")
    # The same controls a person touches, as a screen reader reads and presses them.
    wait_accessible("Open file", "Button"); wait_accessible("Casting mobile test", "TextView")
    press("Open file"); find("Show roots"); adb("shell", "input", "keyevent", "KEYCODE_BACK"); wait_focus(); time.sleep(2)
    scene_tap("Play"); wait_sent("LOAD", 1)
    wait_accessible("Pause", "Button"); wait_accessible("Volume", "SeekBar")
    press("Pause"); wait_sent("PAUSE", 1)
    wait_accessible("Resume", "Button"); press("Resume"); wait_sent("PLAY", 1)
    time.sleep(1); volumes = len(sent("SET_VOLUME")); scene_slide("Volume", 0.5)
    level = wait_sent("SET_VOLUME", volumes + 1)["volume"]["level"]
    # The listener is wider than the track its thumb travels, so its middle is close to, not at, half.
    assert abs(level - 0.5) < 0.05, level
    seeks = len(sent("SEEK")); scene_slide("Position", 0.25)
    position = wait_sent("SEEK", seeks + 1)["currentTime"]; assert abs(position - 50) < 10, position
    scene_tap("Speed", prefix=True); time.sleep(1)
    scene_tap("1.5\u00d7"); assert wait_sent("SET_PLAYBACK_RATE", 1)["playbackRate"] == 1.5
    shot("playing.png"); previous = adb("shell", "wm", "user-rotation").strip().split()
    try:
        adb("shell", "wm", "user-rotation", "lock", "1"); time.sleep(4); shot("landscape-playing.png")
        scene_tap("Pause"); wait_sent("PAUSE", 2)
    finally:
        adb("shell", "wm", "user-rotation", *previous)
    time.sleep(3); scene_tap("Resume"); wait_sent("PLAY", 2)
    time.sleep(1); scene_tap("Stop"); wait_sent("STOP", 1)
    time.sleep(1); scene_tap("Play"); wait_sent("LOAD", 2)
    time.sleep(1); scene_tap("Stop"); wait_sent("STOP", 2)
    adb("shell", "input", "keyevent", "KEYCODE_BACK"); time.sleep(2)
    launch_after_back = adb("shell", "am", "start", "-W", "-n", PACKAGE + "/.CastingActivity")
    assert "Status: ok" in launch_after_back, launch_after_back
    wait_focus(); wait_accessible("Play", "Button")
    launch(); shot("restarted.png")
    print("PASS: Android GPUI player: two picker cancellations, MP3 import, Cast playback, rotation, restart")


class NativeReadContracts(unittest.TestCase):
    def test_launch_waits_for_control_bounds_inside_the_real_window(self):
        with mock.patch.dict(os.environ, {"CAST_TEST_RECEIVER": "fixture"}), mock.patch(__name__ + ".adb", return_value="Status: ok"), mock.patch(__name__ + ".wait_focus"), mock.patch(__name__ + ".wait_accessible") as visible, mock.patch(__name__ + ".scene_bounds", side_effect=AssertionError("control outside active window")) as bounds:
            with self.assertRaisesRegex(AssertionError, "outside active window"):
                launch()
            visible.assert_called_once_with("Open file", "Button")
            bounds.assert_called_once_with("Open file…")

    def test_native_http_preserves_binary_response_and_quotes_shell_arguments(self):
        import mobile_receiver as receiver
        data = b"HTTP/1.1 206 Partial Content\r\nContent-Length: 4\r\nContent-Type: audio/mpeg\r\n\r\n\x00\xff\r\n"
        request = receiver.urllib.request.Request("http://10.0.2.16:8000/media?x=$(private)", headers={"Range": "bytes=0-3"}); result = mock.Mock(returncode=0, stdout=data, stderr=b"")
        with mock.patch.dict(os.environ, {"CAST_TEST_NATIVE_JAR": "/tmp/fixture's path.jar"}), mock.patch.object(receiver.subprocess, "run", return_value=result) as run:
            with receiver.native_http("stub", request, 3) as response:
                self.assertEqual((response.status, response.headers.get_content_type(), response.read()), (206, "audio/mpeg", b"\x00\xff\r\n"))
            expected = ["env", "CLASSPATH=/tmp/fixture's path.jar", "app_process", "/", "CastingNsdPublisher$MediaCheck", "GET", request.full_url, "bytes=0-3"]
            self.assertEqual(shlex.split(run.call_args.args[0][-1]), expected)
            result.stdout = b"invalid HTTP"
            with self.assertRaises(receiver.http.client.HTTPException): receiver.native_http("stub", request, 3)
    def test_accessibility_bounds_require_a_pressed_positive_live_rectangle(self):
        answer = "INSTRUMENTATION_RESULT: pressed=true\nINSTRUMENTATION_RESULT: bounds=136 -20 240 200\n"
        self.assertEqual(parse_bounds(answer, "Pause"), (136, -20, 240, 200))
        malformed = {
            "missing": "INSTRUMENTATION_RESULT: pressed=true\n",
            "not pressed": "INSTRUMENTATION_RESULT: pressed=not found in\nINSTRUMENTATION_RESULT: bounds=10 20 30 40\n",
            "short": "INSTRUMENTATION_RESULT: pressed=true\nINSTRUMENTATION_RESULT: bounds=10 20 30\n",
            "zero width": "INSTRUMENTATION_RESULT: pressed=true\nINSTRUMENTATION_RESULT: bounds=10 20 10 40\n",
            "zero height": "INSTRUMENTATION_RESULT: pressed=true\nINSTRUMENTATION_RESULT: bounds=10 20 30 20\n",
            "text": "INSTRUMENTATION_RESULT: pressed=true\nINSTRUMENTATION_RESULT: bounds=left top right bottom\n",
        }
        for name, text in malformed.items():
            with self.subTest(name), self.assertRaisesRegex(AssertionError, "bounds|positive area"):
                parse_bounds(text, "Pause")
    def test_scene_bounds_queries_the_bounded_instrumentation(self):
        answer = "INSTRUMENTATION_RESULT: pressed=true\nINSTRUMENTATION_RESULT: bounds=-5 20 105 60\n"
        with mock.patch(__name__ + ".adb", return_value=answer) as query, mock.patch("sys.stdout", new_callable=io.StringIO):
            self.assertEqual(scene_bounds("Pause"), (-5, 20, 105, 60))
        query.assert_called_once_with("shell", "am", "instrument", "-w", "-e", "label", "Pause", "-e", "bounds", "1", "-e", "match", "exact", "org.ospreylang.casting.press/.Press")
    def test_scene_pointer_uses_live_bounds_not_duplicated_geometry(self):
        with mock.patch(__name__ + ".scene_bounds", side_effect=[(100, 200, 300, 400), (0, 10, 100, 30)]), mock.patch(__name__ + ".adb") as commands, mock.patch("sys.stdout", new_callable=io.StringIO):
            scene_tap("Pause"); scene_slide("Volume", 0.25)
        self.assertEqual(commands.call_args_list, [mock.call("shell", "input", "tap", "200", "300"), mock.call("shell", "input", "swipe", "25", "20", "26", "20", "150")])
    def test_transient_automation_failure_requests_a_new_snapshot(self):
        failure = subprocess.CalledProcessError(137, "uiautomator", output="interrupted before dump"); good = ElementTree.fromstring('<hierarchy><node text="Play"/></hierarchy>')
        with mock.patch(__name__ + ".window", side_effect=[failure, good]) as query:
            self.assertEqual([node.get("text") for node in nodes()], ["Play"]); self.assertEqual(query.call_count, 2)
        for code, count in [(1, 1), (137, 3)]:
            with self.subTest(code=code), mock.patch(__name__ + ".window", side_effect=subprocess.CalledProcessError(code, "uiautomator")) as query:
                with self.assertRaises(subprocess.CalledProcessError): nodes()
                self.assertEqual(query.call_count, count)
    def test_import_waits_until_the_root_drawer_really_closes(self):
        drawer = ElementTree.fromstring('<hierarchy><node resource-id="picker:id/drawer_roots"><node text="Downloads" bounds="[0,100][200,200]"/></node></hierarchy>')
        directory = ElementTree.fromstring('<hierarchy><node text="Downloads"/></hierarchy>'); selections = []
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
                adb("shell", "content", "call", "--arg", path); arguments = command.call_args.args[0]
                self.assertEqual(arguments[:4], ["adb", "-s", "device", "shell"])
                self.assertEqual(shlex.split(" ".join(arguments[4:])), ["content", "call", "--arg", path])
    def test_failure_diagnostics_keep_the_original_failure_and_survive_dead_adb(self):
        sentinel = RuntimeError("the journey failed at Pause"); lines = "".join(f'{{"requestId": {n}, "event": "recv"}}\n' for n in range(20))
        closed = subprocess.CalledProcessError(1, "adb", output="error: closed")
        for name, answers, saved in [("every adb query dead", [closed, closed], None), ("screenshot saved first", [b"\x89PNG", closed], b"\x89PNG")]:
            with self.subTest(name), tempfile.TemporaryDirectory() as folder, mock.patch(__name__ + ".OUTPUT", Path(folder)), mock.patch(__name__ + ".main", side_effect=sentinel), mock.patch(__name__ + ".adb", side_effect=answers) as queries, mock.patch("sys.stderr", new_callable=io.StringIO) as reported:
                (Path(folder) / "receiver.jsonl").write_text(lines)
                with self.assertRaises(RuntimeError) as caught: run_journey()
                self.assertIs(caught.exception, sentinel); self.assertEqual(str(caught.exception), "the journey failed at Pause")
                self.assertEqual([(call.args, call.kwargs) for call in queries.call_args_list], [(('exec-out', 'screencap', '-p'), {'binary': True}), (('logcat', '-d'), {})])
                self.assertNotIn("test-scene.json", repr(queries.call_args_list))
                self.assertEqual((Path(folder) / "failed-receiver.jsonl").read_text(), "".join(lines.splitlines(keepends=True)[-15:]))
                self.assertEqual((Path(folder) / "failed.png").read_bytes() if (Path(folder) / "failed.png").exists() else None, saved)
                self.assertIn("failed-logcat.txt unavailable", reported.getvalue())
    def test_picker_waits_for_the_drawer_not_the_directory_toolbar(self):
        toolbar = ElementTree.fromstring('<hierarchy><node text="Downloads"/><node content-desc="Show roots" bounds="[0,0][100,100]"/></hierarchy>')
        drawer = ElementTree.fromstring('<hierarchy><node resource-id="com.google.android.documentsui:id/drawer_roots"><node text="Downloads"/></node></hierarchy>')
        with mock.patch(__name__ + ".window", side_effect=[None, toolbar, drawer]) as snapshots, mock.patch(__name__ + ".adb") as commands, mock.patch(__name__ + ".time.sleep"):
            open_roots(); self.assertEqual(snapshots.call_count, 3); commands.assert_called_once_with("shell", "input", "tap", "50", "50")
        with mock.patch(__name__ + ".window", return_value=drawer), mock.patch(__name__ + ".adb") as commands:
            open_roots(); commands.assert_not_called()
    def test_pointer_never_selects_a_file_behind_the_picker_drawer(self):
        covered = ElementTree.fromstring('<hierarchy><node text="track.mp3" bounds="[10,10][60,60]"/><node resource-id="com.google.android.documentsui:id/drawer_roots"><node text="Downloads" bounds="[0,0][100,100]"/></node></hierarchy>')
        visible = ElementTree.fromstring('<hierarchy><node text="track.mp3" bounds="[100,100][200,200]"/></hierarchy>')
        with mock.patch(__name__ + ".window", side_effect=[covered, visible]) as snapshots, mock.patch(__name__ + ".adb") as commands, mock.patch(__name__ + ".time.sleep"):
            tap("track.mp3"); self.assertEqual(snapshots.call_count, 2); commands.assert_called_once_with("shell", "input", "tap", "150", "150")
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
        message = "UI hierchary dumped to: /sdcard/casting-test-window.xml"; xml = '<hierarchy rotation="0"><node text="Pause" /></hierarchy>'
        for completed in [message, subprocess.CalledProcessError(137, "uiautomator", output=message)]:
            with self.subTest(completed=completed), self.queries(completed, xml) as read:
                result = window(); self.assertEqual(result.find("node").get("text"), "Pause")
                self.assertEqual(read.call_args_list, [mock.call("shell", "rm", "-f", "/sdcard/casting-test-window.xml"), mock.call("shell", "uiautomator", "dump", "/sdcard/casting-test-window.xml"), mock.call("exec-out", "cat", "/sdcard/casting-test-window.xml")])
    def test_incomplete_or_failed_queries_never_use_a_previous_snapshot(self):
        for code, output in [(137, "null root"), (137, "dumped to: /sdcard/other.xml"), (1, "dumped to:"), (139, "dumped to:")]:
            failure = subprocess.CalledProcessError(code, "uiautomator", output=output)
            with self.subTest(code=code), self.queries(failure) as read:
                with self.assertRaises(subprocess.CalledProcessError): window()
                self.assertEqual(read.call_count, 2)
        with self.queries("null root") as read: self.assertIsNone(window()); self.assertEqual(read.call_count, 2)
        with self.queries("dumped to: " + WINDOW_PATH):
            with self.assertRaises(ElementTree.ParseError): window()
        failure = subprocess.CalledProcessError(1, "cat", output="missing snapshot")
        with self.queries("dumped to: " + WINDOW_PATH, failure):
            with self.assertRaises(subprocess.CalledProcessError): window()
    def apk(self, abi="arm64-v8a", gpui=True):
        stream = io.BytesIO()
        with zipfile.ZipFile(stream, "w") as archive:
            archive.writestr("assets/player.machine.json", "{}")
            if gpui: archive.writestr(f"lib/{abi}/{GPUI_LIBRARY}", b"\x7fELF\x02\x01")
        return stream.getvalue()
    def device(self, data, abi="arm64-v8a", paths="package:/data/app/~~x/base.apk\n", path="/data/app/~~x/base.apk"):
        def answer(*arguments, binary=False):
            if arguments[:2] == ("shell", "getprop"): return abi
            if arguments[:2] == ("shell", "pm"): return paths
            self.assertEqual((arguments, binary), (("exec-out", "cat", path), True)); return data
        return mock.patch(__name__ + ".adb", side_effect=answer)
    def test_installed_package_is_the_signed_build_for_the_device_abi(self):
        with tempfile.TemporaryDirectory() as directory, mock.patch(__name__ + ".APP", Path(directory)), mock.patch("sys.stdout", new_callable=io.StringIO) as printed:
            for abi, target in APK_TARGETS.items():
                signed = self.apk(abi); expected = Path(directory) / "build" / target / "Casting.apk"
                expected.parent.mkdir(parents=True, exist_ok=True); expected.write_bytes(signed)
                with self.subTest(abi=abi), self.device(signed, abi): verify_installed_package()
                self.assertIn(f"PASS: {PACKAGE} is the signed {target} build with lib/{abi}/{GPUI_LIBRARY}", printed.getvalue())
                with self.subTest(abi=abi, stale=True), self.device(signed + b"stale", abi):
                    with self.assertRaisesRegex(AssertionError, "is not the signed APK built at"): verify_installed_package()
    def test_installed_package_must_contain_the_shared_gpui_renderer(self):
        with tempfile.TemporaryDirectory() as directory, mock.patch(__name__ + ".APP", Path(directory)):
            unreceipted = self.apk(gpui=False); expected = Path(directory) / "build/android-arm64/Casting.apk"
            expected.parent.mkdir(parents=True); expected.write_bytes(unreceipted)
            with self.device(unreceipted):
                with self.assertRaisesRegex(AssertionError, f"lacks the shared GPUI renderer lib/arm64-v8a/{GPUI_LIBRARY}"): verify_installed_package()
    def test_installed_package_rejects_unsupported_device_abis(self):
        for abi in ("armeabi-v7a", "x86", "", "arm64-v8a\nx86_64"):
            with self.subTest(abi=abi), mock.patch(__name__ + ".adb", return_value=abi) as device:
                with self.assertRaisesRegex(AssertionError, "Unsupported device ABI"): verify_installed_package()
                device.assert_called_once_with("shell", "getprop", "ro.product.cpu.abi")
    def test_installed_package_rejects_missing_multiple_or_malformed_paths(self):
        cases = [("", "Expected exactly one installed path"), ("package:/data/app/~~1/base.apk\npackage:/data/app/~~2/base.apk\n", "Expected exactly one installed path"),
                 ("package:/data/app/~~1/base.apk\npackage:/data/app/~~1/base.apk\n", "Expected exactly one installed path"), ("package:/base.apk\n", "not an absolute base.apk"),
                 ("package:/data/app/base.apk.bak\n", "not an absolute base.apk"), ("package: /data/app/base.apk\n", "not an absolute base.apk")]
        for paths, message in cases:
            with self.subTest(paths=paths), self.device(b"", paths=paths):
                with self.assertRaisesRegex(AssertionError, message): verify_installed_package()
    def test_provenance_runs_before_any_device_ui_step(self):
        failure, order = AssertionError("stale installed package"), []
        with tempfile.TemporaryDirectory() as folder, mock.patch(__name__ + ".OUTPUT", Path(folder)), mock.patch(__name__ + ".verify_installed_package", side_effect=failure) as verify, mock.patch(__name__ + ".install_press") as install, mock.patch(__name__ + ".launch") as launch:
            with self.assertRaises(AssertionError) as caught: main()
            self.assertIs(caught.exception, failure); verify.assert_called_once_with(); install.assert_not_called(); launch.assert_not_called()
        with tempfile.TemporaryDirectory() as folder, mock.patch(__name__ + ".OUTPUT", Path(folder)), mock.patch(__name__ + ".verify_installed_package", side_effect=lambda: order.append("verify")), mock.patch(__name__ + ".install_press", side_effect=RuntimeError("stop")) as install:
            with self.assertRaises(RuntimeError): main()
            self.assertEqual(order, ["verify"]); install.assert_called_once_with()


def failure_diagnostics():
    """Live GPUI evidence; one dead artifact never replaces the failure it explains."""
    def tail():
        return "\n".join((OUTPUT / "receiver.jsonl").read_text().splitlines()[-15:]) + "\n"
    steps = [("failed.png", lambda: adb("exec-out", "screencap", "-p", binary=True)), ("failed-logcat.txt", lambda: adb("logcat", "-d")), ("failed-receiver.jsonl", tail)]
    for name, produce in steps:
        try:
            data = produce()
            (OUTPUT / name).write_bytes(data.encode() if isinstance(data, str) else data)
        except Exception as error:
            print(f"DIAGNOSTIC: {name} unavailable: {error}", file=sys.stderr, flush=True)


def run_journey():
    try:
        main()
    except Exception:
        failure_diagnostics()
        raise


if __name__ == "__main__":
    if sys.argv[1:] == ["--reader-contracts"]:
        unittest.main(argv=[sys.argv[0]])
    run_journey()
