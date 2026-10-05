#!/usr/bin/env python3
"""Run native bank acceptance tests against the real, freshly seeded API."""
import contextlib
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "target/bank-mobile"
HOLD = Path("/tmp/talon_bank.hold")
API = "http://127.0.0.1:18790"


def require_free_server():
    with socket.socket() as sock:
        if sock.connect_ex(("127.0.0.1", 18790)) == 0:
            raise RuntimeError("Port 18790 is in use; stop your bank demo before testing.")
    if HOLD.exists():
        raise RuntimeError(f"Existing {HOLD}; finish its bank session before testing.")


def compile_server():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    compiler = os.environ.get("OSPREY_BIN", str(ROOT / "target/release/osprey"))
    binary = OUTPUT / "talon-server"
    with (OUTPUT / "server-build.log").open("w") as log:
        subprocess.run([compiler, "examples/projects/modules", "--compile", "-o",
                        str(binary)], cwd=ROOT, stdout=log, stderr=log, check=True)
    return binary


def wait_for_seed(process):
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError("Bank server exited during startup; see server.log.")
        try:
            with urllib.request.urlopen(API + "/api/activity", timeout=1) as reply:
                entries = json.load(reply)
            if any(entry.get("kind") == "refused" for entry in entries):
                return
        except (OSError, ValueError):
            pass
        time.sleep(0.2)
    raise RuntimeError("Bank seed did not finish within 30 seconds; see server.log.")


def stop_server(process):
    HOLD.unlink(missing_ok=True)
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()


@contextlib.contextmanager
def bank_server():
    require_free_server()
    binary = compile_server()
    with (OUTPUT / "server.log").open("w") as log:
        with HOLD.open("x"):
            pass
        try:
            process = subprocess.Popen([str(binary)], cwd=ROOT, stdout=log,
                                       stderr=log, start_new_session=True)
        except BaseException:
            HOLD.unlink(missing_ok=True)
            raise
        try:
            wait_for_seed(process)
            yield
        finally:
            stop_server(process)


def main():
    command = sys.argv[1:]
    if command[:1] == ["--"]:
        command = command[1:]
    if not command:
        raise RuntimeError("Usage: bank-mobile-test.py -- COMMAND [ARGS ...]")
    with bank_server():
        print(f"Talon native acceptance API: {API}", flush=True)
        return subprocess.run(command, cwd=ROOT, check=False).returncode


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Bank mobile tests: {error}\nLogs: {OUTPUT}", file=sys.stderr)
        sys.exit(1)
