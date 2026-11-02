# osprey: flavor=python
# osprey: namespace=chromecast
# osprey: module=Dns
"""Running the Cast device browse on this machine.

`discover` runs `dns-sd -Z _googlecast._tcp local` for a few seconds through
the system shell (macOS ships `dns-sd`) and hands what it printed to
`Discovery.browse`. It is its own module because it spawns a process, which
targets such as the macOS application archive do not offer; an application
there asks its host to run the browse instead.
"""

from chromecast import Discovery

__all__ = ["discover"]

SCRATCH = ".osprey/scratch"


def ignore(_pid, _kind, _text):
    return None


def run(command):
    """Runs a shell command to completion; its exit status, -1 if it never started."""
    match spawnProcess(command, ignore):
        case Success(value=pid):
            code = awaitProcess(pid)
            cleanupProcess(pid)
            return code
        case Error():
            return -1


def discover(seconds) -> list[Discovery.Device]:
    """The Cast devices that answer a browse lasting `seconds` seconds."""
    out = f"{SCRATCH}/googlecast-{random()}.txt"
    _browsed = run(f"mkdir -p {SCRATCH} && perl -e 'alarm {seconds}; exec @ARGV' dns-sd -Z _googlecast._tcp local > {out} 2>/dev/null")
    text = unwrap_or(readFile(out), "")
    _removed = run(f"rm -f {out}")
    return Discovery.browse(text)

