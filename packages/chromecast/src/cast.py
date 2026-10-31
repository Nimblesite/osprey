# osprey: flavor=python
# osprey: namespace=chromecast
# osprey: module=Cast
"""Casting a local audio file to a Chromecast or Google TV.

`start` does what the reference `cast` does before its loop: serve the file
over HTTP on the interface that reaches the device ([FFI-FILE-SERVER]),
open a session, LAUNCH the Default Media Receiver, connect to its transport
and LOAD the file. `next` waits for the next media decision, and `finish`
closes the session and stops serving. `cast` runs the whole thing and hands
each `STATE` line to a callback, returning how playback ended. `status`
and `stop_all` are the reference's `--status` and `--stop`.

A UI drives `start`/`next`/`finish` from a fiber; a script calls `cast`.
"""

from typing import Callable

from chromecast import Discovery, Protocol, Session

__all__ = ["Casting", "title", "media_url", "launch", "start", "next", "finish", "cast", "status", "stop_all"]


@extern
def osprey_net_local_address(host: str, port: int) -> str:
    ...


@extern
def osprey_file_server_start(path: str, content_type: str, address: str) -> int:
    ...


@extern
def osprey_file_server_stop(port: int) -> int:
    ...


class Casting:
    """A cast in progress: the session, the receiver's transport id, the
    port serving the file, and the last player state reported."""
    session: Session.Session
    transport: str
    port: int
    last: str


class Served:
    """The file's URL and the port of the server behind it."""
    url: str
    port: int


def base_name(path):
    """The last `/`-separated part of a path."""
    parts = unwrap_or(split(path, "/"), [path])
    return unwrap_or(listGet(parts, len(parts) - 1), path)


def leading_dots(s):
    """How many `.` characters a name starts with."""
    return fold(range(0, length(s)), 0, lambda n, i: n + 1 if n == i and take(drop(s, i), 1) == "." else n)


def title(path):
    """The media title: the file's base name without its extension, as
    `os.path.splitext` cuts it (leading dots are not an extension)."""
    name = base_name(path)
    dots = leading_dots(name)
    parts = unwrap_or(split(drop(name, dots), "."), [])
    if len(parts) <= 1:
        return name
    else:
        kept = toList(map(range(0, len(parts) - 1), lambda i: unwrap_or(listGet(parts, i), "")))
        return take(name, dots) + join(kept, ".")


def media_url(path, host) -> Result[Served, str]:
    """Serves `path` on the interface that routes to `host` and returns its URL."""
    address = osprey_net_local_address(host, 9)
    port = osprey_file_server_start(path, "audio/mpeg", address) if address != "" else -1
    if address == "":
        return Error(message=f"{host}: no route to the device")
    elif port < 1:
        return Error(message=f"{path}: cannot serve the file ({port})")
    else:
        return Success(value=Served(url=f"http://{address}:{port}/media", port=port))


def launched(session, received) -> Result[str, str]:
    """One turn of the launch loop over a received message."""
    match received:
        case Success(value=message):
            step = Protocol.launch_step(Protocol.read(message.payload), message.payload)
            if step.kind == "error":
                return Error(message="ERROR " + step.value)
            elif step.kind == "ready":
                return Success(value=step.value)
            else:
                return launch_wait(session)
        case Error(message=failure):
            return Error(message=failure)


def launch_wait(session) -> Result[str, str]:
    return launched(session, Session.receive(session, Session.QUIET_MS))


def launch(session) -> Result[str, str]:
    """Launches the Default Media Receiver and returns its transport id."""
    match Session.send(session, "receiver-0", "receiver", Protocol.launch(1)):
        case Success():
            return launch_wait(session)
        case Error(message=failure):
            return Error(message=failure)


def loaded(session, transport, served, path) -> Result[Casting, str]:
    """Connects to the receiver's transport and LOADs the served file."""
    connected = Session.send(session, transport, "tp.connection", Protocol.connect())
    load = Session.send(session, transport, "media", Protocol.load(served.url, title(path), 2))
    match connected:
        case Error(message=failure):
            return Error(message=failure)
        case Success():
            match load:
                case Success():
                    return Success(value=Casting(session=session, transport=transport, port=served.port, last=""))
                case Error(message=failure):
                    return Error(message=failure)


