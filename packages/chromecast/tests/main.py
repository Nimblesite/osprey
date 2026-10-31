# osprey: flavor=python
"""The @osprey/chromecast suite: the port against the reference CAST v2 sender.

Every frame, parse, browse result and loop decision below was computed by
the reference script's own functions (scripts/oracles/chromecast.py). The
device cases in `chromecast_tests.Device` then drive the whole sender over
real TLS against a scripted device. Everything runs through
`testing.Check.strictly`, so an integer fault stops the suite.
"""
from chromecast import Cast, Discovery, Hex, Protocol
from chromecast_expected import Expected
from chromecast_tests import Device
from testing import Check


def at(xs, i):
    """Element `i` of a string list, "" past its end."""
    return unwrap_or(listGet(xs, i), "")


def at_int(xs, i):
    """Element `i` of an int list, 0 past its end."""
    return unwrap_or(listGet(xs, i), 0)


def each(name, n, case):
    """`case(name.i, i)` for every index below `n`, plus a count check."""
    return listAppend(toList(map(range(0, n), lambda i: case(f"{name}.{i}", i))), Check.passed(f"{name}.count"))


def varint_cases():
    """varint against the reference encoding, and read_varint back to the value."""
    n = len(Expected.VARINT_IN)
    encode = each("varint.encode", n, lambda name, i: Check.equal(name, Protocol.varint(at_int(Expected.VARINT_IN, i)), at(Expected.VARINT_HEX, i)))
    decode = each("varint.decode", n, lambda name, i: Check.equal(name, Protocol.read_varint(at(Expected.VARINT_HEX, i), 0).value, at_int(Expected.VARINT_IN, i)))
    return listConcat(encode, decode)


def built():
    """The messages the sender builds, in the oracle's MESSAGES order."""
    first = [Protocol.connect(), Protocol.ping(), Protocol.pong(), Protocol.launch(1), Protocol.get_status(1), Protocol.stop("7E0C-1F2A", 2), Protocol.connect()]
    return listConcat(first, [Protocol.load("http://192.168.1.20:5123/media", "Blue Monday", 2), Protocol.load("http://10.0.0.5:8000/media", "Say \"hi\" \\ bye", 2)])


def message_cases():
    """Each builder writes json.dumps's text; each frame is the reference frame."""
    texts = built()
    n = len(Expected.MESSAGE_TEXT)
    payloads = each("message.payload", n, lambda name, i: Check.equal(name, at(texts, i), at(Expected.MESSAGE_TEXT, i)))
    frames = each("message.frame", n, lambda name, i: Check.equal(name, Protocol.frame(at(Expected.MESSAGE_DST, i), at(Expected.MESSAGE_NS, i), at(texts, i)), at(Expected.FRAME_HEX, i)))
    return listConcat(payloads, frames)


def received(i):
    """Received frame `i` parsed from its body."""
    return Protocol.parse(drop(at(Expected.RECEIVED_HEX, i), 8))


def parse_cases():
    """parse reads each device frame's namespace and payload; the length prefix matches."""
    n = len(Expected.RECEIVED_HEX)
    spaces = each("parse.namespace", n, lambda name, i: Check.equal(name, received(i).namespace, at(Expected.RECEIVED_NS, i)))
    payloads = each("parse.payload", n, lambda name, i: Check.equal(name, received(i).payload, at(Expected.RECEIVED_TEXT, i)))
    sizes = each("parse.length", n, lambda name, i: Check.equal(name, Protocol.frame_length(at(Expected.RECEIVED_HEX, i)), Hex.byte_count(at(Expected.RECEIVED_HEX, i)) - 4))
    return listConcat(listConcat(spaces, payloads), sizes)


def launch_of(i):
    payload = received(i).payload
    return Protocol.launch_step(Protocol.read(payload), payload)


def reading_cases():
    """The launch decision and the volume the sender reads from each payload."""
    n = len(Expected.RECEIVED_TEXT)
    kinds = each("launch.kind", n, lambda name, i: Check.equal(name, launch_of(i).kind, at(Expected.LAUNCH_KIND, i)))
    values = each("launch.value", n, lambda name, i: Check.equal(name, launch_of(i).value, at(Expected.LAUNCH_VALUE, i)))
    volumes = each("read.volume", n, lambda name, i: Check.equal(name, Protocol.read(received(i).payload).volume, at(Expected.RECEIVED_VOLUME, i)))
    sessions = [Check.equal("read.sessions", Protocol.read(received(1).payload).sessions, ["7E0C-1F2A"]), Check.equal("read.unparseable", Protocol.read("{oops").kind, "")]
    return listConcat(listConcat(listConcat(kinds, values), volumes), sessions)


