# osprey: flavor=python
# osprey: namespace=chromecast
# osprey: module=Player
"""A cast a user interface can drive: every call returns at once.

`begin` serves the file, dials the device and asks it to LAUNCH the Default
Media Receiver; nothing waits for the answer. `poll` reads whatever the
device has said since the last call and folds it into the `Player`: the
receiver's transport (then CONNECT and LOAD are sent), the volume, the media
session, the player state, the position, the duration and the playback
rate. `pause`, `play`, `seek`, `volume`, `rate` and `stop` send one
command each; the player shows a change only once the device reports it, so
a user never sees an unconfirmed state. `end` closes the session and stops
serving. `Cast.cast` is this same loop run to the end for a script.

All I/O is `Wire` effects, so a test plays the device in memory.
"""

from chromecast import Media, Protocol, Session

__all__ = ["BEAT_MS", "Player", "idle", "begin", "poll", "elapsed", "pause", "play", "seek", "volume", "rate", "stop", "end"]

BEAT_MS = 5000
"""How often `poll` sends the device a PING, in milliseconds, as the CAST v2 heartbeat expects."""


class Player:
    """One cast. `phase` is `launching` (waiting for the receiver), `loading`
    (LOAD sent, no media status yet), `playing` (media reported, whatever its
    state) or `ended`; `ending` then says how: `DONE <reason>` or an `ERROR`.
    `player_state` is the last player state the device reported, and `lines` the
    STATE lines the last `poll` saw change. `position` was reported at host
    time `heard_at`; `beat_at` is when the last PING went out. `commands` is
    the device's supported-commands bitmask (1 pause, 2 seek)."""
    session: Session.Session
    port: int
    url: str
    title: str
    phase: str
    ending: str
    transport: str
    media_session: str
    player_state: str
    lines: list[str]
    position: float
    duration: float
    rate: float
    volume: float
    commands: int
    heard_at: int
    beat_at: int
    request: int


def idle() -> Player:
    """A player with nothing cast: ended, on no session, serving nothing.
    A UI holds one until `begin` returns a live player."""
    return Player(session=Session.Session(handle=0), port=0, url="", title="", phase="ended", ending="", transport="", media_session="", player_state="", lines=[], position=0.0, duration=0.0, rate=1.0, volume=-1.0, commands=0, heard_at=0, beat_at=0, request=0)


def number(text, otherwise):
    """A reported number, or `otherwise` when the device did not report it."""
    return otherwise if text == "" else unwrap_or(parseFloat(text), otherwise)


def launched(session, served, path, now) -> Player:
    return Player(session=session, port=served.port, url=served.url, title=Media.title(path), phase="launching", ending="", transport="", media_session="", player_state="", lines=[], position=0.0, duration=0.0, rate=1.0, volume=-1.0, commands=0, heard_at=now, beat_at=now, request=3)


def dialed(device, served, path, now) -> Result[Player, str]:
    """Opens the session and asks for the receiver; stops serving on failure."""
    match Session.open(device.host, device.port):
        case Success(value=session):
            match Session.write(session, "receiver-0", "receiver", Protocol.launch(1)):
                case Success():
                    return Success(value=launched(session, served, path, now))
                case Error(message=failure):
                    _closed = Session.close(session)
                    _stopped = Media.unserve(served.port)
                    return Error(message=failure)
        case Error(message=failure):
            _stopped = Media.unserve(served.port)
            return Error(message=failure)


def begin(device, path, now) -> Result[Player, str]:
    """Serves `path` and asks `device` to launch its media receiver. `now` is
    the host clock in milliseconds; `poll` does the rest."""
    match Media.media_url(path, device.host):
        case Success(value=served):
            return dialed(device, served, path, now)
        case Error(message=failure):
            return Error(message=failure)


def loading(player, transport) -> Result[Player, str]:
    """CONNECTs to the receiver's transport and LOADs the served file."""
    match Session.write(player.session, transport, "tp.connection", Protocol.connect()):
        case Success():
            match Session.write(player.session, transport, "media", Protocol.load(player.url, player.title, 2)):
                case Success():
                    return Success(value=update(player, phase="loading", transport=transport))
                case Error(message=failure):
                    return Error(message=failure)
        case Error(message=failure):
            return Error(message=failure)


def ended(player, ending):
    return update(player, phase="ended", ending=ending)


def receiver(player, reading, payload):
    """A receiver message while launching: the transport, or a launch error."""
    step = Protocol.launch_step(reading, payload)
    if step.kind == "error":
        return Success(value=ended(player, "ERROR " + step.value))
    elif step.kind == "ready":
        return loading(player, step.value)
    else:
        return Success(value=player)


