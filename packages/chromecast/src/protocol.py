# osprey: flavor=python
# osprey: namespace=chromecast
# osprey: module=Protocol
"""The CAST v2 wire protocol: protobuf framing and the JSON messages.

A frame is a big-endian 32-bit length followed by one `CastMessage`:
protocol version 0, `sender-0` as the source, a destination id, a namespace,
payload type 0 and a UTF-8 JSON payload. `frame` builds that as hex and
`parse` reads it back, exactly as the reference sender's `frame` and `parse`
do. The message builders write the same JSON text the reference's
`json.dumps` writes, so the frames are byte-identical. `read` turns a
received payload into the few facts the sender acts on, and `launch_step`
and `media_step` are the reference loops' decisions, one message at a time.
"""

from chromecast import Hex

__all__ = [
    "APP", "NS", "Message", "Reading", "Step", "MediaStep", "varint", "read_varint", "frame",
    "frame_length", "parse", "quote", "connect", "ping", "pong", "launch", "get_status", "stop",
    "load", "read", "launch_step", "media_step",
]

APP = "CC1AD845"
NS = "urn:x-cast:com.google.cast."
FINISHED = ["FINISHED", "ERROR", "CANCELLED", "INTERRUPTED"]


class Message:
    """One parsed frame: its full namespace and its JSON payload text."""
    namespace: str
    payload: str


class Varint:
    """A decoded varint and the byte index just past it."""
    value: int
    next: int


class Reading:
    """What the sender needs from a received payload: the message type, the
    Default Media Receiver's transport id (when its application is listed),
    every listed session id, the volume level as text, and each media
    status's player state with its idle reason."""
    kind: str
    transport_id: str
    sessions: list[str]
    volume: str
    states: list[str]
    idle_reasons: list[str]


class Step:
    """A launch decision: `error` with the message, `ready` with the transport
    id, or `wait`."""
    kind: str
    value: str


class MediaStep:
    """A media decision: the `STATE` lines a message adds, the last state seen
    afterwards, and how the session ended (`DONE <reason>`, `ERROR`, or "")."""
    lines: list[str]
    last: str
    end: str


def varint(n):
    """The base-128 varint of a non-negative integer, as hex."""
    low = bitAnd(n, 127)
    rest = shiftRightLogical(n, 7)
    if rest == 0:
        return Hex.byte_hex(low)
    else:
        return Hex.byte_hex(bitOr(low, 128)) + varint(rest)


def read_varint(hex, i) -> Varint:
    """The varint starting at byte `i` of a hex text."""
    return read_varint_from(hex, i, 0, 0)


def read_varint_from(hex, i, acc, shift) -> Varint:
    b = Hex.byte_at(hex, i)
    value = bitOr(acc, shiftLeft(bitAnd(b, 127), shift))
    if bitAnd(b, 128) == 0:
        return Varint(value=value, next=i + 1)
    else:
        return read_varint_from(hex, i + 1, value, shift + 7)


def field(number, text):
    """A length-delimited protobuf field: tag, byte length, UTF-8 bytes."""
    return Hex.byte_hex(bitOr(shiftLeft(number, 3), 2)) + varint(length(text)) + Hex.of_text(text)


def int32_hex(n):
    """A big-endian 32-bit length prefix."""
    bytes = [shiftRightLogical(n, 24), shiftRightLogical(n, 16), shiftRightLogical(n, 8), n]
    return foldList(bytes, "", lambda acc, b: acc + Hex.byte_hex(bitAnd(b, 255)))


def frame(dst, ns, payload):
    """A complete frame for `payload` to `dst` in the `ns` namespace, as hex."""
    body = "0800" + field(2, "sender-0") + field(3, dst) + field(4, NS + ns) + "2800" + field(6, payload)
    return int32_hex(Hex.byte_count(body)) + body


def frame_length(header):
    """The body length a 4-byte frame header announces."""
    return foldList(range_bytes(header, 4), 0, lambda acc, b: acc * 256 + b)


def range_bytes(hex, n):
    """The first `n` bytes of a hex text."""
    return toList(map(range(0, n), lambda i: Hex.byte_at(hex, i)))


def parse(body) -> Message:
    """The namespace and payload of a frame body (the bytes after the length)."""
    return parse_fields(body, 0, Hex.byte_count(body), Message(namespace="", payload=""))


def parse_fields(body, i, n, message) -> Message:
    if i >= n:
        return message
    else:
        key = read_varint(body, i)
        if bitAnd(key.value, 7) == 0:
            return parse_fields(body, read_varint(body, key.next).next, n, message)
        else:
            size = read_varint(body, key.next)
            text = Hex.to_text(unwrap_or(substring(body, 2 * size.next, 2 * (size.next + size.value)), ""))
            return parse_fields(body, size.next + size.value, n, keep(message, shiftRightLogical(key.value, 3), text))


def keep(message, number, text):
    """A message with field 4 (namespace) or 6 (payload) filled in."""
    if number == 4:
        return update(message, namespace=text)
    elif number == 6:
        return update(message, payload=text)
    else:
        return message


def escaped(cp):
    """A code point as `json.dumps` writes it: escapes for `"`, `\\`, the
    short controls, `\\u00XX` for other controls and `\\uXXXX` (surrogate
    pairs above U+FFFF) for everything outside ASCII."""
    if cp == 34:
        return "\\\""
    elif cp == 92:
        return "\\\\"
    elif cp == 10:
        return "\\n"
    elif cp == 13:
        return "\\r"
    elif cp == 9:
        return "\\t"
    elif cp == 8:
        return "\\b"
    elif cp == 12:
        return "\\f"
    elif cp < 32 or cp > 126:
        return unicode_escape(cp)
    else:
        return unwrap_or(fromCodePoint(cp), "")


