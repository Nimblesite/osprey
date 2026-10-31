# osprey: flavor=python
# osprey: namespace=chromecast
# osprey: module=Session
"""A CAST v2 connection to one device.

A device listens for TLS on its CAST port (8009 for most) with a self-signed
certificate, so `Wire.Native` dials it with the runtime's unverified TLS
client ([FFI-TLS-CLIENT], [FFI-TLS-UNVERIFIED]); bytes cross as hex. `open` dials
and sends the virtual-connection CONNECT to `receiver-0`, as the reference
`Conn` does. `receive` returns the next message that is not a heartbeat:
it answers the device's PING with PONG, drops PONGs, and sends its own PING
whenever the device has been quiet for `quiet_ms`, so the connection stays
up through long silences. `poll` takes whatever has arrived without
blocking, for a UI that reads the device between frames.
"""

from chromecast import Protocol, Wire

__all__ = ["QUIET_MS", "Session", "open", "write", "receive", "poll", "close"]

QUIET_MS = 10000
"""How long the device may stay silent before the session sends its own PING, in milliseconds."""


class Session:
    """An open CAST v2 connection: the runtime's TLS session handle."""
    handle: int


class Chunk:
    """A read of an exact byte count: the bytes as hex when `status` is 0;
    otherwise 1 timed out, 2 the device closed, 3 the read failed."""
    hex: str
    status: int


def dial_reason(code):
    """The words for a negative `Wire.dial` result."""
    reasons = ["bad host or port", "host does not resolve", "no socket", "connection refused or timed out", "too many sessions", "TLS unavailable", "TLS handshake failed"]
    return unwrap_or(listGet(reasons, -1 - code), f"connect failed ({code})")


def write(session, dst, ns, payload) -> Result[None, str]:
    """Writes one message to `dst` in namespace `ns` (without the
    `urn:x-cast:com.google.cast.` prefix)."""
    code = perform(Wire.Wire.write(session.handle, Protocol.frame(dst, ns, payload)))
    return Success(value=None) if code == 0 else Error(message=f"send failed ({code})")


def open(host, port) -> Result[Session, str]:
    """A connected session to a device, with the receiver's virtual connection open."""
    handle = perform(Wire.Wire.dial(host, port, QUIET_MS))
    if handle < 1:
        return Error(message=f"{host}:{port}: {dial_reason(handle)}")
    else:
        session = Session(handle=handle)
        match write(session, "receiver-0", "tp.connection", Protocol.connect()):
            case Success():
                return Success(value=session)
            case Error(message=message):
                _closed = perform(Wire.Wire.close(handle))
                return Error(message=message)


def chunk(session, count, timeout_ms) -> Chunk:
    hex = perform(Wire.Wire.read(session.handle, count, timeout_ms))
    return Chunk(hex=hex, status=perform(Wire.Wire.status(session.handle)))


def heartbeat(session, payload):
    return write(session, "receiver-0", "tp.heartbeat", payload)


class Heard:
    """One read: `message` (with `message` set), `heartbeat` (answered or
    dropped) or `quiet` (nothing arrived in time)."""
    kind: str
    message: Protocol.Message


QUIET = Heard(kind="quiet", message=Protocol.Message(namespace="", payload=""))
BEAT = Heard(kind="heartbeat", message=Protocol.Message(namespace="", payload=""))


def handled(session, message) -> Result[Heard, str]:
    """A message that is not a heartbeat is heard; a PING is answered with PONG."""
    kind = Protocol.read(message.payload).kind
    if kind == "PING":
        match heartbeat(session, Protocol.pong()):
            case Success():
                return Success(value=BEAT)
            case Error(message=failure):
                return Error(message=failure)
    elif kind == "PONG":
        return Success(value=BEAT)
    else:
        return Success(value=Heard(kind="message", message=message))


def body(session, header):
    """Reads the body a frame header announces and handles the message."""
    rest = chunk(session, Protocol.frame_length(header), QUIET_MS)
    if rest.status != 0:
        return Error(message="the device stopped mid-frame")
    else:
        return handled(session, Protocol.parse(rest.hex))


def hear(session, wait_ms):
    """One frame, if one starts arriving within `wait_ms`."""
    head = chunk(session, 4, wait_ms)
    if head.status == 0:
        return body(session, head.hex)
    elif head.status == 1:
        return Success(value=QUIET)
    elif head.status == 2:
        return Error(message="device closed the connection")
    else:
        return Error(message="read failed")


def receive(session, quiet_ms) -> Result[Protocol.Message, str]:
    """The next message that is not a heartbeat, PINGing the device whenever
    it has been quiet for `quiet_ms`."""
    match hear(session, quiet_ms):
        case Success(value=heard):
            if heard.kind == "message":
                return Success(value=heard.message)
            elif heard.kind == "heartbeat":
                return receive(session, quiet_ms)
            else:
                match heartbeat(session, Protocol.ping()):
                    case Success():
                        return receive(session, quiet_ms)
                    case Error(message=failure):
                        return Error(message=failure)
        case Error(message=failure):
            return Error(message=failure)


def polled(session, wait_ms, got) -> Result[list[Protocol.Message], str]:
    match hear(session, wait_ms):
        case Success(value=heard):
            if heard.kind == "quiet":
                return Success(value=got)
            elif heard.kind == "heartbeat":
                return polled(session, wait_ms, got)
            else:
                return polled(session, wait_ms, listAppend(got, heard.message))
        case Error(message=failure):
            return Error(message=failure)


def poll(session, wait_ms) -> Result[list[Protocol.Message], str]:
    """Every message that arrives with no gap longer than `wait_ms` (at least
    1), heartbeats handled; an empty list when the device is quiet. It never
    PINGs: the caller keeps the connection alive with `write`."""
    return polled(session, wait_ms, [])


def close(session):
    """Ends the session."""
    return perform(Wire.Wire.close(session.handle))
