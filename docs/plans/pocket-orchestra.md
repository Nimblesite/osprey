# Pocket Orchestra delivery plan [ORCHESTRA-PLAN]

Status — implementation in progress. The shared session core has passing two-phone ownership and expiry journeys; live transport, audio and platform acceptance remain pending.

The shared audio foundation is the independent [`@osprey/synth` toolkit](../specs/runtime/Synthesis.md); this application is a consumer and must not own reusable DSP.

The normative requirements are in [Pocket Orchestra](../specs/applications/PocketOrchestra.md). The working name is Pocket Orchestra. The first delivery uses a desktop session host, LAN WebSockets, host-generated audio, three instruments and native/canvas presentations across macOS, Windows, iOS, Android and web. Linux is a follow-on target.

## Planning checklist [ORCHESTRA-PLAN-DOCUMENTS]

- [x] Define the ensemble experience, topology and platform roles.
- [x] Define native and canvas widget acceptance on every required platform.
- [x] Define protocol ownership, clock behaviour, disconnect handling and trust boundaries.
- [x] Identify existing runtime, audio and mobile-host gaps.
- [x] Specify implementation phases and measurable acceptance evidence.

## Feasibility and measured baseline [ORCHESTRA-PLAN-SPIKE]

- [ ] Inventory existing native capabilities, host canvas support, multitouch, audio and transport adapters. Record implementation evidence for each platform against [ORCHESTRA-PLATFORMS].
- [ ] Demonstrate a physical phone sending start/update/end events to a desktop through a bidirectional WebSocket adapter, with host acknowledgements and voice release on disconnect.
- [ ] Demonstrate continuous audio output and simultaneous voices on macOS and Windows, measuring physical input-to-speaker latency against [ORCHESTRA-ACCEPTANCE].
- [ ] Serve the controller from the desktop and verify local-network permission and HTTP/WebSocket behaviour in mobile Safari, Chromium and Firefox. Record the exact supported local deployment procedure.
- [ ] Prove native controls beside a canvas and a switch to canvas-rendered controls on each required host. Include simultaneous touches and accessible actions.
- [ ] Declare the canonical models in typeDiagram. Verify generation for Osprey and required host bindings. Evaluate JSON and TDBIN codec support, framing and payload sizes, recording the selected protocol.
- [ ] Record reference hardware, latency distributions, audio buffers and frame timings. Resolve failed feasibility criteria before building out instruments.

Exit evidence consists of runnable spikes, reviewed model generation, a host capability matrix and measured results. Existing sample screenshots or successful compilation alone do not satisfy this phase.

## Shared session core [ORCHESTRA-PLAN-CORE]

- [x] Add the shared application under `src/osprey/examples/pocket_orchestra/`, with canonical typeDiagram models and generated bindings following the existing repository conventions.
- [ ] Implement typed session transitions, admission, roster, player ownership, sequence/epoch validation and state snapshots under [ORCHESTRA-NETWORK-PROTOCOL].
- [ ] Implement bounded queues, update coalescing, heartbeat expiry, reconnect and explicit voice release under [ORCHESTRA-NETWORK-TIMING].
- [ ] Implement production and deterministic test handlers for the effects in [ORCHESTRA-EFFECTS]. Reuse the existing native capability boundary and standard diagnostics package.
- [ ] Drive complete ensemble scenarios using a virtual clock and in-memory peers, including malformed events, denied roles, congestion and suspension. Link tests to the relevant hierarchical spec IDs.

Exit evidence is the same session core running against real transport and deterministic test handlers, with all failure paths producing typed outcomes.

## Playable desktop host and first controller [ORCHESTRA-PLAN-ENSEMBLE]

- [ ] Implement macOS and Windows host adapters for transport, continuous audio, output selection and lifecycle. Keep application decisions in Osprey.
- [ ] Deliver percussion end to end, from phone touch through authenticated networking to host sound and the shared stage.
- [ ] Add bass ribbon and chord field with shared key/scale selection, instrument changes and per-player volume/mute.
- [ ] Add conductor admission, connection health, QR/manual joining, master volume and Silence.
- [ ] Implement both timing modes, accepted-event timestamps and remote stage clock estimation.
- [ ] Test device loss, host sleep, stalled peers and shutdown while notes are active. Measure the eight-controller workload.

Exit evidence is a real ensemble performance through one speaker output, with bounded resources and no stuck voices.

## Complete platform and widget coverage [ORCHESTRA-PLAN-HOSTS]

- [ ] Ship native iOS and Android controllers using the shared Osprey core, with local-network permissions, cancellation and foreground recovery.
- [ ] Ship WebAssembly browser controllers and stage viewers served by the desktop host.
- [ ] Ship additional desktop controller mode on macOS and Windows.
- [ ] Complete both presentations on every platform in [ORCHESTRA-PLATFORMS]. Test switching while connected, while holding a note and after a reconnect.
- [ ] Verify native control classes, semantic HTML, canvas interaction, accessible equivalents, reduced motion, large text, safe areas and resizing.
- [ ] Package Windows with WinUI and MSIX AppContainer capabilities. Verify LAN hosting and local client connections under the installed package's actual restrictions. Document any required sandbox exception with evidence.

