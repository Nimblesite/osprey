# osprey: flavor=python
# osprey: namespace=chromecast
# osprey: module=Session
"""A CAST v2 connection to one device.

A device listens for TLS on its CAST port (8009 for most) with a self-signed
certificate, so the session uses the runtime's unverified TLS client
([FFI-TLS-CLIENT], [FFI-TLS-UNVERIFIED]); bytes cross as hex. `open` dials
and sends the virtual-connection CONNECT to `receiver-0`, as the reference
`Conn` does. `receive` returns the next message that is not a heartbeat:
it answers the device's PING with PONG, drops PONGs, and sends its own PING
whenever the device has been quiet for `quiet_ms`, so the connection stays
up through long silences.
"""

from chromecast import Hex, Protocol

__all__ = ["QUIET_MS", "Session", "open", "write", "receive", "close"]

QUIET_MS = 10000
"""How long the device may stay silent before the session sends its own PING, in milliseconds."""


@extern
def osprey_tls_connect(host: str, port: int, timeout_ms: int) -> int:
    ...


@extern
def osprey_tls_send_hex(handle: int, hex: str) -> int:
    ...


@extern
def osprey_tls_recv_hex(handle: int, count: int, timeout_ms: int) -> str:
    ...


@extern
def osprey_tls_status(handle: int) -> int:
    ...


@extern
def osprey_tls_close(handle: int) -> int:
    ...


class Session:
    """An open CAST v2 connection: the runtime's TLS session handle."""
    handle: int


class Chunk:
    """A read of an exact byte count: the bytes as hex when `status` is 0;
    otherwise 1 timed out, 2 the device closed, 3 the read failed."""
    hex: str
    status: int


def dial_reason(code):
    """The words for a negative `osprey_tls_connect` result."""
    reasons = ["bad host or port", "host does not resolve", "no socket", "connection refused or timed out", "too many sessions", "TLS unavailable", "TLS handshake failed"]
    return unwrap_or(listGet(reasons, -1 - code), f"connect failed ({code})")


def write(session, dst, ns, payload) -> Result[None, str]:
    """Writes one message to `dst` in namespace `ns` (without the
    `urn:x-cast:com.google.cast.` prefix)."""
    code = osprey_tls_send_hex(session.handle, Protocol.frame(dst, ns, payload))
    return Success(value=None) if code == 0 else Error(message=f"send failed ({code})")


def open(host, port) -> Result[Session, str]:
    """A connected session to a device, with the receiver's virtual connection open."""
    handle = osprey_tls_connect(host, port, QUIET_MS)
    if handle < 1:
        return Error(message=f"{host}:{port}: {dial_reason(handle)}")
    else:
        session = Session(handle=handle)
        match write(session, "receiver-0", "tp.connection", Protocol.connect()):
            case Success():
                return Success(value=session)
            case Error(message=message):
                _closed = osprey_tls_close(handle)
                return Error(message=message)


def chunk(session, count, timeout_ms) -> Chunk:
    hex = osprey_tls_recv_hex(session.handle, count, timeout_ms)
    return Chunk(hex=hex, status=osprey_tls_status(session.handle))


def heartbeat(session, payload):
    return write(session, "receiver-0", "tp.heartbeat", payload)


def handled(session, message, quiet_ms):
    """A message that is not a heartbeat is returned; a PING is answered first."""
    kind = Protocol.read(message.payload).kind
    if kind == "PING":
        match heartbeat(session, Protocol.pong()):
            case Success():
                return receive(session, quiet_ms)
            case Error(message=failure):
                return Error(message=failure)
    elif kind == "PONG":
        return receive(session, quiet_ms)
    else:
        return Success(value=message)


def body(session, header, quiet_ms):
    """Reads the body a frame header announces and handles the message."""
    rest = chunk(session, Protocol.frame_length(header), quiet_ms)
    if rest.status != 0:
        return Error(message="the device stopped mid-frame")
    else:
        return handled(session, Protocol.parse(rest.hex), quiet_ms)


def receive(session, quiet_ms) -> Result[Protocol.Message, str]:
    """The next message that is not a heartbeat."""
    head = chunk(session, 4, quiet_ms)
    if head.status == 0:
        return body(session, head.hex, quiet_ms)
    elif head.status == 1:
        match heartbeat(session, Protocol.ping()):
            case Success():
                return receive(session, quiet_ms)
            case Error(message=failure):
                return Error(message=failure)
    elif head.status == 2:
        return Error(message="device closed the connection")
    else:
        return Error(message="read failed")


def close(session):
    """Ends the session."""
    return osprey_tls_close(session.handle)
