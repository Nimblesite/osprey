#!/usr/bin/env python3
"""A TLS loopback Cast receiver for real native app E2E tests.

Translates the in-memory device in examples/casting/test/main.ospml (lines
30-115) onto a real CAST v2 TLS socket on 127.0.0.1, framed exactly as
packages/chromecast/src/protocol.py frames it. Each sequential session keeps
its own position, playback rate and volume; LOAD verifies the media file
over HTTP (`adb forward` for Android, the content URL for iOS) and answers
LOAD_FAILED rather than faking success. No sound plays; no real TV involved.
"""
import argparse
import http.client
import io
import ipaddress
import json
import os
from pathlib import Path
import socket
import shlex
import ssl
import struct
import subprocess
from types import SimpleNamespace
import urllib.error
import urllib.parse
import urllib.request

NS_PREFIX = "urn:x-cast:com.google.cast."
APP_ID = "CC1AD845"
FIELDS = {2: "source", 3: "destination", 4: "namespace", 6: "payload"}
MAX_FRAME = 1024 * 1024
TLS_DEADLINE = 10.0
IDLE_LIMIT = 60.0


def log_event(log, **fields):
    """Append one JSONL line; the log is opened line-buffered so it is live."""
    log.write(json.dumps(fields) + "\n")


def text_field(field, text):
    """One length-delimited protobuf field: tag, varint length, UTF-8 bytes."""
    data = text.encode("utf-8")
    out = bytearray([field << 3 | 2])
    length = len(data)
    while length > 0x7F:
        out.append(length & 0x7F | 0x80)
        length >>= 7
    out.append(length)
    return bytes(out) + data


def build_frame(source, destination, namespace, payload):
    """A complete frame, laid out as protocol.py's `frame` builds it."""
    body = (b"\x08\x00" + text_field(2, source) + text_field(3, destination)
            + text_field(4, namespace) + b"\x28\x00" + text_field(6, payload))
    return struct.pack(">I", len(body)) + body


def read_varint(data, start, end):
    """The varint at `start`, strictly inside `end`; (value, next index)."""
    value = shift = 0
    while True:
        if start >= end:
            raise ValueError("varint overruns frame")
        byte = data[start]
        start += 1
        value |= (byte & 0x7F) << shift
        if not byte & 0x80:
            return value, start
        shift += 7
        if shift > 63:
            raise ValueError("varint longer than 64 bits")


def parse_frame(body):
    """Fields 2/3/4/6 of one frame body; varint and length wire types only."""
    message = {"source": "", "destination": "", "namespace": "", "payload": ""}
    i, end = 0, len(body)
    while i < end:
        key, i = read_varint(body, i, end)
        field, wire = key >> 3, key & 7
        if wire == 0:
            _, i = read_varint(body, i, end)
            continue
        if wire != 2:
            raise ValueError(f"unsupported wire type {wire}")
        size, i = read_varint(body, i, end)
        if i + size > end:
            raise ValueError(f"field {field} overruns frame")
        if field in FIELDS:
            try:
                message[FIELDS[field]] = body[i:i + size].decode("utf-8")
            except UnicodeDecodeError as error:
                raise ValueError(f"field {field} is not UTF-8: {error}") from error
        i += size
    return message


def receiver_status(session, request_id):
    """RECEIVER_STATUS naming the Default Media Receiver, as the fake's does."""
    return {"type": "RECEIVER_STATUS", "requestId": request_id, "status": {
        "applications": [{"appId": APP_ID, "sessionId": "S-1", "transportId": "web-7"}],
        "volume": {"level": session["volume"], "muted": False}}}


def media_status(session, request_id):
    """MEDIA_STATUS for the one media session, duration 200 like the fake's."""
    item = {"mediaSessionId": 1, "playerState": session["state"],
            "currentTime": session["position"], "playbackRate": session["rate"],
            "supportedMediaCommands": 15, "media": {"duration": 200.0}}
    if session["idle_reason"]:
        item["idleReason"] = session["idle_reason"]
    return {"type": "MEDIA_STATUS", "requestId": request_id, "status": [item]}


def media_command(session, kind, payload):
    """The fake's state-changing media commands; True when `kind` was one."""
    if kind == "PAUSE":
        session["state"] = "PAUSED"
    elif kind == "PLAY":
        session["state"] = "PLAYING"
    elif kind == "SEEK":
        session["position"] = float(payload.get("currentTime", session["position"]))
    elif kind == "SET_PLAYBACK_RATE":
        session["rate"] = float(payload.get("playbackRate", session["rate"]))
    elif kind == "STOP":
        session["state"], session["idle_reason"] = "IDLE", "CANCELLED"
    else:
        return False
    return True


