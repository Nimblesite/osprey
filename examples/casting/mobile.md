# Mobile casting through the shared GPUI host [CAST-MOBILE-GPUI]

The native casting apps are migrating to one renderer: the same Rust `hosts/gpui` code that draws the desktop player, per [plan 0053](../../docs/plans/0053-casting-gpui-mobile.md). The separate SwiftUI iOS renderer is retired; its sources are archived in [archive/casting-swiftui](../../archive/casting-swiftui/README.md) and its full delivery record — every checked box, screenshot and coverage number — is preserved verbatim in [mobile-history.md](../../archive/casting-swiftui/mobile-history.md). Those results belong to the retired implementations and are not evidence for the shared GPUI apps. The migration is incomplete.

## Current status [CAST-MOBILE-GPUI-STATUS]

- `examples/casting/ios/build.sh` packages the shared `hosts/gpui` binary — the same crate the desktop app links — for `ios-sim` and `ios`, with native Swift cast services (`CastTLS.swift`, `CastDiscovery.swift` and `PlatformServices.swift`, plus the C file-server and media-render runtimes) linked in as `libosprey_cast_services.a`, alongside the machine document, skins and icon catalog. The resulting `Casting.app` links, installs and launches on the simulator.
- The simulator now renders the desktop GPUI player. A pi verified the first frame appears after about two seconds; the earlier white image was the launch screen. The shared scene renderer now shrinks fixed zoom to fit narrow windows, and the host applies platform safe-area insets; the whole player is visible below the notch. The startup envelope now takes its theme from the initialized skin once. The Sherbet artboard consumes resize events, fills the available region with the skin background and moves its vector controls with the viewport while keeping the artwork circular. Rendering alone is not a passing playback journey.
- The shared GPUI host crate passes its 78 library tests (`cargo test --manifest-path hosts/gpui/Cargo.toml --lib`). Those cover the host library only; they do not exercise the mobile UI, the native cast services or device transport.
- The shared GPUI app cross-compiles and packages for physical iOS hardware. No physical GPUI launch has been verified. Its existing four-step self-test also runs inside the simulator window and reaches its final accessibility record; that script checks startup/discovery only.
- The shared C ABI still cross-compiles for `ios`, `ios-sim`, `android-arm64` and `android-x64` via `make -C apps/tests casting-mobile-build TARGET=…`. That is a build fact, not a journey.
- The iOS native service adapter discovers a real local receiver and opens, cancels and imports a generated MP3 through the native document picker into the GPUI player. `make -C apps/tests casting-mobile-transport PLATFORM=ios` passes TLS transport and 99 simulator checks covering malformed requests, discovery deadlines and bounded reads; it does not prove player playback. Android now uses the GPUI host exclusively, with its Java player renderer removed and native discovery, picker and TalkBack services behind the shared host; see [plan 0053](../../docs/plans/0053-casting-gpui-mobile.md) for the remaining parity work.

## Tests, goldens and coverage [CAST-MOBILE-GPUI-GATES]

The original five iOS XCTest journeys and the Android suite retain their full scope. A sixth iOS XCTest verifies the shared scene's native accessible title and file button, its bounds and hit target, and a real tap opening the native picker; that focused journey passes against GPUI. The original pinned iOS backend renders the same scene but fails that unchanged test, confirming the adapter is required. The Xcode project no longer contains an app target — `build.sh` produces the app outside Xcode — so the UI-test runner now attaches to the installed bundle by explicit identifier, `org.ospreylang.casting`. The original full iOS journeys and `goldens/ios` still need verification and new captures against GPUI. Android's journey and captures have migrated to GPUI; all-skin parity remains open. The runner starts each iOS suite with a fresh test-app install before seeding its fixture documents, preserving restart checks inside the suite, and checks the installed executable against the GPUI build afterwards. This prevents both stale Xcode app products and persisted scene delegates from substituting the retired renderer or a blank restored scene.

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
