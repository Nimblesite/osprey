# @osprey/chromecast

Casts a local audio file to a Chromecast or Google TV. This is a port of a stdlib-only CPython CAST v2 sender (`cast.py`, compared on CPython 3.15.0) to Osprey's Python flavor. It covers discovery, protobuf framing, the TLS session with heartbeats, LAUNCH/LOAD and the media-status loop. The reference's pure functions are reproduced in `scripts/oracles/chromecast.py`, which writes `tests/expected.py`.

The package was checked by hand against two real devices (a Chromecast and a Google TV) with `Discovery.discover` and `Cast.status`. Their output matched the reference CLI's `--status`. The suite then drives the whole sender against a scripted device: `openssl s_server` with a self-signed certificate, replaying canned device frames.

## Using it

```python
from chromecast import Cast, Discovery

def main():
    devices = Discovery.discover(4)
    match Discovery.named(devices, "Family room TV"):
        case Success(value=tv):
            return print(unwrap_or(Cast.cast(tv, "/Users/me/Music/song.mp3", print), "failed"))
        case Error(message=message):
            return print(message)
```

A UI drives the same flow in steps from a fiber: `Cast.start`, then `Cast.next` until a step's `end` is set (carry `step.last` into the casting with `update`), then `Cast.finish`.

## API

| Function | Original | Notes |
| --- | --- | --- |
| `Discovery.discover(seconds)` | `discover()` | Runs `dns-sd -Z _googlecast._tcp local` for `seconds`; returns name, `.local` host and port, never an IP |
| `Discovery.browse(text)` | the parsing half of `discover()` | Pure: SRV gives port and host, TXT's `fn=` gives the name |
| `Discovery.named(devices, name)` | `devs[name]` | `Error` naming the missing device |
| `Protocol.varint(n)` / `Protocol.read_varint(hex, i)` | `varint` / `read_varint` | Hex text instead of `bytes` |
| `Protocol.frame(dst, ns, payload)` | `frame(dst, ns, payload)` | Takes the JSON text; the builders below write it |
| `Protocol.parse(body)` | `parse(body)` | Returns a `Message` with the full namespace and the raw payload text |
| `Protocol.frame_length(header)` | `struct.unpack(">I", head)` | |
| `Protocol.connect/ping/pong/launch/get_status/stop/load` | the dict literals passed to `send` | Byte-identical to `json.dumps` output, including its `\uXXXX` escapes |
| `Protocol.quote(text)` | `json.dumps(str)` | |
| `Protocol.read(payload)` | `msg.get(...)` lookups | The type, receiver transport id, session ids, volume and media states |
| `Protocol.launch_step(reading, payload)` | one turn of `launch()` | `error`, `ready` or `wait` |
| `Protocol.media_step(reading, last)` | one turn of `cast()`'s loop | STATE lines, the last state, `DONE <reason>` or `ERROR` |
| `Session.open(host, port)` | `Conn(ip, port)` | TLS without peer verification, then CONNECT to `receiver-0` |
| `Session.write(session, dst, ns, payload)` | `Conn.send` | |
| `Session.receive(session, quiet_ms)` | `Conn.recv` | Answers PING, drops PONG, PINGs after `quiet_ms` of silence |
| `Session.close(session)` | — | |
| `Cast.media_url(path, host)` / `Cast.unserve(port)` | `serve(path, ip)` | Serves only `/media`, with byte ranges, on the interface that routes to the device |
| `Cast.launch(session)` | `launch(c)` | Returns the transport id |
| `Cast.start` / `Cast.next` / `Cast.finish` | `cast()` split into steps | |
| `Cast.cast(device, path, report)` | `cast(path, ip, port)` | `report` gets each STATE line; returns `DONE <reason>` |
| `Cast.status(device)` | `--status` | The volume level |
| `Cast.stop_all(device)` | `--stop` | The session ids it stopped |
| `Cast.title(path)` | `os.path.splitext(os.path.basename(path))[0]` | |
| `Hex.*` | — | The byte layer: Osprey strings cannot hold NUL, so bytes cross the runtime as hex |

## Runtime boundary

Two C runtime units, called through `@extern`, do what the Python standard library did. They are linked automatically with OpenSSL when a program names them.

- `compiler/runtime/tls_client_runtime.c` [FFI-TLS-CLIENT] dials, writes hex and reads an exact byte count as hex with a deadline. A timed-out read keeps its partial bytes. It also finds the local address that routes to a host.
- `compiler/runtime/file_server_runtime.c` [FFI-FILE-SERVER] serves one file at `/media` from background threads, with `Range` support, as `http.server` did.

`compiler/runtime/net_runtime_tests.c` tests both against real loopback sockets.

## Divergences

- **No peer verification.** Cast devices present self-signed certificates, so the session accepts any certificate, as the reference did with `CERT_NONE` [FFI-TLS-UNVERIFIED].
- **Discovery returns host names.** The reference resolved each host to an IP with `gethostbyname`. The port keeps the `.local` name and lets the TLS dial resolve it.
- **Errors are values.** `sys.exit("ERROR ...")` becomes `Error(message="ERROR ...")`. Connection loss is an `Error` naming the reason.
- **No pickers.** The macOS `osascript` file and device pickers are UI. They belong in the application above this package.
- **audio/mpeg only.** Like the reference, every LOAD and the file server use `audio/mpeg`.
- **`discover` needs `dns-sd`.** It ships with macOS. Elsewhere `discover` returns no devices.
