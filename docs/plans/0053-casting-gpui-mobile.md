# Casting on iOS and Android through GPUI [CAST-MOBILE-GPUI]

The request, verbatim: "the fucking ios and android apps must use the same GPUI code as the fucking desktop version."

Today the three apps share `player.machine.json` and the compiled Osprey core, and each draws it differently: macOS through `hosts/gpui`, iOS through SwiftUI (`hosts/swift`, `examples/casting/ios/CastingApp.swift`), Android through its own Java views (`examples/casting/android`). Their widgets and layout differ, so the same skin looks different.

iOS: GPUI supports it. `hosts/gpui/Cargo.lock` already holds `gpui-pre-ios` 0.3.10, a snapshot of Zed's `gpui_ios` crate over UIKit, CoreText and Metal, with lifecycle, windowing, native text input, safe-area and keyboard insets and touch, and `gpui-pre-platform` selects it on `target_os = "ios"`. It has never been built here.

Android: `gpui-pre` 0.3.10 has no backend, and `gpui-pre-platform` has no Android branch, though `gpui-pre-wgpu` already handles Android surface loss. A backend has to come from the community crate `gpui-mobile` 0.1.0 (early development, Android on Vulkan) or be written here over `gpui-pre-wgpu`.

## Spike

- [ ] Build `hosts/gpui` for `aarch64-apple-ios-sim` and draw the casting screen in the simulator through `gpui_ios`.
- [ ] Read `gpui-mobile`'s Android backend as data, never built or run until reviewed, and choose between it and a backend over `gpui-pre-wgpu`.
- [ ] Build `hosts/gpui` for `aarch64-linux-android` and draw the casting screen on the emulator.
- [ ] Report what those runs show, with the failure output for anything that does not work.

## Port

- [ ] One host: `hosts/gpui` draws casting on macOS, iOS and Android from the same view code, with touch input and safe areas.
- [ ] Keep what only the platform can do behind the capability boundary: Bonjour and NSD discovery, document pickers, TLS, media serving, lifecycle, accessibility.
- [ ] Prove it: the existing native journeys still pass, and each skin's mobile pictures match the desktop's widgets at the same width.
- [ ] Delete the SwiftUI and Java casting renderers once GPUI passes those journeys. `hosts/swift` stays for Osprey Agent.
