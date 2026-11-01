"""Generates packages/chromecast/tests/expected.py from the reference CAST v2 script.

The reference is the stdlib-only `cast.py` Chromecast sender (varint and
protobuf framing, the message parser, mDNS browse parsing and the launch and
media-status decisions). Its pure functions are reproduced here verbatim, the
socket, subprocess and HTTP work factored out, and every case below is
computed by them. The Osprey port must produce the same hex frames, the same
parsed namespaces and payloads, the same device list and the same decisions.

Usage: python -I scripts/oracles/oracle.py chromecast   (from the repository root;
writes tests/expected.py through the shared writer in scripts/oracles/expected_module.py)
"""

import json
import ntpath
import platform
import re
import struct

from expected_module import write

APP = "CC1AD845"  # Default Media Receiver
NS = "urn:x-cast:com.google.cast."

# --- Reference functions, as cast.py spells them ------------------------------


def varint(n):
    out = bytearray()
    while True:
        b, n = n & 0x7F, n >> 7
        out.append(b | (0x80 if n else 0))
        if not n:
            return bytes(out)


def frame(dst, ns, payload):
    s = lambda num, v: bytes([num << 3 | 2]) + varint(len(v.encode())) + v.encode()
    body = b"\x08\x00" + s(2, "sender-0") + s(3, dst) + s(4, NS + ns) + b"\x28\x00" + s(6, json.dumps(payload))
    return struct.pack(">I", len(body)) + body


def frame_text(dst, ns, text):
    """`frame` with the payload text given, so a UTF-8 payload can be framed."""
    s = lambda num, v: bytes([num << 3 | 2]) + varint(len(v.encode())) + v.encode()
    body = b"\x08\x00" + s(2, "sender-0") + s(3, dst) + s(4, NS + ns) + b"\x28\x00" + s(6, text)
    return struct.pack(">I", len(body)) + body


def parse(body):
    fields, i = {}, 0
    while i < len(body):
        key, i = read_varint(body, i)
        if key & 7 == 0:
            fields[key >> 3], i = read_varint(body, i)
        else:
            n, i = read_varint(body, i)
            fields[key >> 3], i = body[i:i + n], i + n
    return fields[4].decode(), json.loads(fields[6].decode())


def read_varint(b, i):
    n = shift = 0
    while True:
        n |= (b[i] & 0x7F) << shift
        shift += 7
        i += 1
        if not b[i - 1] & 0x80:
            return n, i


def browse(out):
    """`discover` without the subprocess and the name lookup: (name, host, port)."""
    hosts = dict(re.findall(r"^(\S+)\s+SRV\s+\d+\s+\d+\s+(\d+\s+\S+)", out, re.M))
    names = {k: re.search(r'"fn=([^"]*)"', v).group(1) for k, v in re.findall(r"^(\S+)\s+TXT\s+(.*)$", out, re.M)}
    devs = []
    for k, name in names.items():
        port, host = hosts[k].split()
        devs.append((name.replace("\\ ", " "), host.rstrip("."), int(port)))
    return devs


def launch_step(msg, text):
    """One turn of `launch`'s loop: error with the message, ready with the transport id, or wait."""
    if msg.get("type") == "LAUNCH_ERROR":
        return ("error", text)
    if msg.get("type") != "RECEIVER_STATUS":
        return ("wait", "")
    apps = msg["status"].get("applications", [])
    app = next((a for a in apps if a.get("appId") == APP), None)
    if app:
        return ("ready", app["transportId"])
    return ("wait", "")


def media_steps(msgs):
    """`cast`'s loop over a message sequence: the STATE lines it prints and how it ends."""
    lines, last = [], None
    for msg in msgs:
        if msg.get("type") in ("LOAD_FAILED", "LOAD_CANCELLED", "INVALID_REQUEST"):
            return lines, "ERROR"
        for st in msg.get("status", []) if msg.get("type") == "MEDIA_STATUS" else []:
            state = st.get("playerState")
            if state != last:
                lines.append(f"STATE {state}")
                last = state
            if state == "IDLE" and st.get("idleReason") in ("FINISHED", "ERROR", "CANCELLED", "INTERRUPTED"):
                return lines, f"DONE {st.get('idleReason')}"
    return lines, "OPEN"