def run_start(r):
    """Where media run `r` starts in MEDIA_RUN_FRAMES."""
    return fold(range(0, r), 0, lambda acc, k: acc + at_int(Expected.MEDIA_RUN_LENGTHS, k))


def media_run(r):
    """The media loop over run `r`, stopping at the first end, as the reference does."""
    start = Protocol.MediaStep(lines=[], last="", end="")
    frames = range(run_start(r), run_start(r) + at_int(Expected.MEDIA_RUN_LENGTHS, r))
    return fold(frames, start, lambda step, k: step if step.end != "" else media_next(step, Protocol.media_step(Protocol.read(received(at_int(Expected.MEDIA_RUN_FRAMES, k)).payload), step.last)))


def media_next(before, step):
    """A step's lines appended to everything printed before it."""
    return Protocol.MediaStep(lines=listConcat(before.lines, step.lines), last=step.last, end=step.end)


def media_cases():
    """The STATE lines and the ending of every media run."""
    n = len(Expected.MEDIA_RUN_END)
    lines = each("media.lines", n, lambda name, r: Check.equal(name, join(media_run(r).lines, "\n"), at(Expected.MEDIA_RUN_LINES, r)))
    ends = each("media.end", n, lambda name, r: Check.equal(name, media_run(r).end if media_run(r).end != "" else "OPEN", at(Expected.MEDIA_RUN_END, r)))
    return listConcat(lines, ends)


def browse_cases():
    """dns-sd browse parsing, named lookup and a live browse that must not fail."""
    devices = Discovery.browse(Expected.BROWSE_TEXT)
    n = len(Expected.BROWSE_NAME)
    names = each("browse.name", n, lambda name, i: Check.equal(name, device_at(devices, i).name, at(Expected.BROWSE_NAME, i)))
    return listConcat(names, browse_detail_cases(devices))


def device_at(devices, i):
    return unwrap_or(listGet(devices, i), Discovery.Device(name="", host="", port=0))


def browse_detail_cases(devices):
    hosts = toList(map(range(0, len(Expected.BROWSE_HOST)), lambda i: Check.equal(f"browse.host.{i}", device_at(devices, i).host, at(Expected.BROWSE_HOST, i))))
    ports = toList(map(range(0, len(Expected.BROWSE_PORT)), lambda i: Check.equal(f"browse.port.{i}", device_at(devices, i).port, at_int(Expected.BROWSE_PORT, i))))
    found = Check.equal("browse.named", Discovery.named(devices, "Kitchen TV"), Success(value=device_at(devices, 1)))
    missing = Check.equal("browse.missing", Discovery.named(devices, "Attic"), Error(message="no Cast device named Attic"))
    live = Check.truth("discover.runs", len(Discovery.discover(1)) >= 0, "discover failed")
    return listConcat(listConcat(hosts, ports), [found, missing, live, Check.equal("browse.empty", len(Discovery.browse("")), 0)])


def title_cases():
    """The media title is the base name without its extension."""
    return each("title", len(Expected.TITLE), lambda name, i: Check.equal(name, Cast.title(at(Expected.TITLE_PATH, i)), at(Expected.TITLE, i)))


def hex_cases():
    """UTF-8 through hex and back, every sequence width, and JSON quoting."""
    text = "Aé☕😀\x01"
    return [
        Check.equal("hex.of_text", Hex.of_text(text), "41c3a9e29895f09f988001"),
        Check.equal("hex.to_text", Hex.to_text(Hex.of_text(text)), text),
        Check.equal("hex.byte_at", Hex.byte_at("00ff7f", 1), 255),
        Check.equal("hex.byte_hex", Hex.byte_hex(10), "0a"),
        Check.equal("hex.code_points", Hex.code_points(text), [65, 233, 9749, 128512, 1]),
        Check.equal("quote.escapes", Protocol.quote("\"\\\n\r\t\b\f\x01é😀"), "\"\\\"\\\\\\n\\r\\t\\b\\f\\u0001\\u00e9\\ud83d\\ude00\""),
        Check.equal("quote.app", Protocol.APP + Protocol.NS, "CC1AD845urn:x-cast:com.google.cast."),
    ]


def all_cases():
    """Every suite case, in a fixed order for the golden."""
    pure = listConcat(listConcat(listConcat(varint_cases(), message_cases()), parse_cases()), reading_cases())
    more = listConcat(listConcat(listConcat(media_cases(), browse_cases()), title_cases()), hex_cases())
    return listConcat(listConcat(pure, more), Device.cases())


def main():
    match Check.strictly(lambda: Check.report(all_cases())):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
