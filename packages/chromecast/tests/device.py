# osprey: flavor=python
# osprey: namespace=chromecast_tests
# osprey: module=Device
"""The sender end to end against a scripted Cast device.

`openssl s_server` plays the device: it presents a self-signed certificate,
as a real Chromecast does, and writes a canned sequence of device frames
(the oracle's RECEIVED payloads, framed by the port) to whoever connects,
while the session under test writes its CONNECT, LAUNCH, LOAD, PING and
PONG messages back. Each scenario gets its own random port and scratch
directory, so the GC and ARC runs of this suite can overlap.
"""
from chromecast import Cast, Discovery, Hex, Protocol, Session
from chromecast_expected import Expected
from testing import Check

__all__ = ["cases", "outcome"]


class Lab:
    """A scratch directory holding the device's certificate, and a base port."""
    dir: str
    port: int


def outcome(result, show):
    """A Result as text a golden can pin: `ok <value>` or `error <message>`."""
    match result:
        case Success(value=value):
            return "ok " + show(value)
        case Error(message=message):
            return "error " + message


def same(text):
    return text


def ignore(_pid, _kind, _text):
    return None


def sh(command):
    """Runs a shell command to completion and returns its exit status."""
    match spawnProcess(command, ignore):
        case Success(value=pid):
            code = awaitProcess(pid)
            cleanupProcess(pid)
            return code
        case Error():
            return -1


def lab():
    """A fresh lab with a self-signed device certificate."""
    dir = f".osprey/scratch/chromecast-tests-{random()}"
    made = sh(f"mkdir -p {dir} && openssl req -x509 -newkey rsa:2048 -nodes -keyout {dir}/k.pem -out {dir}/c.pem -days 1 -subj /CN=device >/dev/null 2>&1")
    return Lab(dir=dir if made == 0 else "", port=30000 + unwrap_or(randomBelow(20000), 0))


def device_frame(i):
    """Received frame `i` as the device sends it."""
    ns = drop(unwrap_or(listGet(Expected.RECEIVED_NS, i), ""), length(Protocol.NS))
    return Protocol.frame("sender-0", ns, unwrap_or(listGet(Expected.RECEIVED_TEXT, i), ""))


def device(l, k):
    """The scripted device of scenario `k`."""
    return Discovery.Device(name="scripted", host="127.0.0.1", port=l.port + k)


def serve(l, k, frames, delay, accepts):
    """Starts scenario `k`'s device: after `delay` seconds it writes `frames`
    to each of `accepts` connections in turn. Returns its process handle."""
    file = f"{l.dir}/{k}.bin"
    hex = foldList(frames, "", lambda acc, i: acc + device_frame(i))
    _written = sh(f"perl -e 'print pack(\"H*\", $ARGV[0])' '{hex}' > {file}")
    script = f"(sleep {delay}; cat {file}; sleep 3)"
    server = f"openssl s_server -quiet -naccept {accepts} -accept {l.port + k} -cert {l.dir}/c.pem -key {l.dir}/k.pem"
    pid = unwrap_or(spawnProcess(f"{script} | {server} >/dev/null 2>&1", ignore), -1)
    _ready = sh("sleep 1")
    return pid


def done(pid):
    """Waits for a scripted device to finish."""
    code = awaitProcess(pid)
    cleanupProcess(pid)
    return code


def cast_case(l, k, frames, want):
    """Casts a file to scenario `k`'s device and compares how it ended."""
    pid = serve(l, k, frames, 0, 1)
    got = Cast.cast(device(l, k), f"{l.dir}/song.mp3", lambda _line: None)
    _done = done(pid)
    return Check.equal(f"device.cast.{k}", outcome(got, same), want)


def drained(casting, lines):
    """Every STATE line until playback ends, and the ending."""
    match Cast.next(casting):
        case Success(value=step):
            seen = listConcat(lines, step.lines)
            return drained(update(casting, last=step.last), seen) if step.end == "" else Success(value=listAppend(seen, step.end))
        case Error(message=message):
            return Error(message=message)


def steps_case(l, k):
    """start, next and finish by hand, as a UI drives them."""
    pid = serve(l, k, [1, 6, 7, 7, 8], 0, 1)
    match Cast.start(device(l, k), f"{l.dir}/song.mp3"):
        case Success(value=casting):
            got = drained(casting, [])
            _finished = Cast.finish(casting)
            _done = done(pid)
            return [Check.equal("device.steps.transport", casting.transport, "web-7"), Check.equal("device.steps", outcome(got, lambda xs: join(xs, "|")), "ok STATE BUFFERING|STATE PLAYING|STATE IDLE|DONE FINISHED")]
        case Error(message=message):
            _done = done(pid)
            return [Check.failed("device.steps", message)]