# --- The messages the sender builds, as cast.py's dicts -----------------------

MESSAGES = [
    ("receiver-0", "tp.connection", {"type": "CONNECT"}),
    ("receiver-0", "tp.heartbeat", {"type": "PING"}),
    ("receiver-0", "tp.heartbeat", {"type": "PONG"}),
    ("receiver-0", "receiver", {"type": "LAUNCH", "appId": APP, "requestId": 1}),
    ("receiver-0", "receiver", {"type": "GET_STATUS", "requestId": 1}),
    ("receiver-0", "receiver", {"type": "STOP", "sessionId": "7E0C-1F2A", "requestId": 2}),
    ("web-7", "tp.connection", {"type": "CONNECT"}),
    ("web-7", "media", {"type": "LOAD", "requestId": 2, "autoplay": True, "currentTime": 0, "media": {
        "contentId": "http://192.168.1.20:5123/media", "contentType": "audio/mpeg", "streamType": "BUFFERED",
        "metadata": {"metadataType": 3, "title": "Blue Monday"}}}),
    ("web-7", "media", {"type": "LOAD", "requestId": 2, "autoplay": True, "currentTime": 0, "media": {
        "contentId": "http://10.0.0.5:8000/media.m3u8", "contentType": "application/x-mpegURL", "streamType": "BUFFERED",
        "metadata": {"metadataType": 3, "title": "Say \"hi\" \\ bye"}}}),
    # Media controls the reference CLI lacks; the payloads follow the CAST v2 media namespace.
    ("web-7", "media", {"type": "PAUSE", "mediaSessionId": 1, "requestId": 3}),
    ("web-7", "media", {"type": "PLAY", "mediaSessionId": 1, "requestId": 4}),
    ("web-7", "media", {"type": "STOP", "mediaSessionId": 1, "requestId": 5}),
    ("web-7", "media", {"type": "SEEK", "mediaSessionId": 1, "currentTime": 30.5, "requestId": 6}),
    ("web-7", "media", {"type": "SET_PLAYBACK_RATE", "mediaSessionId": 1, "playbackRate": 1.5, "requestId": 7}),
    ("receiver-0", "receiver", {"type": "SET_VOLUME", "volume": {"level": 0.25}, "requestId": 8}),
]

# --- Frames a device sends back ---------------------------------------------------

RECEIVED = [
    ("tp.heartbeat", '{"type":"PING"}'),
    ("receiver", '{"requestId":1,"status":{"applications":[{"appId":"CC1AD845","displayName":"Default Media Receiver",'
                 '"sessionId":"7E0C-1F2A","transportId":"web-7"}],"volume":{"level":0.35,"muted":false}},"type":"RECEIVER_STATUS"}'),
    ("receiver", '{"requestId":1,"status":{"applications":[{"appId":"E8C28D3C","displayName":"Backdrop","sessionId":"A1","transportId":"web-1"}],'
                 '"volume":{"level":1.0,"muted":false}},"type":"RECEIVER_STATUS"}'),
    ("receiver", '{"requestId":1,"status":{"volume":{"level":0.5,"muted":true}},"type":"RECEIVER_STATUS"}'),
    ("receiver", '{"reason":"NOT_FOUND","requestId":1,"type":"LAUNCH_ERROR"}'),
    ("receiver", '{"requestId":1,"status":"LAUNCHING","type":"LAUNCH_STATUS"}'),
    ("media", '{"requestId":2,"status":[{"mediaSessionId":1,"playerState":"BUFFERING","currentTime":0}],"type":"MEDIA_STATUS"}'),
    ("media", '{"requestId":0,"status":[{"mediaSessionId":1,"playerState":"PLAYING","currentTime":1.5}],"type":"MEDIA_STATUS"}'),
    ("media", '{"requestId":0,"status":[{"mediaSessionId":1,"playerState":"IDLE","idleReason":"FINISHED"}],"type":"MEDIA_STATUS"}'),
    ("media", '{"requestId":0,"status":[{"mediaSessionId":1,"playerState":"IDLE","idleReason":"CANCELLED"}],"type":"MEDIA_STATUS"}'),
    ("media", '{"requestId":0,"status":[{"mediaSessionId":1,"playerState":"IDLE"}],"type":"MEDIA_STATUS"}'),
    ("media", '{"requestId":2,"detailedErrorCode":104,"type":"LOAD_FAILED"}'),
    ("media", '{"requestId":2,"type":"INVALID_REQUEST","reason":"INVALID_COMMAND"}'),
    ("media", '{"requestId":0,"status":[],"type":"MEDIA_STATUS"}'),
    ("media", '{"requestId":0,"status":[{"mediaSessionId":1,"playerState":"PAUSED","media":{"metadata":{"title":"Lounge – Café ☕"}}}],'
              '"type":"MEDIA_STATUS"}'),
    ("media", '{"requestId":4,"status":[{"mediaSessionId":3,"playerState":"PLAYING","currentTime":12.25,"playbackRate":1.5,'
              '"supportedMediaCommands":15,"media":{"duration":248.0}}],"type":"MEDIA_STATUS"}'),
]

