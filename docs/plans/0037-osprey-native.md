# Plan 0037 — Osprey Native hosts

Contract: [spec 0047](../specs/applications/0047-OspreyNativeHosts.md), [spec 0046](../specs/platforms/0046-GpuiDesktopHost.md), [spec 0048](../specs/platforms/0048-WinUiHost.md), [spec 0053](../specs/applications/0053-NativeSharedViews.md), [AGENTS.md § Apps](../../AGENTS.md#apps). The graphics scene window's plumbing is in [plan 0055](0055-targets.md).

Goal: one shared-frame boundary and one host library per platform draw every Osprey Native app in its platform's own toolkit — FerroUI on the desktop and for Casting's phones, SwiftUI on macOS and iOS, Android views, WinUI on Windows and HTML in the browser. What remains is the FerroUI retarget, the conformance and coverage gates, Storage, the last apps, the shared-memory bridge's acceptance and the phone hosts.

## 1. Retarget the GPUI apps to FerroUI [NATIVE-FERROUI-RETARGET]

GPUI is retired: each app's `run.sh` and the shared make recipes move to `src/toolchain/hosts/ferroui`, and no new GPUI work is planned.

- [ ] Retarget the probe to the FerroUI host: `make -C src/toolchain/targets/macos probe` opens it on FerroUI and its self-test (`make -C src/toolchain/targets/macos/tests probe-headless`) runs there [NATIVE-FERROUI-RETARGET].
- [ ] Retarget Flock to the FerroUI host: `make -C src/toolchain/targets/macos flock` opens it on FerroUI and its self-test (`make -C src/toolchain/targets/macos/tests flock-headless`) runs there [NATIVE-FERROUI-RETARGET].
- [ ] Retarget Painting to the FerroUI host: `make -C src/toolchain/targets/macos painting` opens it on FerroUI and its self-test (`make -C src/toolchain/targets/macos/tests painting-headless`) runs there; Windows stays on the WinUI host [NATIVE-FERROUI-RETARGET].
- [ ] Retarget the signal_flow app to the FerroUI host: `make -C src/toolchain/targets/macos signal-flow` opens it on FerroUI and its self-test (`make -C src/toolchain/targets/macos/tests signal-flow-headless`) runs there [NATIVE-FERROUI-RETARGET].
- [ ] Retarget Casting desktop to the FerroUI host: `make -C src/toolchain/targets/macos casting` opens it on FerroUI and its self-test (`make -C src/toolchain/targets/macos/tests casting-headless`) runs there; Windows stays on the WinUI host [NATIVE-FERROUI-RETARGET].
- [ ] Retarget Casting iOS to the FerroUI host: `make -C src/toolchain/targets/ios casting` opens it on FerroUI and its journey (`make -C src/toolchain/targets/ios/tests casting-headless`) runs there. Casting mobile goes through FerroUI, never GPUI [NATIVE-FERROUI-RETARGET].
- [ ] Retarget Casting Android to the FerroUI host: `make -C src/toolchain/targets/android casting` opens it on FerroUI and its journeys (`make -C src/toolchain/targets/android/tests casting-headless`) run there [NATIVE-FERROUI-RETARGET].
- [ ] Remove the daw's `daw-gpui` fallback: delete the `daw-gpui` target from `src/toolchain/targets/macos/Makefile` and the GPUI path from `src/osprey/examples/daw/run.sh`; `prism` and `daw-events-headless` are its only desktop entry points [NATIVE-FERROUI-RETARGET].
- [ ] Delete `src/toolchain/hosts/gpui` and its make targets once nothing uses GPUI: first move the modules FerroUI includes by `#[path = "../gpui/src/…"]` (`src/toolchain/hosts/ferroui/lib.rs`, `adapters.rs`, `app.rs`) into the FerroUI host, then remove `_gpui-app-self-test` (`scripts/make/apps.mk`), `GPUI_APPS` (`src/toolchain/targets/macos/tests/Makefile`), the `hosts/gpui` cargo test (`src/toolchain/targets/tests/Makefile`) and `.github/workflows/gpui.yml`. Check: `git grep -l hosts/gpui` is empty and `make -C src/toolchain/targets/macos/tests` is green [NATIVE-FERROUI-RETARGET].

## 2. Framework neutrality [NATIVE-HOSTS-NEUTRAL]

- [ ] Remove `ReadPropertyLists`, `Fingerprint`, `RunningApplications` and `OpenSettings` from `Capability` and `Requests::members` in `src/osprey/packages/native/src/capability.ospml`, and `Run`'s `plist` field; product requests travel as `HostCall`. Check: none of the four constructors remain, `read-plists`, `fingerprint`, `running-applications` and `open-settings` leave every host's refused list (`src/toolchain/hosts/winui/OspreyHost.Core/Coverage.cs` among them), and `osprey src/osprey/packages/native/tests --run --memory=arc` and `--memory=gc` are green.

## 3. Conformance suite [NATIVE-HOSTS-CONFORMANCE]

- [ ] Add `src/osprey/packages/native/conformance/`: the probe's start context, a recorded event sequence and the expected frames, produced once from the three boundary calls and checked in.
- [ ] Add a native test that regenerates the frames from the fixtures and diffs them, so a wire change fails there first. Check: it lives under `src/osprey/packages/native/tests`.
- [ ] Run the set in each host's smoke: the SwiftUI host's self-test, the VS Code webview (`npm test --prefix src/toolchain/vscode-extension`), FerroUI, Android, WinUI and the browser host. Check: each smoke passes the conformance set.

## 4. Coverage matrix [NATIVE-HOSTS-COVERAGE]

- [ ] Add a script that reads the constructor names in `src/osprey/packages/native/src/node.ospml` and `capability.ospml` plus each host's mapping table (`src/toolchain/hosts/ferroui/view.rs`, `src/toolchain/hosts/winui/OspreyHost.Core/Coverage.cs`, `src/toolchain/vscode-extension/client/src/osprey-ui/render.ts` + `capabilities.ts`, and the SwiftUI and Android hosts' kind lists) and writes `src/osprey/packages/native/COVERAGE.md`.
- [ ] Make CI regenerate `COVERAGE.md` and fail on a diff, so a new constructor cannot land without every host's cell.
- [ ] Each host README links its column, the iOS and Android hosts' included.

