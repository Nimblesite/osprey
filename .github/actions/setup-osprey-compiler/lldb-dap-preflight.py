#!/usr/bin/env python3
"""Prove the installed lldb-dap can start a debug session, before CI leans on it.

The VS Code debugger end-to-end suite drives `osprey --debug` binaries through
lldb-dap. When the adapter itself is broken that suite reports eight identical
"Debug session did not start within 45000ms" timeouts, which reads like an
Osprey regression and is not one: an apt.llvm.org trunk snapshot of lldb-dap 22
never emitted the `initialized` event, so every DAP client waited for it
forever, on every binary — a three-line clang-compiled C program included.

So run exactly the handshake that suite depends on — initialize, the
`initialized` event, configurationDone, launch, a live stack — against a
three-line C program with no Osprey in the picture. A defective adapter then
fails here, named, in seconds, instead of eight test timeouts later.
"""

import json
import queue
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

# Generous next to the 45s the E2E suite allows per session: this is a hang
# detector, not a performance gate.
TIMEOUT_S = 30
PROBE_SOURCE = "int probe(int n) { return n * n; }\nint main(void) { return probe(5) - 25; }\n"

# Every message the adapter has sent. Retained because the ordering of a
# response and an event is the adapter's to choose: lldb-dap may emit
# `initialized` while answering `initialize` or while answering `launch`, and
# both are legal.
SEEN = []


def fail(message):
    print(f"lldb-dap preflight FAILED: {message}", file=sys.stderr)
    raise SystemExit(1)


def encode(seq, command, arguments):
    """One DAP request in its Content-Length framing."""
    body = json.dumps(
        {"seq": seq, "type": "request", "command": command, "arguments": arguments}
    ).encode()
    return b"Content-Length: %d\r\n\r\n%s" % (len(body), body)


def pump(stream, sink):
    """Parse the adapter's framed stdout into `sink` until the stream closes."""
    while True:
        length = None
        for line in iter(stream.readline, b""):
            if line in (b"\r\n", b"\n"):
                break
            if line.lower().startswith(b"content-length:"):
                length = int(line.split(b":")[1])
        if length is None:
            return
        sink.put(json.loads(stream.read(length)))


def await_message(sink, match, what):
    """The first message satisfying `match`, from what has arrived or is coming."""
    for message in SEEN:
        if match(message):
            return message
    deadline = time.monotonic() + TIMEOUT_S
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            fail(f"the adapter never sent {what}")
        try:
            message = sink.get(timeout=remaining)
        except queue.Empty:
            fail(f"the adapter never sent {what}")
        SEEN.append(message)
        if match(message):
            return message


def is_response(command):
    return lambda m: m.get("type") == "response" and m.get("command") == command


def is_event(name):
    return lambda m: m.get("type") == "event" and m.get("event") == name


def build_probe(work):
    """Compile the C program the session debugs; its DWARF is clang's, not ours."""
    source = Path(work, "probe.c")
    source.write_text(PROBE_SOURCE)
    binary = Path(work, "probe")
    build = subprocess.run(
        ["clang", "-g", "-O0", "-o", str(binary), str(source)],
        capture_output=True,
        text=True,
    )
    if build.returncode != 0:
        fail(f"could not build the probe program: {build.stderr.strip()}")
    return str(binary)


def expect_success(response, what):
    if not response.get("success"):
        fail(f"{what} was rejected: {response.get('message', 'no reason given')}")
    return response


def run_handshake(dap, binary, work):
    """Drive initialize -> initialized -> launch -> stack, or fail saying which step."""
    adapter = subprocess.Popen(
        [dap],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        cwd=work,
    )
    inbox = queue.Queue()
    threading.Thread(target=pump, args=(adapter.stdout, inbox), daemon=True).start()
    counter = iter(range(1, 1_000))

    def send(command, arguments):
        adapter.stdin.write(encode(next(counter), command, arguments))
        adapter.stdin.flush()

    try:
        send("initialize", {"adapterID": "lldb-dap", "clientID": "osprey-preflight",
                            "linesStartAt1": True, "columnsStartAt1": True,
                            "pathFormat": "path"})
        expect_success(await_message(inbox, is_response("initialize"), "an `initialize` response"),
                       "initialize")
        send("launch", {"program": binary, "cwd": work, "stopOnEntry": True})
        await_message(inbox, is_event("initialized"), "the `initialized` event")
        send("configurationDone", {})
        expect_success(await_message(inbox, is_response("launch"), "a `launch` response"), "launch")
        thread_id = first_thread(inbox, send)
        send("stackTrace", {"threadId": thread_id, "startFrame": 0, "levels": 20})
        frames = expect_success(
            await_message(inbox, is_response("stackTrace"), "a `stackTrace` response"),
            "stackTrace",
        ).get("body", {}).get("stackFrames", [])
        if not frames:
            fail("the stopped program reported an empty stack")
        send("disconnect", {"terminateDebuggee": True})
    finally:
        adapter.kill()


def first_thread(inbox, send):
    send("threads", {})
    threads = expect_success(
        await_message(inbox, is_response("threads"), "a `threads` response"), "threads"
    ).get("body", {}).get("threads", [])
    if not threads:
        fail("the stopped program reported no threads")
    return threads[0]["id"]


def main():
    dap = sys.argv[1] if len(sys.argv) > 1 else "lldb-dap"
    with tempfile.TemporaryDirectory() as work:
        run_handshake(dap, build_probe(work), work)
    print(f"lldb-dap preflight: {dap} completed a full debug session")


if __name__ == "__main__":
    main()