# Windows paths too: a Windows sender is handed C:\... paths, so the title is cut at either
# separator, as ntpath cuts it.
TITLE_PATHS = ["/Users/me/Music/Blue Monday.mp3", "song.flac", "/tmp/archive.tar.gz", "/tmp/noext", "/a/b/.hidden",
               r"C:\Users\me\Music\Blue Monday.mp3", r"D:\mixed/dir\track.ogg", r"C:\a\.hidden"]

# Sequences `media_steps` judges: indexes into RECEIVED.
MEDIA_RUNS = [[6, 7, 7, 8], [6, 11], [7, 12], [0, 13, 7, 10, 9], [14, 7], [5, 1]]

BROWSE_TEXT = """\
; To direct clients to browse a different domain, substitute that domain in place of '@'
lb._dns-sd._udp                                 PTR     @

; In the list of services below, the SRV records will typically reference dot-local Multicast DNS names.
_googlecast._tcp                                PTR     Living\\032Room\\032TV._googlecast._tcp
Living\\032Room\\032TV._googlecast._tcp         SRV     0 0 8009 1e2f3a4b-5c6d-7e8f-9a0b-1c2d3e4f5a6b.local. ; Replace with unicast FQDN of target host
Living\\032Room\\032TV._googlecast._tcp         TXT     "id=1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b" "cd=ABCDEF" "rm=" "ve=05" "md=Chromecast" "ic=/setup/icon.png" "fn=Living Room TV" "ca=4101" "st=0" "bs=FA8FCA000000" "nf=1" "rs="
_googlecast._tcp                                PTR     Kitchen._googlecast._tcp
Kitchen._googlecast._tcp                        SRV     0 0 8010 kitchen-cast.local. ; Replace with unicast FQDN of target host
Kitchen._googlecast._tcp                        TXT     "id=00ff" "md=Google TV" "fn=Kitchen\\ TV" "ca=4101"
"""


