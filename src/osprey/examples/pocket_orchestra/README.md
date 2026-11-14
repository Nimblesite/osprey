# Light Orchestra

Your phones control a synthesizer playing on your Mac. The desktop app shows an invite QR code, connected-controller and playing-note counts, an output meter, master volume and **Silence all notes**. Each phone chooses Glass, Sub bass, Pulse lead or Orbit FM, then plays a multitouch ribbon. Move right for pitch and choose what the vertical axis controls — loudness, tone, echo, reverb or metallic warp. Each phone retains its own sound and effect settings.

## Play

On an Apple silicon Mac with the repository's compiler dependencies and Xcode command-line tools:

```sh
make -C src/toolchain/targets/macos pocket-orchestra
```

This builds and opens `target/orchestra/Light Orchestra.app`. Put the phones and Mac on the same Wi-Fi, scan the QR code, tap **Join session**, then touch the ribbon. The app plays through the Mac's current system audio output. Start with a comfortable speaker volume. The compiled app bundles its controller page and can subsequently be opened directly.

The invite uses the first available `en` network interface. If several adapters are active, choose the reachable address when running the binary:

```sh
ORCHESTRA_HOST=192.168.1.110 target/orchestra/pocket-orchestra
```

Use your Mac's actual address. HTTP uses port 9090 and WebSocket uses 9091; `ORCHESTRA_PORT` changes the HTTP port and the next port carries WebSocket. Allow the app's local-network/firewall access if macOS asks. Guest Wi-Fi isolation or a VPN can prevent phones from reaching the Mac.

On a computer, focus the ribbon and hold Space to play; arrow keys change pitch and the selected expression. The expression slider works alongside the ribbon, and switching expression retains the other amounts. Changing instrument releases held notes before applying the new sound. **Release my notes** releases that controller's notes. Closing, hiding or disconnecting a controller releases its notes; missing heartbeats expire its session after 1.5 seconds. The desktop's **Silence all notes** releases the whole ensemble and tells connected controllers to clear their held touches.

## What runs where [ORCHESTRA-PLAN-ENSEMBLE]

The native Osprey kernel owns admission, sequences, touch ownership, heartbeat expiry and pentatonic pitch mapping. Its timbres and metallic ring-modulation kernel come from the UI-free `synth::Performance` module in `@osprey/synth`. The host supplies eight independent controller effect racks, each chaining a 310 ms feedback echo and a four-comb/two-all-pass reverb. Tone changes harmonic balance or FM depth; warp introduces metallic sidebands. Parameter smoothing reduces abrupt edits. These performance timbres and reverb are a small playable synthesis example; oversampling and measured alias rejection remain outstanding. The Swift adapter hosts HTTP/WebSocket, renders bounded audio blocks and feeds AVAudioEngine through a preallocated C single-producer/single-consumer ring. Osprey calls stay on one OS thread; the audio callback reads samples without calling Osprey or allocating buffers. Rendering and session handling share a dedicated producer thread, isolated from AppKit window work. Network callbacks transfer bounded per-connection work to that thread. Hard-real-time scheduling remains unverified.

The browser has semantic HTML buttons and a range control beside its canvas ribbon. It sends authenticated gestures over WebSocket; it does not generate the sound. The desktop has native AppKit controls. Eight controllers can join, with up to ten protocol voices each; the browser reserves one of its ten voice IDs for keyboard playing. Gain smoothing and ensemble normalization keep generated samples bounded. When all eighty voice slots are occupied, a newly admitted voice can replace a quiet release tail.

Output configuration changes trigger an AVAudioEngine restart while retaining the session and held notes. A once-per-second health check retries temporary output failures.

This delivery is macOS plus browser controllers. Native iOS/Android controllers, Windows hosting, the shared Osprey Native presentation, WebAssembly controller logic, selectable output devices, the performance stage remain planned. VST3 hosting remains a future adapter; no plug-in loading is implemented here. The full cross-platform acceptance matrix is in [the application specification](../../../../docs/specs/applications/PocketOrchestra.md).

## Trust boundary [ORCHESTRA-NETWORK-TRUST]

Use a trusted local network: this version serves HTTP and unencrypted WebSocket. The random session token travels in the invite fragment and the initial WebSocket message; anyone with the invite can join while capacity permits. Tokens are kept in the browser tab's session storage for refresh/rejoin. Origin and subprotocol checks, bounded messages, admission limits and heartbeat expiry are enforced, but this is not suitable for public Internet exposure. Do not port-forward these listeners.

