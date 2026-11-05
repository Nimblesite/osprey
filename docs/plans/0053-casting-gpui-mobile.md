# Casting on iOS and Android through GPUI [CAST-MOBILE-GPUI]

The request, verbatim: "the fucking ios and android apps must use the same GPUI code as the fucking desktop version."

**Status: unfinished.** Casting's iOS and Android apps draw through the shared GPUI host and their old renderers are gone, but neither is a finished app. They are not part of CI, and `scripts/ci/ci-scope.test.mjs` fails if a workflow, `scripts/mobile/test-mobile-tools.py` or the host's default tests reach them. Their tool and bridge regressions run by hand with `make -C src/toolchain/targets/tests casting-mobile-tools`. [Remaining](#remaining) lists what is left. CI covers only desktop casting.

Before this migration the three apps shared `player.machine.json` and the compiled Osprey core, but each drew it differently: macOS through `src/toolchain/hosts/gpui`, iOS through SwiftUI (`src/toolchain/hosts/swift`), Android through its own Java views (`src/osprey/examples/casting/android`). Their widgets and layouts differed, so the same skin looked different.

iOS: GPUI supports it. `src/toolchain/hosts/gpui/Cargo.lock` already holds `gpui-pre-ios` 0.3.10, a snapshot of Zed's `gpui_ios` crate over UIKit, CoreText and Metal, with lifecycle, windowing, native text input, safe-area and keyboard insets and touch, and `gpui-pre-platform` selects it on `target_os = "ios"`. The spike below built this backend and verified the shared player in the simulator.

Android: `gpui-pre` 0.3.10 has no backend, and `gpui-pre-platform` has no Android branch, though `gpui-pre-wgpu` already handles Android surface loss. The implemented backend lives in `src/toolchain/hosts/gpui/android`, over `gpui-pre-wgpu`; the spike below verified its emulator rendering.

## Spike

- [x] Build `src/toolchain/hosts/gpui` for `aarch64-apple-ios-sim` and draw the casting screen in the simulator through `gpui_ios`.
- [x] Read `gpui-mobile`'s Android backend as data, never built or run until reviewed, and choose between it and a backend over `gpui-pre-wgpu`.
- [x] Build `src/toolchain/hosts/gpui` for `aarch64-linux-android` and draw the casting screen on the emulator.
- [x] Report what those runs show, with the failure output for anything that does not work.

## Port

- [x] One host: `src/toolchain/hosts/gpui` draws casting on macOS, iOS and Android from the same view code, with touch input and safe areas.
- [x] Keep what only the platform can do behind the capability boundary: Bonjour and NSD discovery, document pickers, TLS, media serving, lifecycle, accessibility.
- [x] Android: the journey passes on the GPUI app against the TLS receiver fixture, with its golden.
- [ ] iOS: its full native journey passes on the GPUI app. The unchanged canonical canvas/native-picker and real Bonjour discovery/rescan XTests both pass through the shared GPUI host, including receiver label, viewport bounds and selected accessibility state. A separate real Bonjour/TLS playback smoke passes. The full media, gesture, lifecycle, appearance and coverage gates remain unverified.
- [ ] Compare each skin's mobile pictures with the desktop's widgets at the same size: `src/osprey/examples/casting/skin_parity.py` scored Android against desktop at Sherbet 0.976 and Midnight 0.901; Poolside has not finished a run, and the script needs more than five minutes and a real window, so it is not in CI.
- [x] Delete the SwiftUI and Java casting renderers. `src/toolchain/hosts/swift` stays for Osprey Agent.
- [x] A fixed scene `zoom` is an upper bound: a narrower region fits the whole scene instead of clipping it, on every platform. The region around a scene takes the skin's colour, and the scene honours the safe area.
- [ ] Windows: casting still runs on `src/toolchain/hosts/winui` (`src/osprey/examples/casting/README.md`). Audit whether `src/toolchain/hosts/gpui` can replace it there, and report.
- [x] Android screen readers: the scene's AccessKit tree reaches TalkBack, and the journey presses through its action.
- [ ] iOS screen readers: the same tree through `accesskit_ios`.
- [ ] Automate real NSD and Bonjour discovery against a published fixture. Android found a real TV by hand; its journey uses the receiver fixture so that it can never reach a real TV.

## Remaining

The item-by-item delivery checklist is in [src/osprey/examples/casting/mobile.md](../../src/osprey/examples/casting/mobile.md) [CAST-MOBILE-DELIVERY]; its boxes tick only on a passing run against the GPUI app. In order:

iOS:

- [ ] Run `src/osprey/examples/casting/ios/ui-test.sh` in full against the GPUI app and fix what it finds: media, gestures, lifecycle, appearance.
- [ ] Recapture `src/osprey/examples/casting/goldens/ios` from the GPUI app, and check each by eye.
- [x] Port the old iOS appearance tests to the shared controls and app-scoped store. Fresh GPUI runs pass the Sherbet Appearance menu and selected semantics, all-three-skin process restart, and native recovery notices for unknown, empty and malformed saved values; repaired values restart without another notice. All assertions remain intact. The full journey and coverage boxes above remain open.
- [ ] Wire the Rust coverage export into the iOS coverage gate, which stays red until then.
- [ ] Bring up the CoreDevice tunnel, install the signed build on the physical iPhone, launch it and cast to a real receiver.

Android:

- [ ] Verify the current `build.sh` end to end: the last journey pass predates its `OSPREY_CASTING_SKIN` change.
- [ ] Build and run the x86_64 emulator target; only ARM64 has run.
- [ ] Collect Android coverage and gate it.
- [ ] Install on a physical device and run a real cast with background playback and local-network permission.

Both:

- [ ] Finish the Poolside parity run; make `skin_parity.py` fast enough and headless enough for CI.
- [ ] Only when both apps are finished: lift the exclusion test in `scripts/ci/ci-scope.test.mjs` and add PR CI jobs for the iOS simulator and Android emulator journeys, against the receiver fixture so that no run can reach a real TV.

Windows:

- [ ] Finish the audit above. Until then casting stays on `src/toolchain/hosts/winui`, whose pictures in `src/osprey/examples/casting/goldens/windows` must match the current scene.
- [ ] Recapture `src/osprey/examples/casting/goldens/windows/150` on a 150% display: CI's runner draws at 100%, so only `100` was refreshed after the shared scene changed.
