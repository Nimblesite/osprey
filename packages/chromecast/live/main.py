# osprey: flavor=python
"""Plays real audio on a real TV: the opt-in end-to-end check of @osprey/chromecast.

It runs only when `.osprey/live-cast-tv` names a Cast device on this network;
otherwise it prints why it skipped. It makes a six-second 440 Hz tone with
ffmpeg, casts it through `Wire.Native`, and checks every step against what
the TV reports: playing, paused, playing again, a seek, the volume round
trip (set to the level the TV already had) and the natural end, DONE
FINISHED. Run it from the repository root:

    echo "Family room TV" > .osprey/live-cast-tv
    ./target/release/osprey packages/chromecast/live --run
"""
from chromecast import Discovery, Dns, Player, Wire
from testing import Check

TONE = ".osprey/scratch/live-tone.mp3"
"""Where the tone is written."""


def ignore(_pid, _kind, _text):
    return None


def tone():
    """Writes the six-second tone; ffmpeg's exit status."""
    match spawnProcess(f"mkdir -p .osprey/scratch && ffmpeg -loglevel error -y -f lavfi -i sine=frequency=440:duration=6 -q:a 9 {TONE}", ignore):
        case Success(value=pid):
            code = awaitProcess(pid)
            cleanupProcess(pid)
            return code
        case Error():
            return -1


def until(player, now, want, budget):
    """Polls until the TV reports `want` (or the cast ends), for at most `budget` polls of 200 ms."""
    match Player.poll(player, now, 200):
        case Success(value=next):
            if next.player_state == want or next.phase == "ended" or budget <= 0:
                # heard_at carries this check's clock to the next step.
                return Success(value=update(next, heard_at=now))
            else:
                return until(next, now + 200, want, budget - 1)
        case Error(message=message):
            return Error(message=message)


def state_of(result):
    match result:
        case Success(value=p):
            return p.player_state + (" " + p.ending if p.ending != "" else "")
        case Error(message=message):
            return "error " + message


def stepped(result, act):
    """`act` on the player inside `result`, then the TV's answer; failures pass through."""
    match result:
        case Success(value=p):
            match act(p):
                case Success(value=sent):
                    return until(sent, p.heard_at + 200, "", 15)
                case Error(message=message):
                    return Error(message=message)
        case Error(message=message):
            return Error(message=message)


def waited(result, want):
    match result:
        case Success(value=p):
            return until(p, p.heard_at + 200, want, 150)
        case Error(message=message):
            return Error(message=message)


def journey(tv):
    """Casts the tone to `tv` and checks each state the TV reports."""
    match Player.begin(tv, TONE, 0):
        case Error(message=message):
            return [Check.failed("live.begin", message)]
        case Success(value=player):
            playing = until(player, 200, "PLAYING", 100)
            paused = waited(stepped(playing, lambda p: Player.pause(p)), "PAUSED")
            resumed = waited(stepped(paused, lambda p: Player.play(p)), "PLAYING")
            sought = stepped(resumed, lambda p: Player.seek(p, 1.0))
            turned = stepped(sought, lambda p: Player.volume(p, p.volume))
            ended = waited(turned, "never")
            match ended:
                case Success(value=last):
                    _ended = Player.end(last)
                case Error():
                    _ended = Player.end(player)
            return [
                Check.equal("live.playing", state_of(playing), "PLAYING"),
                Check.equal("live.paused", state_of(paused), "PAUSED"),
                Check.equal("live.resumed", state_of(resumed), "PLAYING"),
                Check.truth("live.sought", state_of(sought) != "", "no answer to SEEK"),
                Check.truth("live.volume", state_of(turned) != "", "no answer to SET_VOLUME"),
                Check.equal("live.ended", state_of(ended), "IDLE DONE FINISHED"),
            ]


def live(name):
    """Every check against the TV called `name`."""
    made = Check.equal("live.tone", tone(), 0)
    match Discovery.named(Dns.discover(4), name):
        case Success(value=tv):
            return [made] + journey(tv)
        case Error(message=message):
            return [made, Check.failed("live.discover", message)]


def main():
    name = trim(unwrap_or(readFile(".osprey/live-cast-tv"), ""))
    if name == "":
        return print("skipped: write a TV's name to .osprey/live-cast-tv to cast to it")
    else:
        match Check.strictly(lambda: Check.report(Wire.Native(lambda: live(name)))):
            case Success(value=value):
                return value
            case Error(message=message):
                return print(f"live check stopped: {message}")
