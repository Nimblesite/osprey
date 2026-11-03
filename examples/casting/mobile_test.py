#!/usr/bin/env python3
"""[CAST-MOBILE-DELIVERY] Native app journeys against a real TLS Cast receiver."""
import base64
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
APP = ROOT / "examples/casting"


def command(arguments, **options):
    return subprocess.run(arguments, check=True, **options)


def receiver_fixture(port):
    text = f'Test._googlecast._tcp SRV 0 0 {port} 127.0.0.1\nTest._googlecast._tcp TXT "fn=Test receiver"\n'
    return base64.b64encode(text.encode()).decode()


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
        process = subprocess.Popen([sys.executable, str(APP / "mobile_receiver.py"), "--cert", str(work / "cert.pem"),
            "--key", str(work / "key.pem"), "--port-file", str(work / "port"), "--log", str(log)], env=env)
        try:
            port = wait_port(process, work / "port")
            env["CAST_TEST_RECEIVER"] = receiver_fixture(port)
            if platform == "android":
                serial = env["OSPREY_ANDROID_SERIAL"]
                adb = [str(sdk / "platform-tools/adb"), "-s", serial]
                command(adb + ["reverse", f"tcp:{port}", f"tcp:{port}"])
                try: command([sys.executable, str(APP / "android/test.py")], env=env)
                finally: command(adb + ["reverse", "--remove", f"tcp:{port}"])
            else:
                env.pop("OSPREY_ANDROID_SERIAL", None)
                command(["bash", str(APP / "ios/ui-test.sh")], env=env)
            verify_log(log, platform)
            verify_pictures(platform, output)
        finally:
            process.terminate()
            try: process.wait(timeout=5)
            except subprocess.TimeoutExpired: process.kill(); process.wait()


def wait_port(process, path):
    for _ in range(100):
        if path.exists(): return int(path.read_text())
        if process.poll() is not None: raise RuntimeError("Cast receiver fixture exited before binding")
        time.sleep(0.05)
    raise RuntimeError("Cast receiver fixture did not bind")


def verify_log(path, platform):
    records = [json.loads(line) for line in path.read_text().splitlines()]
    errors = [record for record in records if record.get("event") == "error"]
    assert not errors, errors
    kinds = [record["payload"].get("type") for record in records if record.get("event") == "recv"]
    for kind in ["LAUNCH", "LOAD", "PAUSE", "PLAY", "SEEK", "SET_VOLUME", "SET_PLAYBACK_RATE", "STOP"]:
        assert kind in kinds, (kind, kinds)
    assert kinds.count("LOAD") == (3 if platform == "ios" else 2), kinds
    payloads = [record["payload"] for record in records if record.get("event") == "recv"]
    assert any(item.get("type") == "SET_VOLUME" and abs(item["volume"]["level"] - 0.5) < 0.02 for item in payloads), payloads
    assert any(item.get("type") == "SEEK" and abs(item["currentTime"] - 50) < 3 for item in payloads), payloads
    assert any(item.get("type") == "SET_PLAYBACK_RATE" and item["playbackRate"] == 1.5 for item in payloads), payloads
    assert kinds.count("STOP") >= 2, kinds
    assert kinds.count("SEEK") == 1, kinds
    volumes = [item["volume"]["level"] for item in payloads if item.get("type") == "SET_VOLUME"]
    assert len(volumes) == (2 if platform == "android" else 1), volumes
    if platform == "android": assert abs(volumes[1] - volumes[0] - 0.01) < 0.001, volumes
    loads = [record for record in records if record.get("event") == "load"]
    assert all(record["title"] == "Casting mobile test" and record["contentType"] == "audio/mpeg" for record in loads), loads
    print("PASS: native app Cast commands, replay, metadata and real HTTP HEAD/range fetches")


def png_pixels(path):
    probe = command(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height",
                     "-of", "json", str(path)], stdout=subprocess.PIPE)
    size = json.loads(probe.stdout)["streams"][0]
    raw = command(["ffmpeg", "-v", "error", "-i", str(path), "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"], stdout=subprocess.PIPE).stdout
    width, height = size["width"], size["height"]
    assert len(raw) == width * height * 3, (path, width, height, len(raw))
    return width, height, raw


def player_pixels(path):
    width, height, raw = png_pixels(path)
    # The Sherbet artboard is warm cream; the native host and OS bars are neutral.
    # Both side samples must match, so a status icon cannot become the crop.
    def cream(at):
        r, g, b = raw[at:at + 3]
        return r >= 200 and g >= 180 and b >= 160 and r - b >= 6
    rows = [y for y in range(height) if all(cream((y * width + int(width * x)) * 3) for x in (0.05, 0.95))]
    assert rows, f"No player artboard in {path}"
    first, last = rows[0], rows[-1] + 1
    assert abs((last - first) - width * 1113 / 868) < 6, (path, width, first, last)
    return width, last - first, raw[first * width * 3:last * width * 3]


def png_write(path, width, height, pixels):
    command(["ffmpeg", "-v", "error", "-y", "-f", "rawvideo", "-pixel_format", "rgb24", "-video_size", f"{width}x{height}",
             "-i", "pipe:0", "-frames:v", "1", str(path)], input=pixels)


def picture_delta(actual, expected, tile=0):
    assert actual[:2] == expected[:2], (actual[:2], expected[:2])
    width, height = actual[:2]
    one, two = actual[2], expected[2]
    changed = [any(abs(one[i + channel] - two[i + channel]) > 24 for channel in range(3)) for i in range(0, len(one), 3)]
    if not tile: return sum(changed) / (width * height)
    def region(x, y):
        right, bottom = min(width, x + tile), min(height, y + tile)
        count = sum(sum(changed[row * width + x:row * width + right]) for row in range(y, bottom))
        return count / ((right - x) * (bottom - y))
    return max(region(x, y) for y in range(0, height, tile) for x in range(0, width, tile))


def ios_pictures(output):
    result = Path((output / "result-path.txt").read_text().strip())
    folder = output / (result.stem + "-attachments")
    command(["xcrun", "xcresulttool", "export", "attachments", "--path", str(result), "--output-path", str(folder)])
    manifest = json.loads((folder / "manifest.json").read_text())
    attachments = [item for test in manifest for item in test["attachments"]]
    for name in ("ready", "restarted"):
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


if __name__ == "__main__":
    if len(sys.argv) != 2 or sys.argv[1] not in ("ios", "android"):
        raise SystemExit("Usage: mobile_test.py [ios|android]")
    run(sys.argv[1])
