#!/usr/bin/env python3
"""[CAST-MOBILE-GPUI] Each skin on Android against the same skin on desktop at the same logical size.

Both are src/toolchain/hosts/gpui, so only the platform's fonts may differ. Run on an emulator named by OSPREY_ANDROID_SERIAL:
    python3 src/osprey/examples/casting/skin_parity.py
"""
import base64
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import time

APP = Path(__file__).resolve().parent
ROOT = APP.parents[3]
SDK = Path(os.environ.get("ANDROID_HOME", Path.home() / "Library/Android/sdk"))
ADB = [str(SDK / "platform-tools/adb"), "-s", os.environ["OSPREY_ANDROID_SERIAL"]]
OUTPUT = APP / "build/skin-parity"
PACKAGE = "org.ospreylang.casting"
SKINS = list(json.loads((APP / "player.machine.json").read_text())["skins"])
BROWSE = "\n"  # No receivers on either host, so a real TV on this network cannot change the picture.
# Structural similarity of the whole screen. Text is drawn in each platform's own font, so it is not 1.
LEAST_SIMILAR = 0.85


def run(*arguments, **options):
    return subprocess.run(arguments, check=True, capture_output=True, text=True, **options).stdout


def android_shot(skin, path):
    apk = run("bash", str(APP / "android/build.sh"), "android-arm64", env=dict(os.environ, OSPREY_CASTING_SKIN=skin, OSPREY_MOBILE_SKIP_LIBRARY="1")).strip().splitlines()[-1]
    run(*ADB, "install", "-r", apk)
    run(*ADB, "shell", "am", "force-stop", PACKAGE)
    run(*ADB, "logcat", "-c")
    fixture = base64.b64encode(BROWSE.encode()).decode()
    run(*ADB, "shell", "am", "start", "-W", "-n", PACKAGE + "/.CastingActivity", "--es", "testReceiver", fixture)
    time.sleep(10)
    path.write_bytes(subprocess.run([*ADB, "exec-out", "screencap", "-p"], check=True, capture_output=True).stdout)
    found = re.findall(r"safe area top (\d+) bottom (\d+) px", run(*ADB, "logcat", "-d"))
    assert found, "The GPUI backend reported no safe area"
    return tuple(map(int, found[-1]))


WINDOW_ID = """
import CoreGraphics
let owner = CommandLine.arguments[1]
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
let found = windows.first { ($0["kCGWindowOwnerPID"] as? Int).map(String.init) == owner && ($0["kCGWindowLayer"] as? Int) == 0 }
print(found?["kCGWindowNumber"] as? Int ?? 0)
"""


def desktop_shot(skin, size, folder):
    """The real window by its id, which needs no focus and no input."""
    finder = folder / "window.swift"
    finder.write_text(WINDOW_ID)
    environment = dict(os.environ, OSPREY_GPUI_WINDOW=f"{size[0]}x{size[1]}")
    process = subprocess.Popen(["bash", str(APP / "run.sh"), "--skin", skin, "--browse", "/usr/bin/true"], env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    try:
        deadline = time.monotonic() + 240
        window = 0
        while time.monotonic() < deadline and not window:
            time.sleep(3)
            hosts = subprocess.run(["pgrep", "-g", str(process.pid), "-f", "osprey-gpui"], capture_output=True, text=True).stdout.split()
            for host in hosts:
                window = window or int(run("swift", str(finder), host).strip() or 0)
        assert window, "The desktop player never opened a window"
        time.sleep(8)
        path = folder / "desktop.png"
        run("screencapture", "-x", "-o", "-l", str(window), str(path))
        return path
    finally:
        os.killpg(process.pid, signal.SIGTERM)


def similarity(android, desktop, size, bars):
    top, bottom = bars
    chain = (f"[0:v]crop=iw:ih-{top + bottom}:0:{top},scale={size[0]}:{size[1]},format=rgb24[a];"
             f"[1:v]scale={size[0]}:{size[1]},format=rgb24[b];[a][b]ssim")
    report = subprocess.run(["ffmpeg", "-v", "info", "-i", str(android), "-i", str(desktop), "-lavfi", chain, "-f", "null", "-"], capture_output=True, text=True).stderr
    return float(re.search(r"All:([\d.]+)", report).group(1))


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    density = int(re.search(r"(\d+)", run(*ADB, "shell", "wm", "density")).group(1)) / 160
    width, height = map(int, re.search(r"(\d+)x(\d+)", run(*ADB, "shell", "wm", "size")).groups())
    failures = []
    for skin in SKINS:
        android = OUTPUT / f"{skin}-android.png"
        bars = android_shot(skin, android)
        size = (round(width / density), round((height - sum(bars)) / density))
        with tempfile.TemporaryDirectory(prefix="casting-parity-") as folder:
            desktop = OUTPUT / f"{skin}-desktop.png"
            desktop.write_bytes(desktop_shot(skin, size, Path(folder)).read_bytes())
        score = similarity(android, desktop, size, bars)
        print(f"{skin}: {score:.3f} at {size[0]}x{size[1]}")
        if score < LEAST_SIMILAR: failures.append(skin)
    assert not failures, f"Mobile differs from desktop in {failures}; see {OUTPUT}"
    print("PASS: every skin matches desktop at the same logical size")


if __name__ == "__main__":
    sys.exit(main())
