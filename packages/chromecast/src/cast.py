# osprey: flavor=python
# osprey: namespace=chromecast
# osprey: module=Cast
"""Casting from a script: the reference sender's `cast`, `--status` and `--stop`.

`cast` runs a `Player` to the end, handing each `STATE` line to a callback,
and returns how playback ended. A user interface drives `Player` itself.
`status` and `stop_all` ask the receiver once over a short session. Run them
under `Wire.Native` to reach real devices.
"""

from typing import Callable

from chromecast import Player, Protocol, Session

__all__ = ["cast", "status", "stop_all"]


def played(player, now, report) -> Result[str, str]:
    """Polls until playback ends, reporting each STATE line. A quiet second
    counts as a second on the heartbeat clock."""
    match Player.poll(player, now, 1000):
        case Error(message=failure):
            return Error(message=failure)
        case Success(value=next):
            forEachList(next.lines, report)
            if next.phase != "ended":
                return played(next, now + 1000, report)
            elif startsWith(next.ending, "ERROR"):
                return Error(message=next.ending)
            else:
                return Success(value=next.ending)


def cast(device, path, report: Callable[[str], None]) -> Result[str, str]:
    """Casts `path` to `device`, reporting each STATE line, and returns how
    playback ended (`DONE FINISHED`, `DONE CANCELLED`, ...)."""
    match Player.begin(device, path, 0):
        case Error(message=failure):
            return Error(message=failure)
        case Success(value=player):
            outcome = played(player, 0, report)
            _finished = Player.end(player)
            return outcome


def replied(session, request) -> Result[Protocol.Reading, str]:
    """Sends one receiver request on an open session and reads the reply."""
    match Session.write(session, "receiver-0", "receiver", request):
        case Error(message=failure):
            return Error(message=failure)
        case Success():
            match Session.receive(session, Session.QUIET_MS):
                case Success(value=message):
                    return Success(value=Protocol.read(message.payload))
                case Error(message=failure):
                    return Error(message=failure)


def asked(device, request):
    """Opens a session, sends one receiver request and reads the reply."""
    match Session.open(device.host, device.port):
        case Error(message=failure):
            return Error(message=failure)
        case Success(value=session):
            reply = replied(session, request)
            _closed = Session.close(session)
            return reply


def status(device) -> Result[str, str]:
    """The device's volume level, as the receiver reports it."""
    match asked(device, Protocol.get_status(1)):
        case Success(value=reading):
            return Success(value=reading.volume)
        case Error(message=failure):
            return Error(message=failure)


def stopped_one(session, id):
    """Whether the STOP for one application session was written."""
    match Session.write(session, "receiver-0", "receiver", Protocol.stop(id, 2)):
        case Success():
            return True
        case Error():
            return False


def stopped_sessions(device, sessions) -> Result[list[str], str]:
    """STOPs each listed application session over one connection and returns
    the ids whose STOP was written."""
    match Session.open(device.host, device.port):
        case Error(message=failure):
            return Error(message=failure)
        case Success(value=session):
            written = filterList(sessions, lambda id: stopped_one(session, id))
            _closed = Session.close(session)
            return Success(value=written)


def stop_all(device) -> Result[list[str], str]:
    """Stops every application running on the device; returns the session ids."""
    match asked(device, Protocol.get_status(1)):
        case Success(value=reading):
            return stopped_sessions(device, reading.sessions)
        case Error(message=failure):
            return Error(message=failure)