def unicode_escape(cp):
    """`\\uXXXX`, or a surrogate pair for a code point above U+FFFF."""
    if cp > 65535:
        rest = cp - 65536
        return unicode_escape(55296 + shiftRightLogical(rest, 10)) + unicode_escape(56320 + bitAnd(rest, 1023))
    else:
        return "\\u" + Hex.byte_hex(shiftRightLogical(cp, 8)) + Hex.byte_hex(bitAnd(cp, 255))


def quote(text):
    """`text` as a JSON string literal, spelled as `json.dumps` spells it."""
    return "\"" + foldList(Hex.code_points(text), "", lambda acc, cp: acc + escaped(cp)) + "\""


def connect():
    """The CONNECT payload of the `tp.connection` namespace."""
    return "{\"type\": \"CONNECT\"}"


def ping():
    """The PING heartbeat payload."""
    return "{\"type\": \"PING\"}"


def pong():
    """The PONG heartbeat payload."""
    return "{\"type\": \"PONG\"}"


def launch(request_id):
    """LAUNCH the Default Media Receiver."""
    return "{\"type\": \"LAUNCH\", \"appId\": " + quote(APP) + ", \"requestId\": " + toString(request_id) + "}"


def get_status(request_id):
    """Ask the receiver for its status."""
    return "{\"type\": \"GET_STATUS\", \"requestId\": " + toString(request_id) + "}"


def stop(session_id, request_id):
    """STOP one running application session."""
    return "{\"type\": \"STOP\", \"sessionId\": " + quote(session_id) + ", \"requestId\": " + toString(request_id) + "}"


def load(url, title, request_id):
    """LOAD a buffered audio/mpeg stream and start playing it from the top."""
    media = "{\"contentId\": " + quote(url) + ", \"contentType\": \"audio/mpeg\", \"streamType\": \"BUFFERED\", \"metadata\": {\"metadataType\": 3, \"title\": " + quote(title) + "}}"
    return "{\"type\": \"LOAD\", \"requestId\": " + toString(request_id) + ", \"autoplay\": true, \"currentTime\": 0, \"media\": " + media + "}"


def text_at(doc, path):
    """A scalar at `path` as text, or "" when absent."""
    return unwrap_or(jsonGet(doc, path), "")


def transport_of(doc, count):
    """The transport id of the Default Media Receiver among `count` applications."""
    found = filter(range(0, count), lambda i: text_at(doc, f"status.applications[{i}].appId") == APP)
    return fold(found, "", lambda acc, i: text_at(doc, f"status.applications[{i}].transportId"))


def reading_of(doc) -> Reading:
    apps = jsonLength(doc, "status.applications")
    statuses = jsonLength(doc, "status")
    return Reading(
        kind=text_at(doc, "type"),
        transport_id=transport_of(doc, apps),
        sessions=toList(map(range(0, apps), lambda i: text_at(doc, f"status.applications[{i}].sessionId"))),
        volume=text_at(doc, "status.volume.level"),
        states=toList(map(range(0, statuses), lambda i: text_at(doc, f"status[{i}].playerState"))),
        idle_reasons=toList(map(range(0, statuses), lambda i: text_at(doc, f"status[{i}].idleReason"))),
    )


def empty_reading() -> Reading:
    return Reading(kind="", transport_id="", sessions=[], volume="", states=[], idle_reasons=[])


def read(payload) -> Reading:
    """The facts in a received JSON payload; an unreadable payload reads as empty."""
    match jsonParse(payload):
        case Success(value=doc):
            reading = reading_of(doc)
            freed = jsonFree(doc)
            return reading
        case Error(message=message):
            return empty_reading()


def launch_step(reading, payload) -> Step:
    """The reference launch loop's decision for one received message."""
    if reading.kind == "LAUNCH_ERROR":
        return Step(kind="error", value=payload)
    elif reading.kind == "RECEIVER_STATUS" and reading.transport_id != "":
        return Step(kind="ready", value=reading.transport_id)
    else:
        return Step(kind="wait", value="")


def media_status(step, state, reason) -> MediaStep:
    """One media status folded into a step; nothing changes after the end."""
    if step.end != "":
        return step
    else:
        lines = step.lines if state == step.last else listAppend(step.lines, "STATE " + state)
        done = state == "IDLE" and listContains(FINISHED, reason)
        return MediaStep(lines=lines, last=state, end="DONE " + reason if done else "")


def media_step(reading, last) -> MediaStep:
    """The reference media loop's decision for one received message, given
    the last player state it printed."""
    if listContains(["LOAD_FAILED", "LOAD_CANCELLED", "INVALID_REQUEST"], reading.kind):
        return MediaStep(lines=[], last=last, end="ERROR")
    elif reading.kind != "MEDIA_STATUS":
        return MediaStep(lines=[], last=last, end="")
    else:
        start = MediaStep(lines=[], last=last, end="")
        return fold(range(0, len(reading.states)), start, lambda step, i: media_status(step, unwrap_or(listGet(reading.states, i), ""), unwrap_or(listGet(reading.idle_reasons, i), "")))
