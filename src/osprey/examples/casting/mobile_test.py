#!/usr/bin/env python3
"""[CAST-MOBILE-DELIVERY] Native app journeys against a real TLS Cast receiver."""
import base64
from contextlib import contextmanager, redirect_stdout
import hashlib
import io
import json
import os
import re
import shlex
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[4]
APP = ROOT / "src/osprey/examples/casting"
sys.path.insert(0, str(ROOT / "scripts/mobile"))
from goldens import png_pixels, png_write, picture_delta


def verify_native_inputs(folder, library):
    records = (folder / "native-inputs.sha256").read_text().splitlines()
    assert records, "Missing native input checksums"
    for line in records:
        expected, name = line.split("  ", 1)
        assert hashlib.sha256(Path(name).read_bytes()).hexdigest() == expected, f"Stale native input: {name}"
    assert library == (folder / "libosprey_casting.stripped.so").read_bytes(), "APK contains a stale native library"


def wait_native_snapshot(snapshot, matches, description):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        try:
            if matches(snapshot()): return
        except json.JSONDecodeError: pass  # A frame can replace the capture while it is read.
        time.sleep(0.1)
    raise AssertionError("Native binding never updated: " + description)


APPEARANCE_PATH = "files/.local/share/osprey/osprey/casting-appearance.txt"


def saved_appearance(adb, package):
    return adb("exec-out", "run-as", package, "cat", APPEARANCE_PATH, binary=True)


def seed_appearance(adb, package, skin):
    # [CAST-MOBILE-APPEARANCE-RECOVERY] Write the shared host's real store, preserving UTF-8.
    adb("shell", "am", "force-stop", package)
    encoded = base64.b64encode(skin.encode()).decode()
    command = f"mkdir -p {shlex.quote(str(Path(APPEARANCE_PATH).parent))} && printf %s {shlex.quote(encoded)} | base64 -d > {APPEARANCE_PATH}"
    adb("shell", "run-as", package, "sh", "-c", command)
    assert saved_appearance(adb, package) == skin.encode(), "The recovery fixture must preserve exact saved bytes"


def invalid_appearance(adb, launch, press, look, bounds, package):
    for skin in ("removed-skin", "", "poolside & <v2>' ☄"):
        seed_appearance(adb, package, skin)
        launch(notice=True)
        look("Appearance restored", "TextView")
        look("The selected appearance is unavailable. The default appearance has been restored.", "TextView")
        press("OK")
        bounds("Open file…"); bounds("Appearance: Sherbet Signal")
        wait_native_snapshot(lambda: saved_appearance(adb, package), lambda saved: saved == b"sherbet", "repaired appearance")
        launch(); bounds("Appearance: Sherbet Signal")
    print("PASS: invalid saved appearances show a native notice, repair the shared store and restart normally", flush=True)


def selected_appearance(adb, label):
    answer = adb("shell", "am", "instrument", "-w", "-e", "label", label, "-e", "bounds", "1", "-e", "match", "exact", "org.ospreylang.casting.press/.Press")
    assert "pressed=true" in answer and "selected=true" in answer, f"The shared appearance must announce its selected state: {label}: {answer}"


def android_appearance(adb, launch, tap, bounds, press, look, package):
    invalid_appearance(adb, launch, press, look, bounds, package)
    tap("Appearance: Sherbet Signal")
    for key, label in (("midnight", "Midnight Mint"), ("poolside", "Poolside Pop"), ("sherbet", "Sherbet Signal")):
        tap(label)
        wait_native_snapshot(lambda: saved_appearance(adb, package), lambda saved: saved == key.encode(), label)
        launch()
        if key == "sherbet":
            tap("Appearance: Sherbet Signal")
        bounds(label); selected_appearance(adb, label)
    tap("Sherbet Signal")
    print("PASS: all three shared appearances persist across Android process restart", flush=True)


