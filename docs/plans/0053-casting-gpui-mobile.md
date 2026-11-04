# Casting on iOS and Android through GPUI [CAST-MOBILE-GPUI]

The request, verbatim: "the fucking ios and android apps must use the same GPUI code as the fucking desktop version."

Today the three apps share `player.machine.json` and the compiled Osprey core, and each draws it differently: macOS through `hosts/gpui`, iOS through SwiftUI (`hosts/swift`, `examples/casting/ios/CastingApp.swift`), Android through its own Java views (`examples/casting/android`). Their widgets and layout differ, so the same skin looks different.

iOS: GPUI supports it. `hosts/gpui/Cargo.lock` already holds `gpui-pre-ios` 0.3.10, a snapshot of Zed's `gpui_ios` crate over UIKit, CoreText and Metal, with lifecycle, windowing, native text input, safe-area and keyboard insets and touch, and `gpui-pre-platform` selects it on `target_os = "ios"`. It has never been built here.

Android: `gpui-pre` 0.3.10 has no backend, and `gpui-pre-platform` has no Android branch, though `gpui-pre-wgpu` already handles Android surface loss. A backend has to come from the community crate `gpui-mobile` 0.1.0 (early development, Android on Vulkan) or be written here over `gpui-pre-wgpu`.

## Spike

- [x] Build `hosts/gpui` for `aarch64-apple-ios-sim` and draw the casting screen in the simulator through `gpui_ios`.
- [x] Read `gpui-mobile`'s Android backend as data, never built or run until reviewed, and choose between it and a backend over `gpui-pre-wgpu`.
- [x] Build `hosts/gpui` for `aarch64-linux-android` and draw the casting screen on the emulator.
- [x] Report what those runs show, with the failure output for anything that does not work.

## Port

- [x] One host: `hosts/gpui` draws casting on macOS, iOS and Android from the same view code, with touch input and safe areas.
- [x] Keep what only the platform can do behind the capability boundary: Bonjour and NSD discovery, document pickers, TLS, media serving, lifecycle, accessibility.
- [x] Android: the journey passes on the GPUI app against the TLS receiver fixture, with its golden.
- [ ] iOS: its full native journey passes on the GPUI app. The unchanged canonical canvas/native-picker and real Bonjour discovery/rescan XTests both pass through the shared GPUI host, including receiver label, viewport bounds and selected accessibility state. A separate real Bonjour/TLS playback smoke passes. The full media, gesture, lifecycle, appearance and coverage gates remain unverified.
- [ ] Compare each skin's mobile pictures with the desktop's widgets at the same size: `examples/casting/skin_parity.py` scored Android against desktop at Sherbet 0.976 and Midnight 0.901; Poolside has not finished a run, and the script needs more than five minutes and a real window, so it is not in CI.
- [x] Delete the SwiftUI and Java casting renderers. `hosts/swift` stays for Osprey Agent.
- [x] A fixed scene `zoom` is an upper bound: a narrower region fits the whole scene instead of clipping it, on every platform. The region around a scene takes the skin's colour, and the scene honours the safe area.
- [ ] Windows: casting still runs on `hosts/winui` (`examples/casting/README.md`). Audit whether `hosts/gpui` can replace it there, and report.
- [x] Android screen readers: the scene's AccessKit tree reaches TalkBack, and the journey presses through its action.
- [ ] iOS screen readers: the same tree through `accesskit_ios`.
- [ ] Automate real NSD and Bonjour discovery against a published fixture. Android found a real TV by hand; its journey uses the receiver fixture so that it can never reach a real TV.