def handle(session, msg, payload, log):
    """The fake's `answer`: the replies for one decoded message."""
    kind = payload.get("type", "")
    request_id = payload.get("requestId", 0)
    if kind == "PING":
        return [("tp.heartbeat", {"type": "PONG"})]
    if kind in ("LAUNCH", "GET_STATUS") and msg["namespace"] == NS_PREFIX + "receiver":
        return [("receiver", receiver_status(session, request_id))]
    if kind == "GET_STATUS":
        return [("media", media_status(session, request_id))]
    if kind == "LOAD":
        return load_reply(session, payload, request_id, log)
    if kind == "SET_VOLUME":
        session["volume"] = float((payload.get("volume") or {}).get("level", session["volume"]))
        return [("receiver", receiver_status(session, request_id))]
    if media_command(session, kind, payload):
        return [("media", media_status(session, request_id))]
    return []


def load_reply(session, payload, request_id, log):
    """LOAD: verify the media, then BUFFERING and PLAYING; LOAD_FAILED when unverifiable."""
    media = payload.get("media") or {}
    content_id = str(media.get("contentId", ""))
    metadata = media.get("metadata") or {}
    log_event(log, event="load", contentId=content_id,
              contentType=str(media.get("contentType", "")), title=str(metadata.get("title", "")))
    failure = verify_media(content_id)
    if failure is not None:
        log_event(log, event="error", message=f"media fetch failed: {failure}")
        return [("media", {"type": "LOAD_FAILED", "requestId": request_id, "reason": failure})]
    session["state"] = "BUFFERING"
    replies = [("media", media_status(session, request_id))]
    session["state"] = "PLAYING"
    return replies + [("media", media_status(session, request_id))]


def adb(serial, arguments):
    """Run one adb command for the device; (stdout, None) or ("", error)."""
    command = ["adb", "-s", serial] + arguments
    try:
        done = subprocess.run(command, capture_output=True, text=True, timeout=TLS_DEADLINE)
        return done.stdout.strip(), None if done.returncode == 0 else f"adb failed: {done.stderr.strip()}"
    except (OSError, subprocess.TimeoutExpired) as error:
        return "", f"adb {' '.join(arguments)}: {error}"


def forward_port(serial, device_port):
    """The local port adb forwarded to `device_port` on the Android device."""
    output, error = adb(serial, ["forward", "tcp:0", f"tcp:{device_port}"])
    if error:
        return 0, error
    try:
        return int(output.split(":")[-1]), None
    except ValueError:
        return 0, f"adb forward failed: {output}"


def native_http(serial, request, timeout):
    jar = os.environ["CAST_TEST_NATIVE_JAR"]
    arguments = ["env", "CLASSPATH=" + jar, "app_process", "/", "CastingNsdPublisher$MediaCheck",
                 request.get_method(), request.full_url, request.get_header("Range") or ""]
    try:
        result = subprocess.run(["adb", "-s", serial, "exec-out", shlex.join(arguments)],
                                capture_output=True, timeout=timeout)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise urllib.error.URLError(str(error)) from error
    if result.returncode != 0: raise urllib.error.URLError(result.stderr.decode(errors="replace"))
    response = http.client.HTTPResponse(SimpleNamespace(makefile=lambda _: io.BytesIO(result.stdout)))
    response.begin()
    return response


def http_checks(url, opener=None):
    """Require exact fixture metadata and bytes, not merely successful headers."""
    try:
        open_url = opener or urllib.request.urlopen
        expected = Path(os.environ["CAST_TEST_AUDIO_SOURCE"]).read_bytes()
        with open_url(urllib.request.Request(url, method="HEAD"), timeout=TLS_DEADLINE) as head:
            if (head.status, head.headers.get_content_type(), head.headers.get("Content-Length")) != (200, "audio/mpeg", str(len(expected))):
                return f"Incorrect HEAD response: {head.status} {head.headers}"
        request = urllib.request.Request(url, headers={"Range": "bytes=0-15"})
        with open_url(request, timeout=TLS_DEADLINE) as ranged:
            if (ranged.status, ranged.headers.get("Content-Range"), ranged.read()) != (206, f"bytes 0-15/{len(expected)}", expected[:16]):
                return "Range response did not contain the exact requested MP3 bytes"
        return None
    except (urllib.error.URLError, OSError, ValueError, KeyError, http.client.HTTPException) as error:
        return f"media fetch failed: {error}"