def verify_android_package(archive):
    paths = archive.namelist()
    assert len(paths) == len(set(paths)), "Duplicate APK archive entries"
    assert "assets/player.machine.json" in paths
    assert archive.read("assets/player.machine.json") == (APP / "player.machine.json").read_bytes(), "APK contains a stale machine document"
    for name in ("backdrop", "disc", "transport"):
        assert f"assets/skins/sherbet/{name}.png" in paths, name
    assert not any(path.startswith("assets/skins/skins/") for path in paths), "Nested duplicate skin assets in the APK"
    targets = {"arm64-v8a": "android-arm64", "x86_64": "android-x64"}
    abis = {path.split("/")[1] for path in paths if path.startswith("lib/") and path.endswith(".so")}
    assert abis == set(targets), ("APK must support ARM64 and x64", abis)
    for abi, target in targets.items():
        library = archive.read(f"lib/{abi}/libosprey_casting.so")
        assert library[:6] == b"\x7fELF\x02\x01", f"Expected little-endian ELF64 for {abi}"
        assert int.from_bytes(library[18:20], "little") == (183 if abi == "arm64-v8a" else 62), f"Wrong ELF architecture for {abi}"
        verify_native_inputs(APP / "build" / target, library)


class CastingArtifactContracts(unittest.TestCase):
    def test_native_receipt_rejects_changed_sources_binaries_and_missing_provenance(self):
        # [CAST-MOBILE-ABI] Isolate stale-source mutations from real compiler archives.
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            source = folder / "input.c"; source.write_bytes(b"abc")
            binary = folder / "libosprey_casting.stripped.so"; binary.write_bytes(b"native")
            receipt = folder / "native-inputs.sha256"
            receipt.write_text(f"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  {source}\n")
            verify_native_inputs(folder, b"native")
            source.write_bytes(b"changed")
            with self.assertRaisesRegex(AssertionError, "Stale native input"): verify_native_inputs(folder, b"native")
            source.write_bytes(b"abc")
            with self.assertRaisesRegex(AssertionError, "stale native library"): verify_native_inputs(folder, b"old")
            binary.write_bytes(b"wrong")
            with self.assertRaisesRegex(AssertionError, "stale native library"): verify_native_inputs(folder, b"native")
            receipt.write_text("")
            with self.assertRaisesRegex(AssertionError, "Missing native input"): verify_native_inputs(folder, b"native")
            receipt.unlink()
            with self.assertRaises(FileNotFoundError): verify_native_inputs(folder, b"native")

    def test_android_receipt_rejects_changed_and_missing_load_metadata(self):
        media = {"contentId": "receipt", "contentType": "audio/mpeg", "metadata": {"title": "Casting mobile test"}}
        script = [(kind, {}) for kind in ("LAUNCH", "LOAD", "PAUSE", "PLAY", "SET_VOLUME", "SEEK",
                                         "SET_PLAYBACK_RATE", "PAUSE", "PLAY", "STOP", "LOAD", "STOP")]
        given = {"event": "load", "title": media["metadata"]["title"], "contentType": media["contentType"]}
        records = [{"event": "recv", "payload": dict(fields, type=kind)} for kind, fields in script] + [dict(given), dict(given)]

        def verdict(items):
            with tempfile.TemporaryDirectory() as directory, redirect_stdout(io.StringIO()):
                path = Path(directory) / "receiver.jsonl"
                path.write_text("".join(json.dumps(record) + "\n" for record in items))
                try: verify_log(path, "android")
                except AssertionError: return "rejected"
                return "accepted"

        def replaced(load, edit):
            return verdict([edit(record) if index == load else record for index, record in enumerate(records)])

        self.assertEqual(verdict(records), "accepted")
        loads_at = [index for index, record in enumerate(records) if record.get("event") == "load"]
        self.assertEqual(len(loads_at), 2)
        edits = [("title", "Casting mobile tests"), ("title", ""), ("title", None),
                 ("contentType", "audio/mp3"), ("contentType", "video/mp4"), ("title",), ("contentType",)]
        for load in loads_at:
            for field, *value in edits:
                edit = (lambda record, f=field, v=value[0]: dict(record, **{f: v})) if value else (lambda record, f=field: {k: v for k, v in record.items() if k != f})
                self.assertEqual(replaced(load, edit), "rejected", (load, field, value))
        for dropped in (loads_at[:1], loads_at, range(len(records))):
            self.assertEqual(verdict([r for i, r in enumerate(records) if i not in dropped]), "rejected", dropped)


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