## Verify

```sh
# Osprey session journeys, independent of device hardware.
make -C src/toolchain/targets/tests pocket-orchestra

# Native server + mobile browser + captured audio, without an audio device or visible windows.
make -C src/toolchain/targets/macos/tests pocket-orchestra-headless

# The same journey with the native window, visible browsers and the Mac's real audio output.
make -C src/toolchain/targets/macos/tests pocket-orchestra
```

The native integration tests require the website's installed Playwright/`ws` dependencies, Chromium and WebKit. They cover two-player ownership, eighty concurrent notes, stale and malformed gestures, denied origins/tokens/admission, heartbeat expiry, Chromium mobile multitouch, WebKit touch, keyboard playing in both engines, refresh/rejoin, finite bounded PCM and silence after release. Artifacts are `target/orchestra/phone-playing.png` and `target/orchestra/controller-audio.f32` (native little-endian float32, mono; the start diagnostic reports sample rate). A real phone on Wi-Fi is still required to validate physical-device latency and network permissions.

For bounded diagnostics, `ORCHESTRA_DURATION=10` stops after ten seconds, `ORCHESTRA_NO_WINDOW=1` uses the audio device without showing a window, and `ORCHESTRA_HEADLESS=1` renders without a device. `ORCHESTRA_CAPTURE=<path>` saves at most thirty seconds of generated PCM before master volume; exceeding that diagnostic limit reports an error. Startup/shutdown diagnostics are JSON lines on stdout and omit invitation tokens. For a session without a window, supply `ORCHESTRA_TOKEN` (at least twenty characters) and open `http://<host>:<port>/#<token>` on controllers.

`audio.td` under the Swift host is the source of its audio models. Regenerate with `make -C src/toolchain/targets/macos _pocket-orchestra-models TYPEDIAGRAM_DIST=/path/to/typeDiagram/packages/typediagram/dist`; generated output is checked against that source during validation.

### Recorded verification

On 2026-11-14, macOS 26.7.1: the launch target built and opened the signed local app; GC and ARC session journeys passed; headless and real-audio integration journeys passed with Chromium and WebKit; captured PCM verified the 110 Hz root within 1 Hz, finite bounded output and silence after release. The visible Chromium/WebKit journey and the headless journey both enforce zero audio underruns. A loaded repeated run exposed underruns with main-thread production. Moving Osprey and audio production onto a dedicated OS thread passed four visible integration runs and a ninety-second device session with zero underruns, retaining a 1,024-frame queue, approximately 23 ms at 44.1 kHz. Native controls were exercised with a browser-held note before the producer-thread change, including conductor silence and subsequent playing without reconnect. Rechecking those controls after the thread change was blocked by the locked Mac screen; the browser and audio-device journeys ran successfully. Occupied-port failure and token-free diagnostics are checked. The C ring passed two million ordered samples under address/undefined-behaviour sanitizers and independently under ThreadSanitizer; Clang static analysis reported no issues. Model regeneration matched the checked-in Swift binding. Deslop found no duplicates in the C adapter files it supports; its scan did not cover Swift. Both modes and the GC/ARC session journeys are wired into the required Apple CI matrix. The website suite passed all 383 checks and the web compiler container built and passed its API journey. Local `make ci` passed lint and all 2,953 Rust tests, then stopped at the existing Rust coverage thresholds across ten crates. Its later suites therefore remain unverified. Physical-phone testing remains outstanding.

## Diagnostics [ORCHESTRA-DIAGNOSTICS]

Normal app launches persist structured controller, transport, session and audio-device events in `~/Library/Logs/Osprey/LightOrchestra/session-<PID>.jsonl`, alongside macOS unified logging. Each process retains a current log and one previous 2 MiB segment. Invitation tokens are redacted. `ORCHESTRA_LOG_DIRECTORY` selects a different directory. Browser connection, input, visibility and error events travel through an authenticated HTTP endpoint even when the WebSocket has disconnected. The health event records controller/note counts, rendered frames, queue depth, device state and underruns once per second. Logging failures are reported through unified logging and stderr.

The app bundle uses the existing Osprey bird logo for Finder and the Dock. Start everything with the single public `pocket-orchestra` target above.
