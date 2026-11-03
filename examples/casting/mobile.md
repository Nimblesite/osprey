# Native mobile delivery [CAST-MOBILE]

The iOS and Android casting apps are incomplete. Shared-library compilation is an initial check, not evidence that either app runs.

## Shared boundary [CAST-MOBILE-ABI]

`make -C apps/tests casting-mobile-build TARGET=ios-sim` builds the ARC runtime and the existing casting library. The other targets are `ios`, `android-arm64` and `android-x64`. The output is `build/<target>/libOspreyApp.a`. The native shell must implement `native_slice` and `native_shape`, link the Cast networking services, and drive the resident start/dispatch boundary.

## Delivery checklist [CAST-MOBILE-DELIVERY]

- [x] Inspect existing casting adapter, shared tests and native host implementations.
- [x] Add a repeatable mobile shared-library build entry point.
- [ ] Verify shared library builds for physical iPhone, iOS simulator, Android ARM64 and Android x64.
- [ ] Implement reusable SwiftUI and Compose rendering for the casting machine, including canvas, native controls, accessibility, frame clock and responsive layout.
- [ ] Replace process-based discovery with native Bonjour and Android network service discovery; handle permission denial, retries and disappearing receivers.
- [ ] Implement document selection and copying scoped files into app storage, including cancellation and failed reads.
- [ ] Link native TLS, local HTTP serving and media rendering without shelling out to desktop tools; surface unsupported formats and service failures.
- [ ] Handle foreground/background transitions, connection loss, interrupted playback and cleanup of network/file resources.
- [ ] Add per-platform launch and test Makefile targets and required CI coverage gates.
- [ ] Exercise real simulator/emulator journeys: discovery, file selection, play/pause/resume/stop/replay, seek, volume, speed, skins, offline/error paths and app restart.
- [ ] Commit and compare native screenshot goldens in windowed and headless modes, with accessibility assertions.
- [ ] Verify physical-device casting to a real receiver, including background playback and local network permissions.

The existing host-free casting suite simulates the receiver and drives the real resident application. It does not prove native mobile rendering, permission behavior, ABI linkage or real-device playback.
