# Casting on iOS and Android through GPUI [CAST-MOBILE-GPUI]

The request, verbatim: "the fucking ios and android apps must use the same GPUI code as the fucking desktop version."

Today the three apps share `player.machine.json` and the compiled Osprey core, and each draws it differently: macOS through `hosts/gpui`, iOS through SwiftUI (`hosts/swift`, `examples/casting/ios/CastingApp.swift`), Android through its own Java views (`examples/casting/android`). Their widgets and layout differ, so the same skin looks different.

No official GPUI support for iOS or Android has been found. The community crate `gpui-mobile` 0.1.0 claims iOS on Metal and Android on Vulkan, calls itself early development, and needs unpublished Zed crates; it says its iOS work comes from Zed PR #43655, whose status is unverified. `hosts/gpui` builds on `gpui-kit` 0.8.1, so the first question is whether that fork can take such a backend at all.

## Spike

- [ ] Read `gpui-mobile` and PR #43655 as data, never built or run until reviewed, and report what `hosts/gpui` would need: the platform trait, renderer, text, input and windowing.
- [ ] Build `hosts/gpui` for `aarch64-apple-ios-sim` with a pinned, hash-checked backend, in the way `hosts/gpui/dependencies.json` pins its patches, and draw one casting screen.
- [ ] The same for `aarch64-linux-android` on the emulator.
- [ ] Decide from what those two runs show. If a platform cannot work, say why with the failure output.

## Port

- [ ] One host: `hosts/gpui` draws casting on macOS, iOS and Android from the same view code, with touch input and safe areas.
- [ ] Keep what only the platform can do behind the capability boundary: Bonjour and NSD discovery, document pickers, TLS, media serving, lifecycle, accessibility.
- [ ] Prove it: the existing native journeys still pass, and each skin's mobile pictures match the desktop's widgets at the same width.
- [ ] Delete the SwiftUI and Java casting renderers once GPUI passes those journeys. `hosts/swift` stays for Osprey Agent.
