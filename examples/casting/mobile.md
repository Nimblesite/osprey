# Native mobile delivery [CAST-MOBILE]

Both native apps build and run in the simulator/emulator. Both platforms pass the full native playback journey against a TLS Cast receiver fixture, including exact HTTP media bytes, seek, volume, speed, stop and replay. Screenshot goldens, CI coverage, broader error/lifecycle cases and physical-device verification remain incomplete.

## Shared boundary [CAST-MOBILE-ABI]

`make -C apps/tests casting-mobile-build TARGET=ios-sim` builds the ARC runtime and the existing casting library. The other targets are `ios`, `android-arm64` and `android-x64`. The output is `build/<target>/libOspreyApp.a`. The native shell must implement `native_slice` and `native_shape`, link the Cast networking services, and drive the resident start/dispatch boundary.

## Delivery checklist [CAST-MOBILE-DELIVERY]

- [x] Inspect existing casting adapter, shared tests and native host implementations.
- [x] Add a repeatable mobile shared-library build entry point.
- [x] Verify shared library builds for physical iPhone, iOS simulator, Android ARM64 and Android x64.
- [x] Drive both native apps through TLS playback, pause/resume, seek, volume, speed, stop and replay; verify metadata, exact served MP3 bytes and one command per slider adjustment.
- [x] Verify native picker cancellation twice and restart on both platforms; verify actual picker import on Android.
- [ ] Implement reusable SwiftUI and Android rendering for the casting machine, including canvas, native controls, accessibility, frame clock and responsive layout.
- [ ] Replace process-based discovery with native Bonjour and Android network service discovery; handle permission denial, retries and disappearing receivers.
- [ ] Implement document selection and copying scoped files into app storage, including cancellation and failed reads.
- [ ] Link native TLS, local HTTP serving and media rendering without shelling out to desktop tools; surface unsupported formats and service failures.
- [ ] Handle foreground/background transitions, connection loss, interrupted playback and cleanup of network/file resources.
- [ ] Add per-platform launch and test Makefile targets and required CI coverage gates.
- [ ] Exercise real simulator/emulator journeys: discovery, file selection, play/pause/resume/stop/replay, seek, volume, speed, skins, offline/error paths and app restart.
- [ ] Commit and compare native screenshot goldens in windowed and headless modes, with accessibility assertions.
- [ ] Verify physical-device casting to a real receiver, including background playback and local network permissions.

The existing host-free casting suite simulates the receiver and drives the real resident application. It does not prove native mobile rendering, permission behavior, ABI linkage or real-device playback.

## Verified native progress [CAST-MOBILE-EVIDENCE]

- iOS: the unsigned physical-device app and simulator app link successfully; the real iPhone simulator UI test passes launch, two document-picker cancellations, fixture-file loading, play/pause/resume, seek, volume, speed, stop, replay and restart. Native sliders and speed-menu buttons are verified through XCTest. The receiver checks the requested control values and rejects duplicate slider commands.
- Android: ARM64 and x64 JNI libraries link into the APK; the ARM64 emulator discovers a real LAN TV and passes two document-picker cancellations, MP3 import, scene-title verification, play/pause/resume, seek, volume, speed, stop, replay and restart. The fixture verifies exact MP3 bytes over HTTP HEAD and range requests. Android lint passes with warnings treated as errors.
- Both TLS implementations pass a live loopback test covering handshake, fragmented reads, timeout recovery, byte preservation, writes, EOF, cleanup and route selection.
- The shared casting suite passes all seven journeys; the Swift host passes 37 regression tests. Native network and media-render runtime suites pass.
- The mobile media runtime uses the existing contract's original-audio fallback because sandboxed apps cannot launch desktop ffmpeg. A native TV visualization renderer is not implemented.

Entry points: `make -C apps/ios casting`, `make -C apps/android casting`, and each platform's `tests` directory with `casting` or `casting-headless`. Android selects a connected device with `OSPREY_ANDROID_SERIAL`; `OSPREY_BIN` selects the compiler binary.