def started(session, served, path) -> Result[Casting, str]:
    match launch(session):
        case Success(value=transport):
            return loaded(session, transport, served, path)
        case Error(message=failure):
            closed = Session.close(session)
            stopped = osprey_file_server_stop(served.port)
            return Error(message=failure)


def start(device, path) -> Result[Casting, str]:
    """Serves `path`, launches the receiver on `device` and starts playing."""
    match media_url(path, device.host):
        case Error(message=failure):
            return Error(message=failure)
        case Success(value=served):
            match Session.open(device.host, device.port):
                case Success(value=session):
                    return started(session, served, path)
                case Error(message=failure):
                    stopped = osprey_file_server_stop(served.port)
                    return Error(message=failure)


def next(casting) -> Result[Protocol.MediaStep, str]:
    """The next media decision: STATE lines to report and, when `end` is set,
    how playback ended. Carry the step's `last` into the casting with `update`."""
    match Session.receive(casting.session, Session.QUIET_MS):
        case Success(value=message):
            return Success(value=Protocol.media_step(Protocol.read(message.payload), casting.last))
        case Error(message=failure):
            return Error(message=failure)


def finish(casting):
    """Closes the session and stops serving the file."""
    closed = Session.close(casting.session)
    return osprey_file_server_stop(casting.port)


def played(casting, report: Callable[[str], None]) -> Result[str, str]:
    """Runs the media loop, reporting each STATE line, until playback ends."""
    match next(casting):
        case Error(message=failure):
            return Error(message=failure)
        case Success(value=step):
            reported = forEachList(step.lines, report)
            if step.end == "":
                return played(update(casting, last=step.last), report)
            elif step.end == "ERROR":
                return Error(message="ERROR the device refused the media")
            else:
                return Success(value=step.end)


def cast(device, path, report: Callable[[str], None]) -> Result[str, str]:
    """Casts `path` to `device`, reporting each STATE line, and returns how
    playback ended (`DONE FINISHED`, `DONE CANCELLED`, ...)."""
    match start(device, path):
        case Error(message=failure):
            return Error(message=failure)
        case Success(value=casting):
            outcome = played(casting, report)
            finished = finish(casting)
            return outcome


def asked(device, request) -> Result[Protocol.Reading, str]:
    """Opens a session, sends one receiver request and reads the reply."""
    match Session.open(device.host, device.port):
        case Error(message=failure):
            return Error(message=failure)
        case Success(value=session):
            sent = Session.send(session, "receiver-0", "receiver", request)
            reply = Session.receive(session, Session.QUIET_MS)
            closed = Session.close(session)
            match reply:
                case Success(value=message):
                    return Success(value=Protocol.read(message.payload))
                case Error(message=failure):
                    return Error(message=failure)


def status(device) -> Result[str, str]:
    """The device's volume level, as the receiver reports it."""
    match asked(device, Protocol.get_status(1)):
        case Success(value=reading):
            return Success(value=reading.volume)
        case Error(message=failure):
            return Error(message=failure)


def stopped_sessions(device, sessions) -> Result[list[str], str]:
    """STOPs each listed application session over one connection."""
    match Session.open(device.host, device.port):
        case Error(message=failure):
            return Error(message=failure)
        case Success(value=session):
            sent = mapList(sessions, lambda id: Session.send(session, "receiver-0", "receiver", Protocol.stop(id, 2)))
            closed = Session.close(session)
            return Success(value=sessions)


def stop_all(device) -> Result[list[str], str]:
    """Stops every application running on the device; returns the session ids."""
    match asked(device, Protocol.get_status(1)):
        case Success(value=reading):
            return stopped_sessions(device, reading.sessions)
        case Error(message=failure):
            return Error(message=failure)
