# @osprey/chromecast

Casts a local audio file to a Chromecast or Google TV. This is a port of a stdlib-only CPython CAST v2 sender (`cast.py`, compared on CPython 3.15.0) to Osprey's Python flavor. It covers discovery, protobuf framing, the TLS session with heartbeats, LAUNCH/LOAD, the media-status loop and playback controls. The reference's pure functions are reproduced in `scripts/oracles/chromecast.py`, which writes `tests/expected.py`.

The package was checked by hand against two real devices (a Chromecast and a Google TV) with `Dns.discover` and `Cast.status`. Their output matched the reference CLI's `--status`. The suite then drives the whole sender against a scripted device: `openssl s_server` with a self-signed certificate, replaying canned device frames. [live/](live/main.py) is the opt-in check that plays real audio: with a TV's name in `.osprey/live-cast-tv`, it casts a six-second tone and checks playing, paused, resumed, seek, volume and `DONE FINISHED`. It passed on both of those devices.

## Using it

```python
from chromecast import Cast, Discovery, Dns, Wire

def main():
    devices = Dns.discover(4)
    match Discovery.named(devices, "Family room TV"):
        case Success(value=tv):
            return Wire.Native(lambda: print(unwrap_or(Cast.cast(tv, "/Users/me/Music/song.mp3", print), "failed")))
        case Error(message=message):
            return print(message)
```

`Cast.cast` runs a `Player` to the end for a script. A UI drives `Player` itself: `begin(device, path, now)` starts without waiting, `poll(player, now, wait_ms)` folds in every available reply, and `elapsed(player, now)` estimates the current position. `pause`, `play`, `seek`, `rate`, `volume` and `stop` send commands; displayed state changes only when the device reports it. Call `end(player)` to close the session and stop serving the file.

`examples/casting` is a native GPUI app built on `Player`. Its UI is a state-machine document, `examples/casting/player.machine.json`.

### Testing with a fake device

All I/O is a `Wire` effect. `Wire.Native(work)` answers it with real TLS, routing and file-serving operations. A test can install its own `Wire` handler and run the same casting code under an in-memory fake Cast device. The fake sees every frame written and chooses every byte read, so tests need no network, TLS server or HTTP server.

## API