def main():
    hexes = lambda bs: bs.hex()
    varints = [0, 1, 127, 128, 300, 16384, 2 ** 31, 2 ** 40]
    frames = [frame(dst, ns, payload) for dst, ns, payload in MESSAGES]
    received = [frame_text("sender-0", ns, text) for ns, text in RECEIVED]
    parsed = [parse(body[4:]) for body in received]
    launches = [launch_step(json.loads(text), text) for _, text in RECEIVED]
    volumes = [str(json.loads(text).get("status", {}).get("volume", {}).get("level", ""))
               if isinstance(json.loads(text).get("status"), dict) else "" for _, text in RECEIVED]
    statuses = [json.loads(text).get("status") for _, text in RECEIVED]
    firsts = [st[0] if isinstance(st, list) and st else {} for st in statuses]
    media = [[str(f.get(key, "")) for key in ("mediaSessionId", "currentTime", "playbackRate", "supportedMediaCommands")]
             + [str(f.get("media", {}).get("duration", ""))] for f in firsts]
    titles = [ntpath.splitext(ntpath.basename(path))[0] for path in TITLE_PATHS]
    runs = [media_steps([json.loads(RECEIVED[i][1]) for i in run]) for run in MEDIA_RUNS]
    devices = browse(BROWSE_TEXT)
    write(
        "chromecast",
        "The reference Chromecast sender's frames, parses, browse results and decisions.",
        {"python": platform.python_version()},
        {
            "VARINT_IN": varints,
            "VARINT_HEX": [hexes(varint(n)) for n in varints],
            "MESSAGE_DST": [dst for dst, _, _ in MESSAGES],
            "MESSAGE_NS": [ns for _, ns, _ in MESSAGES],
            "MESSAGE_TEXT": [json.dumps(payload) for _, _, payload in MESSAGES],
            "FRAME_HEX": [hexes(body) for body in frames],
            "RECEIVED_HEX": [hexes(body) for body in received],
            "RECEIVED_NS": [ns for ns, _ in parsed],
            "RECEIVED_TEXT": [json.dumps(payload, separators=(",", ":"), ensure_ascii=False) for _, payload in parsed],
            "LAUNCH_KIND": [kind for kind, _ in launches],
            "LAUNCH_VALUE": [value for _, value in launches],
            "MEDIA_RUN_LENGTHS": [len(run) for run in MEDIA_RUNS],
            "MEDIA_RUN_FRAMES": [i for run in MEDIA_RUNS for i in run],
            "MEDIA_RUN_LINES": ["\n".join(lines) for lines, _ in runs],
            "MEDIA_RUN_END": [end for _, end in runs],
            "RECEIVED_VOLUME": volumes,
            "RECEIVED_MEDIA": ["|".join(fields) for fields in media],
            "TITLE_PATH": TITLE_PATHS,
            "TITLE": titles,
            "BROWSE_TEXT": BROWSE_TEXT,
            "BROWSE_NAME": [name for name, _, _ in devices],
            "BROWSE_HOST": [host for _, host, _ in devices],
            "BROWSE_PORT": [port for _, _, port in devices],
        },
        {
            "VARINT_IN": "Integers whose varint encodings are checked.",
            "VARINT_HEX": "The reference `varint` of each VARINT_IN entry, as hex.",
            "MESSAGE_DST": "Destination id of each message the sender builds.",
            "MESSAGE_NS": "Namespace suffix of each message the sender builds.",
            "MESSAGE_TEXT": "The JSON text `json.dumps` writes for each message's payload.",
            "FRAME_HEX": "The reference `frame` of each message: length prefix and protobuf body, as hex.",
            "RECEIVED_HEX": "Frames a device sends, as hex, in RECEIVED order.",
            "RECEIVED_NS": "The full namespace the reference `parse` reads from each received frame.",
            "RECEIVED_TEXT": "The compact JSON the reference `parse` reads from each received frame.",
            "LAUNCH_KIND": "The reference launch loop's decision for each received payload: error, ready or wait.",
            "LAUNCH_VALUE": "The transport id (ready) or the message (error) that decision carries.",
            "MEDIA_RUN_LENGTHS": "How many RECEIVED indexes each media run below holds.",
            "MEDIA_RUN_FRAMES": "The RECEIVED indexes of every media run, run after run.",
            "MEDIA_RUN_LINES": "The STATE lines the reference media loop prints for each run, newline-joined.",
            "MEDIA_RUN_END": "How the reference media loop ends each run: DONE <reason>, ERROR or OPEN.",
            "RECEIVED_VOLUME": "`str(status.volume.level)` of each received payload, or \"\" when it has none.",
            "RECEIVED_MEDIA": "`str` of the first media status's mediaSessionId, currentTime, playbackRate, supportedMediaCommands and media.duration, |-joined (\"\" when absent).",
            "TITLE_PATH": "Media paths whose titles are checked.",
            "TITLE": "The reference title of each path: the base name without its extension.",
            "BROWSE_TEXT": "A `dns-sd -Z _googlecast._tcp local` transcript with two devices.",
            "BROWSE_NAME": "Each device's friendly name from the reference browse parsing.",
            "BROWSE_HOST": "Each device's host name, trailing dot removed.",
            "BROWSE_PORT": "Each device's port.",
        },
    )