Exit evidence is a matrix of real platform runs covering every required role and presentation, with physical phone evidence in addition to simulator tests.

## Commands, tests and release evidence [ORCHESTRA-PLAN-DELIVERY]

The following are planned entry points, to be added with implementation and documented through each folder's `make help`.

| Location | Planned target | Expected result |
| --- | --- | --- |
| `src/toolchain/targets/{macos,windows}/Makefile` | `pocket-orchestra` | Build and open the desktop session host |
| `src/toolchain/targets/{ios,android}/Makefile` | `pocket-orchestra` | Build and open the native controller |
| `src/toolchain/targets/web/Makefile` | `pocket-orchestra` | Build and open the browser controller with a local test host |
| `src/toolchain/targets/<platform>/tests/Makefile` | `pocket-orchestra`, `pocket-orchestra-headless` | Run this application's windowed or headless journey with screenshot checks |
| `src/toolchain/targets/tests/Makefile` | `pocket-orchestra` | Run the deterministic ensemble and real transport suites |

- [ ] Place host implementation in existing `src/toolchain/hosts/` platforms and shared recipes in `scripts/make/`. Reuse shared view and instrument logic across presentations.
- [ ] Add the planned launch/test targets for every delivered platform, plus app-specific diagnostics under `target/`.
- [ ] Add reviewed platform screenshot goldens, accessibility journeys, audio assertions, transport integration tests and measured physical-device acceptance runs.
- [ ] Integrate build/static analysis, cached parallel testing with coverage, then coverage checks into the required CI phases. Retain or increase package coverage thresholds.
- [ ] Run static duplication analysis on implementation, review host adapters for duplicated application logic and verify model generation is reproducible.
- [ ] Complete the acceptance evidence in [ORCHESTRA-ACCEPTANCE], documenting actual browser versions, operating systems, device models and measured limits.
- [ ] Document installation, starting a session, joining, permission recovery and LAN troubleshooting with the actual app commands.

Completion requires the full platform matrix and both widget presentations. The first playable desktop/phone pair is an intermediate milestone.

## Later extensions [ORCHESTRA-PLAN-LATER]

Linux desktop hosting, trusted HTTPS/WSS provisioning, motion instruments, saved compositions, event recordings and replay, external lighting and Internet relay sessions each require a follow-up scope decision. Keep the initial ensemble focused on reliable local play and verifiable platform parity.

## First playable pair evidence [ORCHESTRA-PLAN-PLAYABLE]

- [x] Build a native macOS session app that serves phone controllers and plays Osprey oscillator output through AVAudioEngine.
- [x] Deliver the browser pentatonic multitouch ribbon over authenticated WebSocket, with keyboard access and note release on disconnect or suspension.
- [x] Add native QR/invite, controller/note counts, master volume, output meter and ensemble silence controls.
- [x] Exercise the native server with eight controllers and eighty notes; verify PCM bounds, ownership, malformed input, expiry and silence after release.
- [x] Exercise a mobile-sized browser controller and run the same transport/audio journey through the actual Mac output device.
- [x] Verify the audio ring with two million ordered samples under address/undefined-behaviour sanitizers and ThreadSanitizer.
- [ ] Measure physical iPhone/Android-to-speaker latency on Wi-Fi and record device/browser versions.
- [ ] Complete Windows hosting, native phone controllers and the full native/canvas presentation matrix above.

Run commands and honest delivery boundaries are in `src/osprey/examples/pocket_orchestra/README.md`. Browser emulation and generated PCM checks do not establish physical-phone or speaker-to-ear latency. The renderer and Osprey session run on a dedicated OS thread, separate from AppKit; hard-real-time scheduling remains unproven.

## Playable sound and recovery [ORCHESTRA-PLAN-SOUND]

- [x] Expose independent instrument selection, tone, echo, reverb and metallic warp through authenticated sequenced edits.
- [x] Connect the phone expression selector and range control to real native synthesis and per-controller feedback racks.
- [x] Add persistent cross-cutting controller/backend diagnostics and the Osprey app icon.
- [x] Reproduce stopped output with a real engine configuration notification and restart it while retaining the held note.
- [x] Verify final headless and visible browser/audio journeys for the four-instrument palette, including real output recovery and zero callback underruns.
- [ ] Extract host echo/reverb storage into reusable synth graph processors and measure alias rejection and physical-device latency.

Validation on 2026-11-14 used the real native kernel and captured PCM. Both app make targets passed with Chromium/WebKit controllers, eight peers/eighty voices, distinct timbres, all four audible expressions, feedback tails, malformed/stale settings and persistent logs. GC and ARC each passed three session journeys and thirteen synth journeys. Generated Osprey/Swift models reproduced byte-for-byte. Deslop reported no visible duplicate groups in the supported Mac test files after sharing the native-fixture compiler. Full-branch CI retains the previously recorded coverage gaps above. Physical-phone and physical output-route switching remain unverified.
