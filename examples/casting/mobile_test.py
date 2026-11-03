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
    assert kinds.count("SEEK") == 1 and kinds.count("SET_VOLUME") == 1, kinds
    loads = [record for record in records if record.get("event") == "load"]
    assert all(record["title"] == "Casting mobile test" and record["contentType"] == "audio/mpeg" for record in loads), loads
    print("PASS: native app Cast commands, replay, metadata and real HTTP HEAD/range fetches")


if __name__ == "__main__":
    if len(sys.argv) != 2 or sys.argv[1] not in ("ios", "android"):
        raise SystemExit("Usage: mobile_test.py [ios|android]")
    run(sys.argv[1])