def reported(player, reading, now) -> Player:
    """A media status folded in: state, lines, position and how it ended."""
    step = Protocol.media_step(reading, player.player_state)
    heard = update(player, phase="playing", player_state=step.last, lines=listConcat(player.lines, step.lines), heard_at=now)
    facts = update(heard, media_session=reading.media_session if reading.media_session != "" else player.media_session, position=number(reading.position, player.position), duration=number(reading.duration, player.duration), rate=number(reading.rate, player.rate), commands=unwrap_or(floatToInt(number(reading.commands, toFloat(player.commands))), player.commands))
    if step.end == "ERROR":
        return ended(facts, "ERROR the device refused the media")
    elif step.end != "":
        return ended(facts, step.end)
    else:
        return facts


def heard(player, message, now):
    """One message from the device folded into the player."""
    reading = Protocol.read(message.payload)
    tuned = update(player, volume=number(reading.volume, player.volume))
    if tuned.phase == "ended":
        return Success(value=tuned)
    elif tuned.phase == "launching":
        return receiver(tuned, reading, message.payload)
    else:
        return Success(value=reported(tuned, reading, now)) if reading.kind != "RECEIVER_STATUS" else Success(value=tuned)


def folded(player, messages, i, now) -> Result[Player, str]:
    match listGet(messages, i):
        case Success(value=message):
            match heard(player, message, now):
                case Success(value=next):
                    return folded(next, messages, i + 1, now)
                case Error(message=failure):
                    return Error(message=failure)
        case Error():
            return Success(value=player)


def beat(player, now) -> Result[Player, str]:
    """A PING when `BEAT_MS` have passed since the last one."""
    if now - player.beat_at < BEAT_MS:
        return Success(value=player)
    else:
        match Session.write(player.session, "receiver-0", "tp.heartbeat", Protocol.ping()):
            case Success():
                return Success(value=update(player, beat_at=now))
            case Error(message=failure):
                return Error(message=failure)


def poll(player, now, wait_ms) -> Result[Player, str]:
    """The player after everything the device said within `wait_ms` (at least
    1); `lines` holds only this poll's STATE lines. An `Error` means the
    connection is lost: the device may still be playing."""
    match Session.poll(player.session, wait_ms):
        case Success(value=messages):
            match folded(update(player, lines=[]), messages, 0, now):
                case Success(value=next):
                    return beat(next, now)
                case Error(message=failure):
                    return Error(message=failure)
        case Error(message=failure):
            return Error(message=failure)


def elapsed(player, now):
    """The position now, in seconds: the last report, advanced at the
    reported rate while playing and held at the duration."""
    since = toFloat(now - player.heard_at) / 1000.0 * player.rate if player.player_state == "PLAYING" else 0.0
    moved = player.position + since
    return moved if player.duration <= 0.0 or moved < player.duration else player.duration


def commanded(player, payload) -> Result[Player, str]:
    """Sends one media command for the loaded media."""
    if player.media_session == "":
        return Error(message="nothing is playing yet")
    else:
        match Session.write(player.session, player.transport, "media", payload):
            case Success():
                return Success(value=update(player, request=player.request + 1))
            case Error(message=failure):
                return Error(message=failure)


def pause(player) -> Result[Player, str]:
    """Asks the device to pause."""
    return commanded(player, Protocol.media("PAUSE", player.media_session, "", player.request))


def play(player) -> Result[Player, str]:
    """Asks the device to play from where it paused."""
    return commanded(player, Protocol.media("PLAY", player.media_session, "", player.request))


def seek(player, seconds) -> Result[Player, str]:
    """Asks the device to play from `seconds`."""
    return commanded(player, Protocol.media("SEEK", player.media_session, f"\"currentTime\": {seconds}", player.request))


def rate(player, speed) -> Result[Player, str]:
    """Asks the device to play at `speed` (1.0 is normal); the player's
    `rate` changes only if the device reports it."""
    return commanded(player, Protocol.media("SET_PLAYBACK_RATE", player.media_session, f"\"playbackRate\": {speed}", player.request))


def stop(player) -> Result[Player, str]:
    """Asks the device to stop; it then reports IDLE, which ends the cast."""
    return commanded(player, Protocol.media("STOP", player.media_session, "", player.request))


def volume(player, level) -> Result[Player, str]:
    """Asks the receiver to set its volume, from 0 to 1."""
    match Session.write(player.session, "receiver-0", "receiver", Protocol.set_volume(level, player.request)):
        case Success():
            return Success(value=update(player, request=player.request + 1))
        case Error(message=failure):
            return Error(message=failure)


def end(player):
    """Closes the session and stops serving the file; the device keeps
    playing whatever it has buffered unless it was stopped first."""
    _closed = Session.close(player.session)
    return Media.unserve(player.port)
