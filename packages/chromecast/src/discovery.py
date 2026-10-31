# osprey: flavor=python
# osprey: namespace=chromecast
# osprey: module=Discovery
"""Finding Cast devices on the local network.

`browse` reads a `dns-sd -Z _googlecast._tcp local` transcript: each device
has an SRV record (port and host) and a TXT record whose `fn=` entry is its
friendly name. `discover` runs that browse for a few seconds through the
system shell (macOS ships `dns-sd`) and parses what it printed. Host names
end in `.local`, which the system resolver answers over multicast DNS, so a
device's host can be passed straight to `Session.open`.
"""

__all__ = ["Device", "browse", "discover", "named"]

SCRATCH = ".osprey/scratch"


class Device:
    """A Cast device: its friendly name, host name and CAST v2 port."""
    name: str
    host: str
    port: int


def word(ws, i):
    """Word `i` of a split line, "" past its end."""
    return unwrap_or(listGet(ws, i), "")


def record_lines(text, kind):
    """Each line whose second word is `kind`, split into words."""
    return filterList(mapList(lines(text), words), lambda ws: word(ws, 1) == kind)


def friendly_name(line):
    """The `fn=` value of a TXT line, with `\\ ` read as a space."""
    after = unwrap_or(listGet(unwrap_or(split(line, "\"fn="), []), 1), "")
    raw = unwrap_or(listGet(unwrap_or(split(after, "\""), []), 0), "")
    return unwrap_or(replace(raw, "\\ ", " "), raw)


def bare_host(host):
    """A host name without its trailing dot."""
    return take(host, length(host) - 1) if endsWith(host, ".") else host


def device_for(srv, key, name):
    """The device a TXT record names, completed from the SRV record of its key."""
    found = filterList(srv, lambda ws: word(ws, 0) == key)
    ws = unwrap_or(listGet(found, 0), [])
    return Device(name=name, host=bare_host(word(ws, 5)), port=unwrap_or(parseInt(word(ws, 4)), 0))


def has_srv(srv, key):
    return len(filterList(srv, lambda ws: word(ws, 0) == key)) > 0


def txt_lines(text):
    """Every TXT line with its `fn=` entry, unsplit."""
    return filterList(lines(text), lambda line: word(words(line), 1) == "TXT" and contains(line, "\"fn="))


def browse(text) -> list[Device]:
    """Every device in a browse transcript, in the order their TXT records came."""
    srv = record_lines(text, "SRV")
    named_lines = filterList(txt_lines(text), lambda line: has_srv(srv, word(words(line), 0)))
    return mapList(named_lines, lambda line: device_for(srv, word(words(line), 0), friendly_name(line)))


def ignore(_pid, _kind, _text):
    return None


def run(command):
    """Runs a shell command to completion; its exit status, -1 if it never started."""
    match spawnProcess(command, ignore):
        case Success(value=pid):
            code = awaitProcess(pid)
            cleanupProcess(pid)
            return code
        case Error(message=message):
            return -1


def discover(seconds) -> list[Device]:
    """The Cast devices that answer a browse lasting `seconds` seconds."""
    out = f"{SCRATCH}/googlecast-{random()}.txt"
    code = run(f"mkdir -p {SCRATCH} && perl -e 'alarm {seconds}; exec @ARGV' dns-sd -Z _googlecast._tcp local > {out} 2>/dev/null")
    text = unwrap_or(readFile(out), "")
    removed = run(f"rm -f {out}")
    return browse(text)


def named(devices, name) -> Result[Device, str]:
    """The device with this friendly name."""
    match listGet(filterList(devices, lambda d: d.name == name), 0):
        case Success(value=device):
            return Success(value=device)
        case Error(message=message):
            return Error(message=f"no Cast device named {name}")