| Function | Original | Notes |
| --- | --- | --- |
| `Discovery.Device` | device dictionary | Friendly name, `.local` host and CAST v2 port |
| `Dns.discover(seconds)` | `discover()` | Spawns `dns-sd -Z _googlecast._tcp local` for `seconds`; returns the devices that answer |
| `Discovery.browse(text)` | the parsing half of `discover()` | Pure: SRV gives port and host, TXT's `fn=` gives the name |
| `Discovery.named(devices, name)` | `devs[name]` | `Error` naming the missing device |
| `Protocol.varint(n)` / `Protocol.read_varint(hex, i)` | `varint` / `read_varint` | Hex text instead of `bytes` |
| `Protocol.frame(dst, ns, payload)` | `frame(dst, ns, payload)` | Takes the JSON text; the builders below write it |
| `Protocol.parse(body)` | `parse(body)` | Returns a `Message` with the full namespace and the raw payload text |
| `Protocol.frame_length(header)` | `struct.unpack(">I", head)` | |
| `Protocol.connect/ping/pong/launch/get_status/stop/load` | the dict literals passed to `send` | Byte-identical to `json.dumps` output, including its `\uXXXX` escapes |
| `Protocol.media(kind, media_session, extra, request_id)` | — | Builds PAUSE, PLAY, STOP, SEEK or SET_PLAYBACK_RATE; `extra` adds one JSON member |
| `Protocol.set_volume(level, request_id)` | — | Builds receiver SET_VOLUME for a level from 0 to 1 |
| `Protocol.quote(text)` | `json.dumps(str)` | |
| `Protocol.read(payload)` | `msg.get(...)` lookups | Type, transport and session ids, volume, media states, media session, position, duration, rate and commands |
| `Protocol.launch_step(reading, payload)` | one turn of the launch loop | `error`, `ready` or `wait` |
| `Protocol.media_step(reading, last)` | one turn of the media loop | STATE lines, the last state, `DONE <reason>` or `ERROR` |
| `Session.QUIET_MS` | heartbeat timeout | 10 seconds |
| `Session.open(host, port)` | `Conn(ip, port)` | TLS without peer verification, then CONNECT to `receiver-0` |
| `Session.write(session, dst, ns, payload)` | `Conn.send` | |
| `Session.receive(session, quiet_ms)` | `Conn.recv` | Answers PING, drops PONG, PINGs after `quiet_ms` of silence |
| `Session.poll(session, wait_ms)` | — | Non-blocking read of all available messages; handles heartbeats but never PINGs |
| `Session.close(session)` | — | |
| `Media.Served` | served URL and port | The URL and file-server port returned by `media_url` |
| `Media.media_url(path, host)` / `Media.unserve(port)` | `serve(path, ip)` | Serves only `/media`, with byte ranges, on the interface that routes to the device |
| `Media.title(path)` | `os.path.splitext(os.path.basename(path))[0]` | |
| `Player.BEAT_MS` | heartbeat interval | `poll` PINGs every 5 seconds on the supplied host clock |
| `Player.Player` | casting state | Session, served file, lifecycle, `player_state`, reported position and playback fields, STATE lines, and heartbeat/request bookkeeping |
| `Player.idle()` | — | An ended player with no session or served file |
| `Player.begin(device, path, now)` | the start of `cast()` | Serves, dials and requests LAUNCH without waiting for the answer |
| `Player.poll(player, now, wait_ms)` | one media-loop turn | Non-blocking; advances LAUNCH/LOAD, folds in reports and maintains the heartbeat |
| `Player.elapsed(player, now)` | — | Advances the reported position at the reported rate while PLAYING, capped at duration |
| `Player.pause/play/seek/rate/volume/stop` | — | Sends one command; state changes only after a device report |
| `Player.end(player)` | connection and server cleanup | Closes the session and stops serving; buffered playback can continue unless stopped |
| `Cast.cast(device, path, report)` | `cast(path, ip, port)` | Runs a `Player` to the end; `report` gets each STATE line; returns `DONE <reason>` |
| `Cast.status(device)` | `--status` | The volume level |
| `Cast.stop_all(device)` | `--stop` | The session ids whose STOP was written |
| `Wire.Wire` / `Wire.Native(work)` | C runtime calls | The I/O effect and its native handler: dial/write/read/status/close, route, serve and unserve |
| `Hex.*` | — | The byte layer: Osprey strings cannot hold NUL, so bytes cross the runtime as hex |

## Runtime boundary

`Wire.Native` is the only caller of the two C runtime units. It uses `@extern` for the operations the Python standard library provided. The units are linked automatically with OpenSSL when a program names them.

- `compiler/runtime/tls_client_runtime.c` [FFI-TLS-CLIENT] dials, writes hex and reads an exact byte count as hex with a deadline. A timed-out read keeps its partial bytes. It also finds the local address that routes to a host.
- `compiler/runtime/file_server_runtime.c` [FFI-FILE-SERVER] serves one file at `/media` from background threads, with `Range` support, as `http.server` did.

`compiler/runtime/net_runtime_tests.c` tests both against real loopback sockets.

## Divergences

- **No peer verification.** Cast devices present self-signed certificates, so the session accepts any certificate, as the reference did with `CERT_NONE` [FFI-TLS-UNVERIFIED].
- **Discovery returns host names.** The reference resolved each host to an IP with `gethostbyname`. The port keeps the `.local` name and lets the TLS dial resolve it. `Discovery.browse` consumes a caller-supplied `dns-sd` transcript.
- **Errors are values.** `sys.exit("ERROR ...")` becomes `Error(message="ERROR ...")`. Connection loss is an `Error` naming the reason.
- **Media controls extend the reference CLI.** Media-command payloads follow the CAST v2 media namespace; volume uses receiver SET_VOLUME. `scripts/oracles/chromecast.py` checks their payloads with `json.dumps`.
- **No pickers.** The macOS `osascript` file and device pickers are UI. They belong in the application above this package.
- **audio/mpeg only.** Like the reference, every LOAD and the file server use `audio/mpeg`.
- **`Dns.discover` spawns a process.** It needs `dns-sd`, which ships with macOS. Targets such as the macOS app archive cannot spawn processes, so a UI asks its host to run `dns-sd` and passes the transcript to `Discovery.browse`.
