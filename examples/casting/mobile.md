# Native mobile delivery [CAST-MOBILE]

Both native apps build and run in the simulator/emulator. Both platforms' Sherbet player passes the full native playback journey against a TLS Cast receiver fixture, including exact HTTP media bytes, seek, volume, speed, stop and replay. Native screenshot goldens now gate the ready and restarted player. CI coverage, broader error/lifecycle cases, the visible iOS Simulator run and physical-device verification remain incomplete.

## Shared boundary [CAST-MOBILE-ABI]

`make -C apps/tests casting-mobile-build TARGET=ios-sim` builds the ARC runtime and the existing casting library. The other targets are `ios`, `android-arm64` and `android-x64`. The output is `build/<target>/libOspreyApp.a`. The native shell must implement `native_slice` and `native_shape`, link the Cast networking services, and drive the resident start/dispatch boundary.

## Delivery checklist [CAST-MOBILE-DELIVERY]

- [x] Inspect existing casting adapter, shared tests and native host implementations.
- [x] Add a repeatable mobile shared-library build entry point.
- [x] Verify shared library builds for physical iPhone, iOS simulator, Android ARM64 and Android x64.
- [x] Drive both native apps through TLS playback, pause/resume, seek, volume, speed, stop and replay; verify metadata, exact served MP3 bytes and one command per slider adjustment.
- [x] Verify native picker cancellation twice and restart on both platforms; verify actual picker import and playback from the imported copy on both platforms.
- [x] Replace Android virtual accessibility controls with real native buttons and seek bars; verify Tab focus and Enter activation against the actual file picker, plus a native keyboard volume increment reported by the TLS receiver.
- [ ] Implement reusable SwiftUI and Android rendering for the casting machine, including canvas, native controls, accessibility, frame clock and responsive layout.
- [ ] Replace process-based discovery with native Bonjour and Android network service discovery; handle permission denial, retries and disappearing receivers.
- [ ] Implement document selection and copying scoped files into app storage, including cancellation and failed reads.
- [ ] Link native TLS, local HTTP serving and media rendering without shelling out to desktop tools; surface unsupported formats and service failures.
- [ ] Handle foreground/background transitions, connection loss, interrupted playback and cleanup of network/file resources.
- [ ] Add per-platform launch and test Makefile targets and required CI coverage gates.
- [x] Rotate Android during playback, use landscape pause/resume and volume, preserve the connection, return to portrait and restore the previous system rotation policy.
- [ ] Complete the alternate skins: Android needs the generic themed UI tree; iOS needs the Midnight background/foreground palette verified and corrected.
- [ ] Exercise real simulator/emulator journeys: discovery, file selection, play/pause/resume/stop/replay, seek, volume, speed, skins, offline/error paths and app restart.
- [x] Commit native ready/restart screenshot goldens and verify headless iOS plus visible and headless Android rendering with accessibility assertions.
- [ ] Verify the same iOS goldens with the Simulator window visible.
- [ ] Verify physical-device casting to a real receiver, including background playback and local network permissions.

The existing host-free casting suite simulates the receiver and drives the real resident application. It does not prove native mobile rendering, permission behavior, ABI linkage or real-device playback.

## Verified native progress [CAST-MOBILE-EVIDENCE]

- iOS: the unsigned physical-device app and simulator app link successfully; the real iPhone simulator UI test passes launch, two document-picker cancellations, actual Files selection and import, fixture-file loading, play/pause/resume, seek, volume, speed, stop, replay and restart. Native sliders and speed-menu buttons are verified through XCTest. The receiver checks the requested control values and rejects duplicate slider commands.
- Android: real `Button` and `SeekBar` widgets provide touch, keyboard navigation, native accessibility and focus indicators beneath the shared skin artwork. Programmatic native range updates are paced every two seconds while the canvas keeps animating; focusing or dragging a slider synchronizes it immediately. The keyboard journey opens and cancels the native file picker. ARM64 and x64 JNI libraries link into the APK; the ARM64 emulator discovers a real LAN TV and passes two document-picker cancellations, MP3 import, scene-title verification, play/pause/resume, seek, volume, speed, stop, replay and restart. The fixture verifies exact MP3 bytes over HTTP HEAD and range requests. Android lint passes with warnings treated as errors.
- Both TLS implementations pass a live loopback test covering handshake, fragmented reads, timeout recovery, byte preservation, writes, EOF, cleanup and route selection.
- The shared casting suite passes all seven journeys; the Swift host passes 37 regression tests. Native network and media-render runtime suites pass.
- The mobile media runtime uses the existing contract's original-audio fallback because sandboxed apps cannot launch desktop ffmpeg. A native TV visualization renderer is not implemented.

Entry points: `make -C apps/ios casting`, `make -C apps/android casting`, and each platform's `tests` directory with `casting` or `casting-headless`. Android selects a connected device with `OSPREY_ANDROID_SERIAL`; `OSPREY_BIN` selects the compiler binary.

## Native screenshot checks [CAST-MOBILE-GOLDENS]

The platform test targets compare both ready and restarted captures with one reviewed reference per platform and pixel width. They crop only the neutral native host and OS bars around the Sherbet player; every player control remains in the comparison. The gate permits at most 0.5% changed pixels overall and 2% in any 64-pixel tile, with a channel difference above 24 counted as changed. The local check has a regression proving that a small missing control fails even when the overall percentage would pass. Native playback and accessibility assertions run before the image checks.

The checked references use an iPhone 17 simulator on iOS 27 at 1206 pixels wide and a Pixel 7 emulator on API 34 at 1080 pixels wide. Start the Android emulator with `-gpu swiftshader_indirect` in both visible and headless modes so the renderer is consistent. The host GPU backend produced an entirely black framebuffer while the control journey passed; the image gate correctly rejected that run. With SwiftShader, both Android modes match the reference exactly. iOS headless captures also match exactly; this build host lacks the selected Xcode's `Simulator.app`, so its visible Simulator run remains unverified.

Use `make -C apps/ios/tests casting-headless` or `make -C apps/android/tests casting-headless` for comparison. Each platform also has `casting-update`, which runs the native journey, rewrites only the ready reference, and checks that restarting still matches it. Review the images under `examples/casting/goldens/<platform>` before committing an update. A missing reference or changed dimensions fails; tests never silently create a baseline. The runner uses the ffmpeg already required for its audio fixture, without an extra image-library dependency.

## Alternate skin gap [CAST-MOBILE-SKINS]

The native playback and golden results above cover Sherbet. Capturing an iOS launch with `--skin midnight` proves the alternate skin exports a `themed` tree containing stacks, an inline scene, empty state, progress, buttons and pickers. Android currently accepts only a top-level scene and must support that tree before the other skins can work. The captured iOS Midnight screen renders its controls but retains a white background and dark text despite the exported navy background and light text tokens; its palette is not complete. These gaps remain part of delivery, alongside native access to skin selection and full playback checks for every skin.