def sdk_tool(sdk, pattern):
    paths = list(sdk.glob(pattern))
    assert paths, f"Missing Android SDK tool: {pattern}"
    return max(paths, key=lambda path: tuple(map(int, re.findall(r"\d+", path.parent.name))))


def command(arguments, **options):
    return subprocess.run(arguments, check=True, **options)


def android_java_path(name):
    return f"/data/local/tmp/osprey-casting-{name}-{os.getpid()}.jar"


@contextmanager
def android_java(sdk, adb, text, name, work):
    platform = sdk_tool(sdk, "platforms/*/android.jar")
    compiler = sdk_tool(sdk, "build-tools/*/d8")
    folder = work / name
    folder.mkdir()
    source = folder / (name + ".java"); source.write_text(text)
    command(["javac", "-Xlint:all", "-Werror", "-classpath", str(platform), "-d", str(folder), str(source)])
    archive = folder / "check.jar"
    command([str(compiler), "--lib", str(platform), "--output", str(archive), *map(str, sorted(folder.glob(name + "*.class")))])
    remote = android_java_path(name)
    try:
        command(adb + ["push", str(archive), remote])
        command(adb + ["shell", "chmod", "444", remote])
        yield remote
    finally: command(adb + ["shell", "rm", "-f", remote])


def receiver_fixture(port, duplicate=False):
    devices = [("Same name receiver", 1), ("Same name receiver", port)] if duplicate else [("Test receiver", port), ("Unavailable receiver", 1)]
    text = "".join(f'cast-{i} SRV 0 0 {at} 127.0.0.1\ncast-{i} TXT "fn={name}"\n' for i, (name, at) in enumerate(devices))
    return base64.b64encode(text.encode()).decode()


def stop_process(process):
    process.terminate()
    try: process.wait(timeout=5)
    except subprocess.TimeoutExpired: process.kill(); process.wait(timeout=5)


def wait_publisher(process, path, marker, alive=True):
    for _ in range(100):
        if marker in path.read_text() and (not alive or process.poll() is None): return
        if process.poll() is not None: raise RuntimeError("Publisher exited: " + path.read_text())
        time.sleep(0.05)
    raise RuntimeError("Publisher registration timed out: " + path.read_text())


def finish_android_publisher(process, adb, path):
    try:
        process.stdin.write(b"stop\n"); process.stdin.flush()
        wait_publisher(process, path, "UNREGISTERED", alive=False)
    finally:
        if process.poll() is None:
            pid = re.search(r"^PID: (\d+)$", path.read_text(), re.M)
            if not pid: raise RuntimeError("Android publisher did not report its process ID")
            # ActivityThread leaves Binder threads alive after its looper returns.
            command(adb + ["shell", "kill", pid[1]])
            process.communicate(timeout=5)
    if process.returncode not in (0, 143) or "UNREGISTERED" not in path.read_text() or "FAILED" in path.read_text():
        raise RuntimeError("Android publisher cleanup failed: " + path.read_text())


@contextmanager
def android_publisher(port, work, adb, sdk):
    from mobile_receiver import ANDROID_PUBLISHER
    with android_java(sdk, adb, ANDROID_PUBLISHER, "CastingNsdPublisher", work) as remote:
        path = work / "android-publisher.log"
        with path.open("w") as log:
            process = subprocess.Popen(adb + ["shell", "env", "CLASSPATH=" + remote, "app_process", "/", "CastingNsdPublisher", str(port)],
                stdin=subprocess.PIPE, stdout=log, stderr=subprocess.STDOUT)
            try:
                wait_publisher(process, path, "REGISTERED")
                yield
            finally: finish_android_publisher(process, adb, path)


