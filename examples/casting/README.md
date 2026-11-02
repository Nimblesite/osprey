# Casting player

A native player that casts a local MP3 to a Chromecast or Google TV. Its whole interface is data: [player.machine.json](player.machine.json) holds the inputs, state machines, view and skins ([UI-MACHINE](../../docs/specs/platforms/UiMachines.md)), and `ui::Machines` in [@osprey/native](../../packages/native/README.md) plays it on any host. The only code is a small adapter, [src/link.ospml](src/link.ospml). It answers the document's requests (`cast.discover`, `cast.start`, `cast.pause` and so on) with the Python-flavor [@osprey/chromecast](../../packages/chromecast/README.md) port and feeds what the TV reports back in as inputs. The designs are in [designs/](designs/design.md).

## Run

```sh
make -C apps/macos casting          # builds what it needs and opens the player in the GPUI host
make -C apps/windows casting        # Windows: the same player in its sandbox, on the WinUI host
```

On macOS `run.sh` compiles the app to a C archive and links it, with the TLS and media-server runtime and OpenSSL, into the GPUI host, then runs it from `build/Casting.app` so the Dock shows its name and icon. `bundle.sh` also makes that bundle the default MP3 application. On Windows, `hosts/winui/app.ps1` builds the app as a DLL with the same runtime and OpenSSL linked in, and packages it with `windows/cast-browse.c`, a DnsServiceBrowse helper that prints the lines `dns-sd` would. The host runs `dns-sd`, or the helper `--browse` names, to find TVs and opens the file picker, so the window never waits on the network. Each frame polls the TV for at most a millisecond.

## How it fits together

| Piece | Role |
| --- | --- |
| `player.machine.json` | Layers `receivers`, `playback` and `commands`; the Sherbet Signal, Midnight Mint and Poolside Pop skins |
| `src/app.ospml` | Routes framework messages: controls pull the machine's levers, completions and frames go to the adapter |
| `src/link.ospml` | The adapter: requests to `chromecast.Player`, device facts to inputs |
| `main.ospml` | The C boundary; device I/O runs under `chromecast.Wire.Native` |

## Test

```sh
./target/release/osprey examples/casting/test --run
make -C apps/windows test           # Windows: also drives the player in a real window, against the screenshots in goldens/windows
```

The reusable label-driven harness lives in [@osprey/uitest](../../packages/uitest/README.md). The tests drive the framework's real resident `start`/`dispatch` boundary with host-shaped events. They press the controls the rendered view issued, by label. An in-memory Cast device answers the port's `Wire` effect: it parses every frame the sender writes and replies as a Default Media Receiver does. The journeys cover discovery, opening a file, play, pause, resume, seek, volume, speed, stop, replay, the track finishing, skins, no TVs found and an unreachable TV.