## 5. Shared web renderer and browser host [NATIVE-HOSTS-FAMILY]

- [ ] Extract `osprey-ui` (`protocol.ts`, `render.ts`, `styles.ts`, `theme.ts`, `icons.ts`, `session.ts`, `wasi.ts`) from `src/toolchain/vscode-extension/client/src/osprey-ui` into `packages/web` as an npm package; `capabilities.ts` and the webview plumbing stay in the extension. Check: `vscode-extension` still builds and its renderer suite is green.
- [ ] Browser host (`src/toolchain/hosts/web`, which answers `http` only today): run the WASM module in a worker and answer the file and store capabilities that exist in browser APIs. Check: `src/toolchain/hosts/web/test` covers each.
- [ ] Run the probe application in a browser tab and pass the conformance set.

## 6. Storage [UI-EFFECTS-MEASURE] [UI-COMPONENTS-SHORTCUT] [NATIVE-HOSTS-NEUTRAL]

- [ ] Run Storage on the FerroUI host, whose view already draws `split` and `context-menu`. Check: `make -C src/toolchain/targets/macos storage` opens it with no diagnostics.
- [ ] Add the FerroUI `MeasureFolders` adapter in `src/toolchain/hosts/ferroui/adapters/`: Rust on `std::fs`, `st_blocks` and `(st_dev, st_ino)` on Unix, the Windows rules from spec 0048's folder walk; tests on a temporary tree cover a hidden file, a symbolic link, a hard link counted once, an unreadable folder, deadline and cancel. Check: `cargo test --manifest-path src/toolchain/hosts/ferroui/Cargo.toml measure`.
- [ ] Windows folder walk in both adapters (Rust and C#): capacity from `GetDiskFreeSpaceExW`, one `GetFileInformationByHandleEx(FileIdExtdDirectoryInfo)` batch per directory, hidden is `FILE_ATTRIBUTE_HIDDEN`, only `IO_REPARSE_TAG_SYMLINK` and `IO_REPARSE_TAG_MOUNT_POINT` are links, `WOF` files count allocated bytes and `APPEXECLINK` 0, cloud placeholders are never opened, hard links once by volume serial + 128-bit file id, `ERROR_ACCESS_DENIED` adds one unreadable. Check: both adapters produce the same JSON on a shared fixture tree.
- [ ] Add `Shortcut`/`Chord` to `src/osprey/packages/native/src/node.ospml`: a `Button` may carry a typed `Chord`; `Primary` is Command on Apple systems and Control elsewhere; the host shows the chord in its menu or tooltip; Osprey picks the chords per platform (Alt+Up, F5, Ctrl+O on Windows; Command+Up, Command+R, Command+O on the Mac).
- [ ] Elevation on Windows: a `windows`-namespace `HostCall` "restart as administrator", answered by the WinUI host with `ShellExecuteExW`/`runas` and `--elevated`, offered only on Windows when the walk hit unreadable folders and the process is not elevated. Check: a Storage self-test drives it.
- [ ] Move Storage's Apple-only requests to `HostCall`s only the SwiftUI host answers: Photos summary `HostCall "photos" "summary"` and Full Disk Access `HostCall "macos" "privacy-settings"`.
- [ ] Storage acceptance: `./target/release/osprey test src/osprey/examples/storage/test` is green on every OS and no host source names Storage.
- [ ] Retire `src/osprey/examples/macos` once Storage runs on FerroUI, WinUI and SwiftUI; move every link to `src/osprey/examples/storage` and keep a website redirect.
- [ ] Record a full `C:` drive measurement (walk time, peak memory, gap vs used capacity) in `src/osprey/examples/storage/README.md`.
- [ ] Record flat memory over ten minutes of Flock in the FerroUI window in `src/toolchain/hosts/ferroui/README.md`, and a focused-window fps reading at 5,000 birds with the screen unlocked (M-series Mac) in `src/osprey/examples/gpui/flock/README.md`.

## 7. WinUI host [WINUI-HOST-CAPABILITIES] [WINUI-HOST-DLL] [WINUI-HOST-SCENE]

- [ ] Answer `list-directories` and `read-texts` (`save-file` and `http` are answered) and the elevated-restart `HostCall`; move them from `Unanswered` to `Answered` in `src/toolchain/hosts/winui/OspreyHost.Core/Coverage.cs`. Check: `dotnet test --project OspreyHost.Tests` from PowerShell.
- [ ] Add the MinGW C fixture host that loads the probe's DLL, binds both imports and calls both exports; run it in `.github/workflows/ci-windows.yml`.
- [ ] Measure whether WinUI shapes hold Flock's 5,000 birds at 60 Hz on a named Windows machine, and record it.
- [ ] MSIX: sign a real `.msix` with a test certificate in CI (today `app.ps1` registers a loose layout with `Add-AppxPackage -Register`). Check: CI produces and installs the signed package.
- [ ] Add a `windows-arm64` slice (MSYS2 CLANGARM64) and a `win-arm64` publish. Check: `--target` accepts `windows-arm64` and the host publishes for arm64.
- [ ] Graphics scenes in the WinUI host: `OspreyHost.exe --scene` draws into a `SwapChainPanel` and `src/osprey/examples/graphics/ospgfx_d3d12_window.c` is deleted ([plan 0055](0055-targets.md)). Check: `src/toolchain/targets/windows/apps.json` gains a `graphics` entry.

## 8. Remaining apps [NATIVE-APPS-ALL] [UI-COMPONENTS-RICH]

- [ ] Every host completes `Http` with its platform HTTPS client within the framework's limits; the FerroUI host does not answer it today.
- [ ] Inbox: move its SQL tables to JSON documents in the app store via `StoreRead`/`StoreWrite`, each query an Osprey list function (`src/osprey/examples/mobile/inbox/src/storage.ospml`, `update.ospml`).
- [ ] Port Inbox to Osprey Native and delete `NativeRenderer.swift` and `NativeRenderer.kt`; it then runs on FerroUI, WinUI, the SwiftUI and Android hosts (plain Android views in Kotlin, not Compose) and the browser host, whose iOS and Android integrations then draw the leased frame instead of their own JSON protocol.
- [ ] iPhone counter (`src/osprey/examples/ios`) on `src/toolchain/hosts/swift`.
- [ ] Counter, Inbox and Workbench on Osprey Native in the WinUI host as AppContainer packages; add their entries to `src/toolchain/targets/windows/apps.json` with the capabilities listed in AGENTS.md § Apps.
- [ ] Bank and GPT-2 sandboxed in AppContainers launched by the apps Makefile.

## 9. Casting on phones [CAST-MOBILE-GPUI]

The spec ID and `src/osprey/examples/casting/mobile.md` ([CAST-MOBILE-DELIVERY], the item-by-item checklist) still say GPUI; after section 1 every run below is against the FerroUI app.

- [ ] iOS: the full native journey passes — `src/osprey/examples/casting/ios/ui-test.sh` (media, gestures, lifecycle, appearance), the canonical canvas/native-picker and real Bonjour discovery/rescan XTests, and a real Bonjour/TLS playback smoke. Check: `make -C src/toolchain/targets/ios/tests casting-headless`.
- [ ] iOS screen readers: the scene's accessibility tree reaches VoiceOver and the journey presses through its action.
- [ ] Recapture `src/osprey/examples/casting/goldens/ios` from the FerroUI app and check each picture by eye [CAST-MOBILE-GOLDENS].
- [ ] Bring up the CoreDevice tunnel, install the signed build on the physical iPhone, launch it and cast to a real receiver.
- [ ] Android: verify `src/osprey/examples/casting/android/build.sh` end to end (the last journey pass predates its `OSPREY_CASTING_SKIN` change), build and run the x86_64 emulator target (only ARM64 has run), and install on a physical device for a real cast with background playback and local-network permission.
- [ ] Wire the host's Rust coverage export into the iOS coverage gate, which stays red until then; collect Android coverage and gate it [CAST-MOBILE-COVERAGE].
- [ ] Skin parity: finish the Poolside run of `src/osprey/examples/casting/skin_parity.py` (Android scored 0.976 on Sherbet and 0.901 on Midnight against desktop) and make it fast and headless enough for CI.
- [ ] Automate real NSD and Bonjour discovery against a published fixture; Android found a real TV by hand, and its journey uses the receiver fixture so it can never reach one.
- [ ] Only when both apps are finished: lift the exclusion test in `scripts/ci/ci-scope.test.mjs` and add PR CI jobs for the iOS simulator and Android emulator journeys against the receiver fixture.
- [ ] Recapture `src/osprey/examples/casting/goldens/windows/150` on a 150% display (only `100` was refreshed after the shared scene changed).

## 10. Shared-memory bridge [NATIVE-SHM]

Contract: [the shared frame](../specs/applications/0047-OspreyNativeHosts.md#the-shared-frame-native-shm). One schema, allocator, publication path and delivery implementation: each migration names and deletes the code it replaces, with no compatibility wrapper, second bridge or dual mode.

- [ ] Record instructions, parsed/serialized/copied bytes, allocations and p50/p99 dispatch/frame timings for Storage's large tree, Flock at 5,000 birds, Casting playback and a long Agent transcript in `src/osprey/packages/native/README.md`, naming hardware, revision, workload and warm/cold conditions; include the ARC framework modes 0–3 baseline (verify).
- [ ] Expand the native and host tests with A/B/C skipped-render commands, slow/pinned readers, allocation failure at each commit stage, full queues, duplicate completions, stale bindings and shutdown with a GPU lease, so every [NATIVE-SHM-VERIFY] row has a fixture.
- [ ] Generate exact signed-64-bit counter/offset accessors with bigint in JavaScript, rejecting negative values and overflow; adjacent identities still collapse at 2^53 under the number mapping (`target/logs/native-shm/lifecycle-numeric-red.json`), and cross-language conformance tests keep the distinction.
- [ ] Declare all boundary models in `src/toolchain/schemas/native.td` (limits, typed errors, ownership descriptors, transport frames) and generate matching Osprey, Rust, Swift, C# and TypeScript views/builders with fixed layouts and exact shared-ABI fingerprints. Check: compile-time cross-language layout checks and deliberate startup mismatch refusal pass.
- [ ] Deliver the missing typeDiagram view/generator support upstream and pin it: the zero-copy reader, schema-driven canonical tdbin codecs, verify-once readers and the numeric representation scene data needs; at that revision verify the local, unpinned changes — bigint model and codec emission for Rust, TypeScript, C#, Swift and Osprey, the tdbin scalar writer, the shared-layout manifest with its 64-byte inline-record limit and the C# logical emitter (verify).
- [ ] Same logical fixtures read identically through all generated views; canonical wire output expands shared DAGs within limits and matches a freshly built equivalent tree; reserved kinds, malformed lengths, hashes and excessive depth/expansion are refused; shared identity is never written to disk.
- [ ] Single allocator and handle registry, immutable chunk builder/seal/retain/release protocol, checked references, iterative reclamation and generated lease wrappers. Check: GC/ARC tests reject managed-heap references, prove host-held snapshots survive collection and replacement, and force stale and double-release failures.
- [ ] Complete snapshots, protected owning-root acquisition and atomic logical commit of candidate state, root and commands. Check: every failed preparation leaves state unchanged and emits no command; ASan/TSan, Miri and loom cover the lifetime and interleaving invariants.
- [ ] Finite startup budgets and completion reservations: exhausted queues and allocations return the specified errors without blocking the UI, dropping accepted input or invalidating current snapshots; retained old chunks and shared-DAG expansion count toward their budgets.
- [ ] Complete the windowed Casting, Painting and Flock journeys with fresh rendered screenshots and golden comparison on an unlocked desktop (locked-screen runs stop at "the window did not render the requested screenshot frame"). Check: `make -C src/toolchain/targets/macos/tests casting`, `painting` and `flock`.
- [ ] Run Tally windowed on Windows. Check: `make -C src/toolchain/targets/windows/tests tally`.
- [ ] Remove round-tripped `saved` state, migrate persistence explicitly to versioned tdbin, retain GPU payloads through actual completion and count the necessary uploads and platform copies.
- [ ] Ordered capability delivery, completion admission, precise pointer-motion coalescing and non-reentrant dispatch. Check: a request from a skipped render snapshot still executes once and its completion reaches the application; cancellation, duplicate delivery and stale instances are tested.
- [ ] Compile authored machine JSON to generated constants at build time (direct-memory constants in-process, canonical tdbin for external assets) and delete runtime JSON machine parsing from the bridge; external HTTP, MCP, Cast and config formats stay adapter responsibilities.
- [ ] Close/admission stop, cancellation, ticket drain, executor join and lease drain before unload. Check: delayed GPU and adapter completions cannot touch destroyed state, and `Busy` retains memory safely.
- [ ] Browser: read leased wasm views, refresh view epochs on growth, work same-thread without isolation and in a shared worker only with isolation that never blocks or spins the UI thread. Check: both modes verified.
- [ ] Replace the WASI JSON frames with canonical tdbin over the bounded base64 carrier of the actual VS Code transport, verified once before exposing immutable views; unknown versions and malformed input fail before presentation.
- [ ] Write the browser boundary decision ([NATIVE-SHM-WIRE]: direct wasm views, shared memory for cross-thread use, canonical tdbin for VS Code isolation and persistence) into spec 0047, which lacks it today (verify).
- [ ] Satisfy [NATIVE-SHM-ACCESSIBILITY]: generated semantic models, coherent render and semantic revisions, keyboard and accessibility focus kept independent, announcements through reliable queues. Check: retained AT callbacks and stale action refusal hold under publication and release races.
- [ ] Record real VoiceOver, Narrator, TalkBack and browser screen-reader journeys on their supported hosts, plus keyboard and switch access, enlarged text, contrast and reduced motion, asserting semantic state, actions and focus at every interaction; state web WCAG 2.2 A/AA applicability and results, and retain manual AT evidence where automation falls short. Screenshots alone never pass this gate.
- [ ] All hosts pass the same logical conformance fixtures and canonical tdbin comparison, with process-local allocation IDs never affecting it; tests and self-test scripts stay readable, converting their inputs at the driver edge.
- [ ] Existing app journeys and goldens pass unchanged on every supported host. Check: guards prove zero JSON parsing/serialization on shared paths and no unlisted copies, counting unavoidable construction, platform and transport copies separately; named external-format adapters are the only exclusions.
- [ ] Run hostile-frame fuzzing, pinned-reader memory stress, shutdown races, GC/ARC lifetime suites and wasm memory-growth tests, keeping receipts for every [NATIVE-SHM-VERIFY] row and all source, schema and generator revisions.
- [ ] Publish before/after measurements for the same four workloads: zero shared-path serialization, no instruction-count regression on any workload, frame-time distributions rather than wall-clock CI thresholds; then delete the obsolete bridge paths, leaving no dual mode or JSON fallback.
- [ ] Add to [NATIVE-SHM-VERIFY] in spec 0047 the grounding references it lacks (Fabric, Rust ownership and synchronization, wasm memory semantics, Metal completion, the typeDiagram wire and reader specifications) (verify).

## 11. Phone hosts [NATIVE-HOSTS-MOBILE]

- [ ] Phone layout for `split` and `aside`: Storage's three panes are wider than a phone on both hosts; collapse a split to one pane with navigation on a compact width, in the SwiftUI and Android hosts.
- [ ] iOS device builds are unsigned and read media from `--resources`; bundle an application's `assets` in the `.app` and sign with a development identity.
- [ ] Map every remaining node, symbol (SF Symbols) and capability in `src/toolchain/hosts/swift` and run the conformance set there. Check: Storage runs on `src/toolchain/hosts/swift` with no diagnostics.
