# Mobile casting through the shared GPUI host [CAST-MOBILE-GPUI]

The native casting apps are migrating to one renderer: the same Rust `hosts/gpui` code that draws the desktop player, per [plan 0053](../../docs/plans/0053-casting-gpui-mobile.md). The separate SwiftUI iOS renderer is retired; its sources are archived in [archive/casting-swiftui](../../archive/casting-swiftui/README.md) and its full delivery record — every checked box, screenshot and coverage number — is preserved verbatim in [mobile-history.md](../../archive/casting-swiftui/mobile-history.md). Those results belong to the retired implementations and are not evidence for the shared GPUI apps. The migration is incomplete.

## Current status [CAST-MOBILE-GPUI-STATUS]

- `examples/casting/ios/build.sh` packages the shared `hosts/gpui` binary — the same crate the desktop app links — for `ios-sim` and `ios`, with native Swift cast services (`CastTLS.swift`, `CastDiscovery.swift` and `PlatformServices.swift`, plus the C file-server and media-render runtimes) linked in as `libosprey_cast_services.a`, alongside the machine document, skins and icon catalog. The resulting `Casting.app` links, installs and launches on the simulator.
- The simulator now renders the desktop GPUI player. A pi verified the first frame appears after about two seconds; the earlier white image was the launch screen. The shared scene renderer now shrinks fixed zoom to fit narrow windows, and the host applies platform safe-area insets; the whole player is visible below the notch. The unused lower window still paints white pending the startup theme correction. Rendering alone is not a passing playback journey.
- The shared GPUI host crate passes its 78 library tests (`cargo test --manifest-path hosts/gpui/Cargo.toml --lib`). Those cover the host library only; they do not exercise the mobile UI, the native cast services or device transport.
- The shared C ABI still cross-compiles for `ios`, `ios-sim`, `android-arm64` and `android-x64` via `make -C apps/tests casting-mobile-build TARGET=…`. That is a build fact, not a journey.
- The iOS native service adapter discovers a real local receiver and opens, cancels and imports a generated MP3 through the native document picker into the GPUI player. Native bridge probes cover malformed requests and bounded reads; they do not prove playback. The Android GPUI backend remains in progress under plan 0053.

## Tests, goldens and coverage [CAST-MOBILE-GPUI-GATES]

The five iOS XCTest journeys and the Android suite remain strict and unchanged in intent. The Xcode project no longer contains an app target — `build.sh` produces the app outside Xcode — so the UI-test runner now attaches to the installed bundle by explicit identifier, `org.ospreylang.casting`. Those tests and the committed `goldens/ios` and `goldens/android` screenshots passed only against the retired renderers; until the GPUI apps pass those full journeys, they are not green, and the goldens must be re-captured from the GPUI apps before any screenshot gate runs.

The Xcode coverage gate fails clearly while the GPUI app — a Rust binary built outside Xcode — reports no coverage. Wiring the Rust coverage export into that gate is pending work; weakening or bypassing the gate is not.

The retired production SwiftUI app was installed on a paired physical iPhone but never launched, because the screen was locked. That install is not physical proof for GPUI either.

## Mandatory checklist [CAST-MOBILE-DELIVERY]

A box is checked only by a passing run against the shared GPUI app on the platform named; a historical pass from a retired renderer never ticks a box.

Shared renderer and desktop parity:

- [ ] Draw the casting machine on iOS and Android from the same `hosts/gpui` view code as desktop, with touch input and safe areas. [CAST-MOBILE-GPUI]
- [x] Diagnose the white first frame as the launch screen and verify the GPUI frame on the iOS simulator; a visible, correct Sherbet scene is the entry criterion for every journey below.
- [ ] Match each skin's mobile pictures to the desktop's widgets at the same pixel width. [CAST-MOBILE-GPUI]

File input:

- [ ] Native document selection on both platforms, including cancellation and failed reads; copy scoped files into app storage and play from the imported copy. [CAST-DESIGN-STATE]
- [ ] Recover from unreadable, corrupt and replacement files, with native share/open integration.

Playback controls:

- [ ] Play, pause, resume, seek, volume, speed, stop, replay and track completion, with exactly one Cast command per adjustment and metadata verified by the receiver fixture.

Discovery, TLS and HTTP:

- [ ] Native Bonjour (iOS) and NSD (Android) discovery without injected transcripts; handle permission denial, retries and disappearing receivers. [CAST-MOBILE-NATIVE-DISCOVERY]
- [ ] Native TLS, local HTTP serving and media rendering inside the app, without shelling out to desktop tools; surface unsupported formats and service failures.
- [ ] Keep equal receiver names distinct by endpoint, retain the selection across rescans, and verify an unavailable receiver's error, its retention across a skin change, and recovery on a reachable receiver. [CAST-MOBILE-RECEIVER-IDENTITY]

Skins, persistence and errors:

- [ ] All three skins render and switch during playback without reloading, preserving the session. [CAST-MOBILE-SKINS]
- [ ] Restore the selected skin after process restart; repair unknown, empty and malformed saved values with a native notice and a usable fallback. [CAST-MOBILE-SKIN-PERSISTENCE], [CAST-MOBILE-APPEARANCE-RECOVERY]
- [ ] Reject stale popup choices after an unsolicited receiver update. [CAST-MOBILE-POPUP-FRESHNESS]

Touch, accessibility and layout:

- [ ] 44-point iOS and 48-dp Android touch targets for every control, opening the picker from the enlarged edge; preserve the actual slider touch endpoint.
- [ ] Keyboard navigation and activation, focus indicators, screen-reader announcements for selections and errors, and full accessible track titles.
- [ ] Target separation in crowded layouts, scaled text, high contrast and reduced motion in all skins. [CAST-DESIGN-TOKENS], [CAST-DESIGN-NATIVE], [CAST-DESIGN-SKIN-CONTRACT]

Background and lifecycle:

- [ ] Home-and-return during live playback preserves the session, connection and exact receiver command counts; rotation preserves the connection and restores the system rotation policy.
- [ ] Handle connection loss, interrupted playback, and cleanup of network and file resources on backgrounding and termination.

Real journeys and goldens:

- [ ] iOS simulator journey: discovery, file selection, the full control set, skins, offline and error paths, app restart, and ready/restart screenshot goldens re-captured from the GPUI app. [CAST-MOBILE-GOLDENS]
- [ ] The equivalent Android journey on the GPUI backend, with the same gates.
- [ ] Both journeys verify exact served media bytes over HTTP, including range requests.

Coverage and CI:

- [ ] Wire the Rust coverage export into the iOS gate; collect Android coverage; add the required CI coverage gates. [CAST-MOBILE-COVERAGE]

Physical verification:

- [ ] Physical-device casting to a real receiver on both platforms, including background playback and local-network permissions. A locked-screen install proves nothing.

## Entry points

`make -C apps/ios casting` builds, installs and launches the simulator app; `make -C apps/ios casting-build TARGET=ios-sim|ios` builds the unsigned bundle. `make -C apps/android casting` builds and opens the current Android package. `make -C apps/tests casting-mobile-build TARGET=…` cross-compiles the shared library, and `make -C apps/tests casting-mobile-transport PLATFORM=ios` exercises the native transport sources against a live loopback TLS receiver. `examples/casting/ios/test.sh` and `examples/casting/ios/ui-test.sh` run the journey and coverage gates; the coverage gate is expected to stay red until the Rust export is wired in.