def receiver_cases(l):
    """status and stop_all ask the receiver once each."""
    status_pid = serve(l, 3, [1], 0, 1)
    status = Cast.status(device(l, 3))
    _status_done = done(status_pid)
    stop_pid = serve(l, 4, [1], 0, 2)
    stopped = Cast.stop_all(device(l, 4))
    _stop_done = done(stop_pid)
    return [Check.equal("device.status", outcome(status, same), "ok 0.35"), Check.equal("device.stop_all", outcome(stopped, lambda xs: join(xs, ",")), "ok 7E0C-1F2A")]


def quiet_case(l):
    """A quiet device gets a PING; the frame that follows is still delivered."""
    pid = serve(l, 5, [1], 2, 1)
    match Session.open("127.0.0.1", l.port + 5):
        case Success(value=session):
            got = Session.receive(session, 300)
            _closed = Session.close(session)
            _done = done(pid)
            match got:
                case Success(value=message):
                    return Check.equal("device.quiet", message.payload, unwrap_or(listGet(Expected.RECEIVED_TEXT, 1), ""))
                case Error(message=failure):
                    return Check.failed("device.quiet", failure)
        case Error(message=failure):
            _done = done(pid)
            return Check.failed("device.quiet", failure)


def closed_case(l):
    """A device that hangs up is reported, not waited on forever."""
    pid = serve(l, 6, [], 0, 1)
    got = Cast.status(Discovery.Device(name="gone", host="127.0.0.1", port=l.port + 6))
    _done = done(pid)
    return Check.equal("device.closed", outcome(got, same), "error device closed the connection")


def serve_cases(l):
    """media_url serves the file whole over HTTP; unserve stops it."""
    match Cast.media_url(f"{l.dir}/song.mp3", "127.0.0.1"):
        case Success(value=served):
            client = httpCreateClient(f"http://127.0.0.1:{served.port}", 5000)
            body = unwrap_or(body_of(httpGetResponse(client, "/media", "")), "")
            _closed = httpCloseClient(client)
            return [Check.equal("serve.url", served.url, f"http://127.0.0.1:{served.port}/media"), Check.equal("serve.body", body, "ID3 scripted audio"), Check.equal("serve.unserve", Cast.unserve(served.port), 0), Check.equal("serve.unserve.again", Cast.unserve(served.port), -1)]
        case Error(message=message):
            return [Check.failed("serve.url", message)]


def body_of(response):
    """The body of a retained response, which is then freed."""
    match response:
        case Success(value=handle):
            body = httpResponseBody(handle)
            _freed = httpResponseFree(handle)
            return body
        case Error(message=message):
            return Error(message=message)


def failure_cases(l):
    """Every way of not reaching a device names its reason."""
    nowhere = Discovery.Device(name="nowhere", host="no-such-host.invalid", port=8009)
    refused = Discovery.Device(name="refused", host="127.0.0.1", port=l.port + 9)
    return [
        Check.equal("fail.resolve", outcome(Session.open(nowhere.host, nowhere.port), lambda _s: "session"), "error no-such-host.invalid:8009: host does not resolve"),
        Check.equal("fail.refused", outcome(Cast.status(refused), same), f"error 127.0.0.1:{l.port + 9}: connection refused or timed out"),
        Check.equal("fail.stop_all", outcome(Cast.stop_all(refused), lambda xs: join(xs, ",")), f"error 127.0.0.1:{l.port + 9}: connection refused or timed out"),
        Check.equal("fail.start", outcome(Cast.start(refused, f"{l.dir}/song.mp3"), lambda c: c.transport), f"error 127.0.0.1:{l.port + 9}: connection refused or timed out"),
        Check.equal("fail.no_route", outcome(Cast.media_url(f"{l.dir}/song.mp3", "no-such-host.invalid"), lambda m: m.url), "error no-such-host.invalid: no route to the device"),
        Check.equal("fail.no_file", outcome(Cast.media_url(f"{l.dir}/missing.mp3", "127.0.0.1"), lambda m: m.url), f"error {l.dir}/missing.mp3: cannot serve the file (-2)"),
    ]


def cases():
    """Every device scenario, in a fixed order for the golden."""
    l = lab()
    _song = writeFile(f"{l.dir}/song.mp3", "ID3 scripted audio")
    first = [
        Check.truth("device.lab", l.dir != "", "openssl could not make a certificate"),
        cast_case(l, 0, [0, 5, 1, 6, 7, 7, 8], "ok DONE FINISHED"),
        cast_case(l, 1, [4], "error ERROR " + unwrap_or(listGet(Expected.RECEIVED_TEXT, 4), "")),
        cast_case(l, 2, [1, 11], "error ERROR the device refused the media"),
    ]
    rest = listConcat(listConcat(steps_case(l, 7), receiver_cases(l)), [quiet_case(l), closed_case(l)])
    all = listConcat(listConcat(first, rest), listConcat(serve_cases(l), failure_cases(l)))
    _cleaned = sh(f"rm -rf {l.dir}")
    return all