def verify_media(content_id):
    """Fetch routed Android URLs on-device; forward loopback fixtures; fetch iOS directly."""
    serial = os.environ.get("OSPREY_ANDROID_SERIAL", "")
    if not serial:
        return http_checks(content_id)
    try:
        location = urllib.parse.urlsplit(content_id)
        device_port = location.port
    except ValueError:
        return f"contentId has no port to forward: {content_id}"
    try: loopback = ipaddress.ip_address(location.hostname).is_loopback
    except ValueError: loopback = location.hostname == "localhost"
    if not loopback:
        return http_checks(content_id, opener=lambda request, timeout: native_http(serial, request, timeout))
    local_port, error = forward_port(serial, device_port)
    if error:
        return error
    try:
        failure = http_checks(f"http://127.0.0.1:{local_port}/media")
    finally:
        _, cleanup = adb(serial, ["forward", "--remove", f"tcp:{local_port}"])
    return failure if cleanup is None else cleanup if failure is None else f"{failure}; {cleanup}"


def send_replies(tls, msg, replies, log):
    """Frame, log and send each reply to the message's sender."""
    for namespace, payload in replies:
        text = json.dumps(payload)
        log_event(log, event="send", namespace=NS_PREFIX + namespace, source="receiver-0",
                  destination=msg["source"], type=payload.get("type"), payload=payload)
        try:
            tls.sendall(build_frame("receiver-0", msg["source"], NS_PREFIX + namespace, text))
        except OSError as error:
            log_event(log, event="error", message=f"send: {error}")
            return False
    return True


def deliver(session, body, tls, log):
    """Parse one frame body, log it and answer it; False closes the session."""
    try:
        msg = parse_frame(body)
        payload = json.loads(msg["payload"])
        if not isinstance(payload, dict):
            raise ValueError("Cast payload must be a JSON object")
        log_event(log, event="recv", source=msg["source"], destination=msg["destination"],
                  namespace=msg["namespace"], payload=payload)
        return send_replies(tls, msg, handle(session, msg, payload, log), log)
    except ValueError as error:
        log_event(log, event="error", message=f"malformed message: {error}")
        return False


def process_frames(session, buffer, tls, log):
    """Handle every complete frame in `buffer`; the tail, or None when fatal."""
    while len(buffer) >= 4:
        length = struct.unpack(">I", buffer[:4])[0]
        if length > MAX_FRAME:
            log_event(log, event="error", message=f"frame length {length} exceeds {MAX_FRAME}")
            return None
        if len(buffer) < 4 + length:
            break
        if not deliver(session, buffer[4:4 + length], tls, log):
            return None
        buffer = buffer[4 + length:]
    return buffer


def run_session(tls, log):
    """One TLS session: read frames until EOF, an error, or the idle limit."""
    session = {"position": 0.0, "rate": 1.0, "volume": 1.0, "state": "IDLE", "idle_reason": ""}
    buffer, idle = b"", 0.0
    while True:
        try:
            chunk = tls.recv(4096)
        except socket.timeout:
            idle += TLS_DEADLINE
            if idle >= IDLE_LIMIT:
                log_event(log, event="idle")
                return
            continue
        except OSError as error:
            log_event(log, event="error", message=f"recv: {error}")
            return
        if not chunk:
            log_event(log, event="eof")
            return
        idle = 0.0
        buffer = process_frames(session, buffer + chunk, tls, log)
        if buffer is None:
            return


def serve(listener, context, log):
    """Sequential sessions, one at a time, until the process is terminated."""
    while True:
        try:
            conn, _ = listener.accept()
        except OSError as error:
            log_event(log, event="error", message=f"accept: {error}")
            continue
        try:
            conn.settimeout(TLS_DEADLINE)
            run_session(context.wrap_socket(conn, server_side=True), log)
        except OSError as error:
            log_event(log, event="error", message=f"session: {error}")
        finally:
            conn.close()


def main():
    """Parse the four required arguments, bind, write the real port, serve."""
    parser = argparse.ArgumentParser(description="TLS loopback Cast receiver fixture")
    parser.add_argument("--cert", required=True, help="PEM certificate for the TLS server")
    parser.add_argument("--key", required=True, help="PEM private key for the TLS server")
    parser.add_argument("--port-file", required=True, help="file the bound port is written to")
    parser.add_argument("--log", required=True, help="JSONL file every message is logged to")
    args = parser.parse_args()
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(args.cert, args.key)
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.bind(("127.0.0.1", 0))
    listener.listen(4)
    port = listener.getsockname()[1]
    with open(args.port_file, "w") as port_file:
        port_file.write(f"{port}\n")
    with open(args.log, "a", buffering=1) as log:
        log_event(log, event="listening", port=port)
        serve(listener, context, log)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        pass