@contextmanager
def bonjour_publisher(port, name, work):
    path = work / (name + ".log")
    with path.open("w") as log:
        process = subprocess.Popen(["/usr/bin/dns-sd", "-P", name + "-" + str(os.getpid()),
            "_googlecast._tcp", "local.", str(port), f"osprey-casting-{os.getpid()}.local.",
            "127.0.0.1", "fn=Osprey native test receiver"], stdout=log, stderr=subprocess.STDOUT)
        try:
            wait_publisher(process, path, f"reply for service {name}-{os.getpid()}._googlecast._tcp.local.: Name now registered and active")
            yield
        finally: stop_process(process)


@contextmanager
def bonjour_fixture(port, work):
    # [CAST-MOBILE-RECEIVER-IDENTITY] Equal display names, different endpoints.
    with bonjour_publisher(1, "AUnavailable", work), bonjour_publisher(port, "BAvailable", work):
        yield


def run(platform):
    output = APP / "build" / (platform + "-test")
    output.mkdir(parents=True, exist_ok=True)
    log = output / "receiver.jsonl"
    log.write_text("")
    env = dict(os.environ)
    if platform == "ios": env.pop("OSPREY_ANDROID_SERIAL", None)
    sdk = Path(env.get("ANDROID_HOME", Path.home() / "Library/Android/sdk"))
    env["PATH"] = str(sdk / "platform-tools") + os.pathsep + env["PATH"]
    with tempfile.TemporaryDirectory(prefix="casting-receiver-") as folder:
        work = Path(folder)
        command(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", str(work / "key.pem"),
                 "-out", str(work / "cert.pem"), "-subj", "/CN=localhost", "-days", "1"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        command(["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "anullsrc=r=44100:cl=mono", "-t", "1", str(work / "Casting mobile test.mp3")])
        env["CAST_TEST_AUDIO_SOURCE"] = str(work / "Casting mobile test.mp3")
        env["CAST_TEST_VOLUME_FILE"] = str(work / "remote-volume")
        if platform == "android": env["CAST_TEST_NATIVE_JAR"] = android_java_path("CastingNsdPublisher")
        process = subprocess.Popen([sys.executable, str(APP / "mobile_receiver.py"), "--cert", str(work / "cert.pem"),
            "--key", str(work / "key.pem"), "--port-file", str(work / "port"), "--log", str(log)], env=env)
        adb = None
        try:
            port = wait_port(process, work / "port")
            env["CAST_TEST_RECEIVER"] = receiver_fixture(port)
            env["CAST_TEST_DUPLICATE_RECEIVER"] = receiver_fixture(port, duplicate=True)
            env["CAST_TEST_NATIVE_HOST"] = f"osprey-casting-{os.getpid()}.local"
            env["CAST_TEST_NATIVE_PORT"] = str(port)
            if platform == "android":
                serial = env["OSPREY_ANDROID_SERIAL"]
                adb = [str(sdk / "platform-tools/adb"), "-s", serial]
                command(adb + ["reverse", f"tcp:{port}", f"tcp:{port}"])
                try:
                    with android_publisher(port, work, adb, sdk): command([sys.executable, str(APP / "android/test.py")], env=env)
                finally: command(adb + ["reverse", "--remove", f"tcp:{port}"])
            else:
                env.pop("OSPREY_ANDROID_SERIAL", None)
                with bonjour_fixture(port, work): command(["bash", str(APP / "ios/ui-test.sh")], env=env)
            verify_log(log, platform)
            verify_pictures(platform, output)
            if platform == "ios": verify_skin_palettes(output)
        finally:
            try:
                if adb is not None: command(adb + ["shell", "am", "force-stop", "org.ospreylang.casting"], timeout=15)
            finally:
                stop_process(process)


def wait_port(process, path):
    for _ in range(100):
        if path.exists(): return int(path.read_text())
        if process.poll() is not None: raise RuntimeError("Cast receiver fixture exited before binding")
        time.sleep(0.05)
    raise RuntimeError("Cast receiver fixture did not bind")


def verify_load_metadata(records, kinds):
    """[CAST-MOBILE-GPUI] Every LOAD command must be answered with its exact media metadata."""
    loads = [record for record in records if record.get("event") == "load"]
    assert loads and len(loads) == kinds.count("LOAD"), loads
    for record in loads:
        assert record.get("title") == "Casting mobile test", record
        assert record.get("contentType") == "audio/mpeg", record


def verify_log(path, platform):
    records = [json.loads(line) for line in path.read_text().splitlines()]
    errors = [record for record in records if record.get("event") == "error"]
    assert not errors, errors
    remote = [record["level"] for record in records if record.get("event") == "remote-volume"]
    assert remote == [], remote
    kinds = [record["payload"].get("type") for record in records if record.get("event") == "recv"]
    verify_load_metadata(records, kinds)
    if platform == "android":
        # [CAST-MOBILE-GPUI] the journey in android/test.py waits for each command itself.
        for kind in ["LAUNCH", "LOAD", "PAUSE", "PLAY", "SEEK", "SET_VOLUME", "SET_PLAYBACK_RATE", "STOP"]:
            assert kind in kinds, (kind, kinds)
        assert kinds.count("LOAD") == 2 and kinds.count("PAUSE") == 2 and kinds.count("PLAY") == 2 and kinds.count("STOP") == 2, kinds
        print("PASS: Android GPUI Cast commands")
        return
    for kind in ["LAUNCH", "LOAD", "PAUSE", "PLAY", "SEEK", "SET_VOLUME", "SET_PLAYBACK_RATE", "STOP"]:
        assert kind in kinds, (kind, kinds)
    assert kinds.count("LOAD") == 7, kinds
    payloads = [record["payload"] for record in records if record.get("event") == "recv"]
    assert any(item.get("type") == "SET_VOLUME" and abs(item["volume"]["level"] - 0.5) < 0.02 for item in payloads), payloads
    assert any(item.get("type") == "SEEK" and abs(item["currentTime"] - 50) < 3 for item in payloads), payloads
    assert any(item.get("type") == "SET_PLAYBACK_RATE" and item["playbackRate"] == 1.5 for item in payloads), payloads
    assert kinds.count("STOP") == 7, kinds
    assert kinds.count("PAUSE") == 3, kinds
    assert kinds.count("PLAY") == 2, kinds
    assert kinds.count("SEEK") == 3, kinds
    volumes = [item["volume"]["level"] for item in payloads if item.get("type") == "SET_VOLUME"]
    assert len(volumes) == 3, volumes
    assert abs(volumes[0] - 0.5) < 0.02 and volumes[1:] == [0.5, 0.25], volumes
    seeks = [item["currentTime"] for item in payloads if item.get("type") == "SEEK"]
    assert abs(seeks[0] - 50) < 3 and seeks[1:] == [10, 0], seeks
    rates = [item["playbackRate"] for item in payloads if item.get("type") == "SET_PLAYBACK_RATE"]
    assert rates == [1.5, 1.5, 0.75, 1.25], rates
    print("PASS: native app Cast commands, replay, metadata and real HTTP HEAD/range fetches")


def player_pixels(path):
    width, height, raw = png_pixels(path)
    # The Sherbet artboard is warm cream; the native host and OS bars are neutral.
    # Both side samples must match, so a status icon cannot become the crop.
    def cream(at):
        r, g, b = raw[at:at + 3]
        return r >= 200 and g >= 180 and b >= 160 and r - b >= 6
    rows = [y for y in range(height) if all(cream((y * width + int(width * x)) * 3) for x in (0.05, 0.95))]
    assert rows, f"No player artboard in {path}"
    # [CAST-MOBILE-GPUI] Exclude the upper system-icon strip from translucent bars.
    # Keep the last cream row: controls and file-drop hints reach the player bottom.
    first, last = rows[0], rows[-1] + 1
    assert last - first >= height * 0.9, (path, width, first, last)
    first = first + height // 20
    return width, last - first, raw[first * width * 3:last * width * 3]


def ios_pictures(output):
    result = Path((output / "result-path.txt").read_text().strip())
    folder = output / (result.stem + "-attachments")
    command(["xcrun", "xcresulttool", "export", "attachments", "--path", str(result), "--output-path", str(folder)])
    manifest = json.loads((folder / "manifest.json").read_text())
    attachments = [item for test in manifest for item in test["attachments"]]
    for name in ("ready", "restarted", "midnight-ready", "midnight-title", "poolside-ready", "poolside-title", "midnight-loaded-title", "midnight-search", "midnight-receiver"):
        matching = [item for item in attachments if item["suggestedHumanReadableName"].startswith(f"casting-ios-{name}_")]
        assert len(matching) == 1, (name, matching)
        (output / f"{name}.png").write_bytes((folder / matching[0]["exportedFileName"]).read_bytes())


def verify_pictures(platform, output):
    if platform == "ios": ios_pictures(output)
    folder = APP / "goldens" / platform
    folder.mkdir(parents=True, exist_ok=True)
    for name in ("ready", "restarted"):
        actual = player_pixels(output / f"{name}.png")
        image = output / f"{name}-artboard.png"
        png_write(image, *actual)
        golden = folder / f"{actual[0]}-ready.png"
        if os.environ.get("OSPREY_UPDATE_GOLDENS") == "1" and name == "ready": golden.write_bytes(image.read_bytes())
        assert golden.exists(), f"Missing {golden}; run the platform's casting-update target and review its images"
        expected = png_pixels(golden)
        fraction = picture_delta(actual, expected)
        local = picture_delta(actual, expected, tile=64)
        assert fraction <= 0.005 and local <= 0.02, f"{platform} {name}: {fraction:.2%} of player pixels differ, worst tile {local:.2%}; inspect {image}"
        print(f"PASS: {platform} {name} golden ({fraction:.3%} differing pixels)")


def verify_foreground(path, colour):
    width, height, raw = png_pixels(path)
    foreground = tuple(int(colour[i:i + 2], 16) for i in (1, 3, 5))
    matches = sum(all(abs(raw[i + channel] - foreground[channel]) <= 16 for channel in range(3)) for i in range(0, len(raw), 3))
    assert matches > width * height * 0.01, (path.name, "foreground", matches, width * height)


def verify_skin_palettes(output):
    skins = json.loads((APP / "player.machine.json").read_text())["skins"]
    for name in ("midnight", "poolside"):
        width, height, raw = png_pixels(output / f"{name}-ready.png")
        at = (int(height * 0.5) * width + int(width * 0.05)) * 3
        background = tuple(int(skins[name]["tokens"]["background"][i:i + 2], 16) for i in (1, 3, 5))
        assert all(abs(raw[at + channel] - background[channel]) <= 3 for channel in range(3)), (name, "background", tuple(raw[at:at + 3]), background)
        verify_foreground(output / f"{name}-title.png", skins[name]["tokens"]["text"])
        print(f"PASS: {name} native background and foreground palette")
    for name, token in [("loaded-title", "text"), ("search", "text"), ("receiver", "secondary")]:
        verify_foreground(output / f"midnight-{name}.png", skins["midnight"]["tokens"][token])
        print(f"PASS: midnight native {name} foreground palette")


if __name__ == "__main__":
    if len(sys.argv) != 2 or sys.argv[1] not in ("ios", "android"):
        raise SystemExit("Usage: mobile_test.py [ios|android]")
    run(sys.argv[1])
