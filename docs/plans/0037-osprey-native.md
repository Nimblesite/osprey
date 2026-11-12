# Plan 0037 — Osprey Native hosts

Contract: [spec 0047](../specs/applications/0047-OspreyNativeHosts.md), [spec 0046](../specs/platforms/0046-GpuiDesktopHost.md), [spec 0048](../specs/platforms/0048-WinUiHost.md), [AGENTS.md § Apps](../../AGENTS.md#apps).

Depends on: [plan 0023](0023-gpu.md) (its Graphics plumbing section puts the scenes in the WinUI host).

## Prerequisites in place

- GPUI, WinUI and in-repo SwiftUI hosts, Storage/Flock/Painting, the UI-machine framework and the sandboxed-WinUI-with-goldens CI all run (spec 0046, spec 0048, UiMachines).
- The `Symbol` and `Platform` unions are typed and host-neutral ([UI-COMPONENTS-SYMBOLS], [UI-CONTEXT-PLATFORM]); `HostCall` exists and the native suite encodes one ([UI-EFFECTS]).

## 1. Finish framework neutrality [NATIVE-HOSTS-NEUTRAL]

- [ ] Remove `ReadPropertyLists`, `Fingerprint`, `RunningApplications`, `OpenSettings` from `Capability` and `Requests::members` in `src/osprey/packages/native/src/capability.ospml`, and remove `Run`'s `plist` field; product work travels as `HostCall`. Drop `read-plists`, `fingerprint`, `running-applications`, `open-settings` from `src/toolchain/hosts/gpui/src/coverage.rs` (UNANSWERED) and `src/toolchain/hosts/winui/OspreyHost.Core/Coverage.cs` (Unanswered). Check: `osprey src/osprey/packages/native/tests --run --memory=arc` and `--memory=gc` green; none of the four constructors remain in `capability.ospml`.
- [ ] Remove `Evidence` and fold it into `Pair` (`Ui::pair`, wire `pair`): delete `Evidence` from `Node` in `src/osprey/packages/native/src/node.ospml`, delete `Ui::evidence` and the `encode.ospml` arm, and drop `evidence` from GPUI `coverage.rs` REFUSED and WinUI `Coverage.cs` Refused. Check: `pair` is in both hosts' drawn lists (already is) and `grep evidence src/osprey/packages/native/src` is empty.
- [ ] Delete `Brand` from `Node`/`Ui`/`encode.ospml`; `Keyed::region`'s placeholder stays `Spacer` (`src/osprey/packages/native/src/reactive.ospml` already uses it); drop `brand` from GPUI `coverage.rs` REFUSED and WinUI `Coverage.cs` Refused. Check: `grep brand src/osprey/packages/native/src` is empty; `cargo test coverage` and WinUI `CoverageTests` green.

## 2. Conformance suite [NATIVE-HOSTS-CONFORMANCE]

- [ ] Add `src/osprey/packages/native/conformance/`: the probe application's start context, a recorded event sequence and the expected envelopes, produced once from `Runtime::start`/`dispatch` and checked in.
- [ ] Add an Osprey test that regenerates the envelopes from the fixtures and diffs them, so a framework change that alters the wire shape fails here first. Check: the test lives under `src/osprey/packages/native/tests` and fails on a wire change.
- [ ] Wire each host's smoke to run the set: SwiftUI in Vigil's smoke, webview in `npm test --prefix src/toolchain/vscode-extension`, GPUI in `src/toolchain/hosts/gpui/run.sh --smoke`, WinUI in its smoke. Check: each smoke passes the conformance set.

## 3. Coverage matrix [NATIVE-HOSTS-COVERAGE]

- [ ] Add a script that reads `src/osprey/packages/native/src/node.ospml` and `capability.ospml` for constructor names plus each host's mapping table (`src/toolchain/hosts/gpui/src/coverage.rs`, `src/toolchain/hosts/winui/OspreyHost.Core/Coverage.cs`, `src/toolchain/vscode-extension/client/src/osprey-ui/render.ts` + `capabilities.ts`), and writes `src/osprey/packages/native/COVERAGE.md`.
- [ ] Make CI regenerate it and fail on a diff, so a new constructor cannot land without every host's cell.
- [ ] Each host README links its column.

## 4. Shared web renderer [NATIVE-HOSTS-FAMILY]

- [ ] Extract `osprey-ui` (`protocol.ts`, `render.ts`, `styles.ts`, `theme.ts`, `icons.ts`, `session.ts`, `wasi.ts`) from `src/toolchain/vscode-extension/client/src/osprey-ui` into `packages/web` as an npm package; `capabilities.ts` and the webview plumbing stay in the extension. Check: `vscode-extension` still builds and its renderer suite is green.
- [ ] Add a browser host page that loads the WASI module in a worker and renders into a `div`, with file and store capabilities on the browser APIs that exist.
- [ ] Run the probe application in a browser tab and pass the conformance set.

## 5. Storage on GPUI, plus the two unrecorded measurements

- [ ] Add the GPUI `MeasureFolders` adapter `src/toolchain/hosts/gpui/src/adapters/measure.rs` ([UI-EFFECTS-MEASURE]): Rust on `std::fs`, `st_blocks` and `(st_dev, st_ino)` on Unix, the Windows rules from spec 0048's folder walk. Tests on a temporary tree: hidden file, symbolic link, a hard link counted once, an unreadable folder, deadline, cancel. Move `measure-folders` from UNANSWERED to ANSWERED in `coverage.rs`. Check: `cargo test --manifest-path src/toolchain/hosts/gpui/Cargo.toml measure`.
- [ ] Map `split` and `context-menu` in `src/toolchain/hosts/gpui/src/render.rs` (+ `render/controls.rs`) so Storage's root split and row menus draw; move them from REFUSED to DRAWN in `coverage.rs`. Check: `cargo test coverage`; `make -C src/toolchain/targets/macos storage` opens with no diagnostics.
- [ ] Storage acceptance: `./target/release/osprey test src/osprey/examples/storage/test` green on every OS; no host source names Storage ([NATIVE-HOSTS-NEUTRAL]).
- [ ] Record flat memory over ten minutes of Flock in the GPUI window; put the numbers in `src/toolchain/hosts/gpui/README.md`.
- [ ] Record a focused-window fps reading at 5,000 birds with the screen unlocked (M-series Mac); put it in `src/osprey/examples/gpui/flock/README.md`.

## 6. Storage: Windows walk, shortcuts, elevation [UI-EFFECTS-MEASURE] [UI-COMPONENTS-SHORTCUT]

- [ ] GPUI Windows walk: capacity from `GetDiskFreeSpaceExW` (available after quotas); one `GetFileInformationByHandleEx(FileIdExtdDirectoryInfo)` batch per directory; hidden is `FILE_ATTRIBUTE_HIDDEN`; only `IO_REPARSE_TAG_SYMLINK` and `IO_REPARSE_TAG_MOUNT_POINT` are links; `WOF` files count allocated bytes and `APPEXECLINK` 0; cloud placeholders never opened (`CLOUD_*`, `RECALL_ON_DATA_ACCESS`, `RECALL_ON_OPEN`, `OFFLINE`); hard links once by volume serial + 128-bit file id; `ERROR_ACCESS_DENIED` adds one to `unreadable`. Run a shared fixture tree with expected JSON against both the Rust and C# walkers. Check: both adapters produce the same JSON.
- [ ] Add `Shortcut`/`Chord` to `src/osprey/packages/native/src/node.ospml` ([UI-COMPONENTS-SHORTCUT]): a `Button` may carry a typed `Chord`; `Primary` is Command on Apple systems, Control elsewhere; the host shows the chord in its menu or tooltip; Osprey picks chords per platform (Alt+Up, F5, Ctrl+O on Windows; Command+Up, Command+R, Command+O on the Mac).
- [ ] Elevation on Windows: `HostCall` in the `windows` namespace for "restart as administrator", answered by the WinUI host with `ShellExecuteExW`/`runas` and `--elevated`; Osprey offers it only on Windows when the walk hit unreadable folders and the process is not elevated. Check: a Storage self-test drives it.
- [ ] Measure a full `C:` drive (walk time, peak memory, gap vs used capacity) and record in `src/osprey/examples/storage/README.md`.

## 7. WinUI remaining: adapters, C fixture host, 60 Hz gate, packaging

- [ ] Answer the remaining WinUI capabilities ([WINUI-HOST-CAPABILITIES]): `list-directories`, `read-texts`, `save-file`, `http` (on `HttpClient`) and the `windows` `HostCall` for elevated restart; move them from `Unanswered` to `Answered` in `src/toolchain/hosts/winui/OspreyHost.Core/Coverage.cs`. Check: `dotnet test --project OspreyHost.Tests` (from PowerShell).
- [ ] Add the MinGW C fixture host that loads the probe's DLL, binds both imports and calls both exports; run it in `ci-windows.yml` ([WINUI-HOST-DLL]).
- [ ] Measure whether WinUI shapes hold Flock's 5,000 birds at 60 Hz on a named Windows machine; record it ([WINUI-HOST-SCENE]).
- [ ] MSIX: sign a real `.msix` with a test certificate in CI (today `app.ps1` registers a loose layout via `Add-AppxPackage -Register`). Check: CI produces and installs the signed package.
- [ ] Add a `windows-arm64` slice (MSYS2 CLANGARM64) and a `win-arm64` publish. Check: `--target` accepts `windows-arm64`; the host publishes for arm64.

## 8. SwiftUI host, and retiring the old Storage app

- [ ] One SwiftUI host, not two: `src/osprey/workbench/hosts/apple/Sources/OspreyNative` is a diverged copy of `src/toolchain/hosts/swift/Sources/OspreyNative` (Capabilities, Store, Process, Network and Log adapters, Layout, Controls), so #355's store fix had to land twice. Move the shared sources into one Swift package both apps depend on; the workbench keeps only its own views (Film, Math, Cards). Check: `diff -rq` finds no file in both trees, and both `swift test` suites pass.
- [ ] Extend `src/toolchain/hosts/swift` (the in-repo SwiftUI host) to macOS and iOS, map every node and symbol (SF Symbols) and every capability including `measure-folders`, and run the conformance set. Check: Storage runs on `src/toolchain/hosts/swift` with no diagnostics.
- [ ] Move Storage's Apple-only requests to `HostCall`s only this host answers: Photos summary `HostCall "photos" "summary"`, Full Disk Access `HostCall "macos" "privacy-settings"`.
- [ ] Retire `src/osprey/examples/macos` once Storage runs on GPUI, WinUI and SwiftUI; move every link to `src/osprey/examples/storage`; keep a website redirect.

## 9. Issue Inbox [UI-COMPONENTS-RICH]

- [ ] Every host completes `Http` with its platform HTTPS client within the framework's limits.
- [ ] Add `Rich` ([UI-COMPONENTS-RICH]): a paragraph of styled runs — strong, emphasis, code, link; markdown parsing stays in Osprey (`markdown.ospml`).
- [ ] Move Inbox's SQL tables to JSON documents in the app store via `StoreRead`/`StoreWrite`; each query becomes an Osprey list function.
- [ ] Add `hosts/compose` (Android) from today's `NativeRenderer.kt`; delete Inbox's own protocol, `NativeRenderer.swift` and `NativeRenderer.kt`; Inbox runs on GPUI, WinUI, SwiftUI and Compose.

## 10. Remaining apps [NATIVE-APPS-ALL]

- [ ] iPhone counter on `src/toolchain/hosts/swift`.
- [ ] Talon Bank's Android and iOS apps on the Compose and SwiftUI hosts; Talon Bank's web client on the browser host.
- [ ] Graphics scenes in the WinUI host: `OspreyHost.exe --scene` draws into a `SwapChainPanel` and the Win32 window code (`src/osprey/examples/graphics/ospgfx_d3d12_window.c`) is deleted — [plan 0023](0023-gpu.md) Graphics plumbing. Check: `src/toolchain/targets/windows/apps.json` gains a `graphics` entry.
- [ ] Counter, Inbox and Workbench on Osprey Native in the WinUI host as AppContainer packages (after the Inbox port); add entries to `src/toolchain/targets/windows/apps.json` with the capabilities listed in AGENTS.md § Apps.
- [ ] Bank and GPT-2 sandboxed in AppContainers launched by the apps Makefile.

## 11. Replace the JSON bridge with shared memory [NATIVE-SHM]

**Design settled, 2026-11-12; implementation outstanding.** Normative contract: [shared-memory replacement](../specs/applications/0047-OspreyNativeHosts.md#shared-memory-replacement-native-shm). The user's requirement is direct memory sharing wherever possible, typeDiagram on both sides, and tdbin only where serialization is necessary. This section supersedes the earlier tdbin-in-memory/triple-buffer proposal; there is one target design and no permanent legacy mode.

### Decisions closed

- [x] One logical schema, `src/toolchain/schemas/native.td`; generate a shared-memory ABI and a separate canonical tdbin representation. No serialized native dispatch, relative pointers across generations or contiguous-address reservation dependency. This is two generated representations, not two hand-maintained models. [NATIVE-SHM-LAYOUT]
- [x] Application state stays on one executor behind an opaque handle. Bridge chunks are immutable, reference counted outside GC/ARC and retained by explicit leases; no references back into the managed heap. Share chunks, not entire historical generation arenas. [NATIVE-SHM-OWNERSHIP]
- [x] Publish complete render snapshots through a mutex-protected owning-root slot; acquire ownership under the same lock before dereferencing. No raw-pointer load/retain race and no triple buffer. [NATIVE-SHM-PUBLISH]
- [x] Keep events, commands and completions in bounded reliable queues, separate from replaceable snapshots. Specify admission, completion reservations, coalescing barriers, stale binding rejection, shutdown and explicit resource failure. [NATIVE-SHM-DELIVERY], [NATIVE-SHM-RESOURCES]
- [x] Browser uses direct wasm views; cross-thread use requires shared memory and nonblocking UI acquisition. VS Code isolation and persistence use verified canonical tdbin; account for actual transport carriers rather than assuming transferable buffers. [NATIVE-SHM-WIRE]
- [x] Ground the decisions in Fabric, Rust ownership/synchronization, wasm memory semantics, Metal completion and the current typeDiagram wire/reader specifications; references and limitations live in [NATIVE-SHM-VERIFY]. These checks close design decisions only, not implementation acceptance.

### Complexity constraint

Keep one schema, allocator, publication path and delivery implementation. Change dependent apps directly; do not preserve obsolete APIs with adapters, compatibility flags or a second bridge. Every migration must name and delete the code it replaces, including duplicated accessibility models/generators. Add abstractions only for a demonstrated shared use or a specified safety/accessibility invariant; host-specific behavior stays in its host adapter. Do not build a general plugin, reflection or transport framework for this cutover. Judge simplification across the runtime, hosts and migrated apps together, not by moving complexity into another folder.

### Delivery sequence and ownership

The integrating agent owns the shared runtime/schema and reviews host-owner changes against the one contract. Allocate exact file ownership on TMC before edits. Every step is test-first and lands with its evidence; the model/code generator work is a dependency to implement, not an unresolved architectural choice. The integration owner owns the required typeDiagram changes and pinned release as part of this work; an unimplemented generator is a tracked delivery prerequisite, not permission for hand-written bridge models or a second codec.

1. **Baseline and adversarial fixtures**
   - [x] Collect the existing isolated framework modes 0–3 under ARC, three runs each, and record instructions/RSS, compiler fingerprint and limitations in the native README (2026-11-12). This preliminary measurement does not complete the four-app baseline below.
   - [ ] Record instructions, parsed/serialized/copied bytes, allocations and p50/p99 dispatch/frame timings for Storage's large tree, Flock at 5,000 birds, Casting playback and a long Agent transcript in the native README. Name hardware, revision, workload and warm/cold conditions; ratios are not guessed in advance.
   - [ ] Expand existing native/host tests with A/B/C skipped-render commands, slow/pinned readers, allocation failure at each commit stage, full queues, duplicate completions, stale bindings and shutdown with a GPU lease. Preserve the current journeys and goldens. Fixture intent covers every row of [NATIVE-SHM-VERIFY].
2. **Schema and generated contracts**
   - [x] Declare the initial lifecycle/lease/admission/limit/error models in `src/toolchain/schemas/native.td` and validate parsing plus Rust/TypeScript/C# model generation (2026-11-12). This is not the complete UI schema or fixed-layout ABI; TypeScript typechecking passes but the current generator maps Int to lossy number, so generated outputs are not integrated.
   - [x] Implement and test explicit bigint model emission in the upstream typeDiagram checkout: red generator regression, 457 core tests with coverage, three nested/container/union paths, and actual native schema IDs through signed-64-bit maximum (2026-11-12). Upstream changes are local and not released/pinned; runtime bounds checks and ABI accessors remain below.
   - [x] Declare the initial semantic tree, secure-text variant, separate focus channels, typed actions and reliable announcement payloads in the shared schema (2026-11-12). Parse/model generation passes for Rust/TypeScript/C#; a strict TypeScript fixture checks forbidden secure text and exact IDs. This is a logical subset, not runtime validation, full role coverage, host implementation or assistive-technology acceptance.
   - [x] Extend the existing upstream tdbin scalar writer to accept checked full-width bigint values; real framed/packed round trips preserve signed boundaries and adjacent IDs above 2^53, while overflow and unsafe numbers fail (2026-11-12; 458 core tests/coverage pass). Local upstream prerequisite only: generated codecs and shared-memory accessors remain outstanding.
   - [x] Connect bigint mode to the existing upstream TypeScript codec emitter, including optional integer reads and required-child defaults. Emitted code executes real packed/unpacked round trips, rejects overflow, matches number-mode bytes for shared inputs and typechecks strictly; 459 core tests/coverage pass (2026-11-12). This does not implement shared ABI accessors or add unsupported schema shapes.
   - [ ] Generate exact signed-64-bit counter/offset accessors using bigint in JavaScript; reject negative values and overflow. The initial numeric probe collapses adjacent identities at 2^53 with the current number mapping (`target/logs/native-shm/lifecycle-numeric-red.json`); preserve this distinction in cross-language conformance tests.
   - [ ] Declare all boundary models in `src/toolchain/schemas/native.td`, including limits, typed errors, ownership descriptors and transport frames. Generate matching Osprey, Rust, Swift, C# and TypeScript views/builders; generate fixed field layouts and exact shared-ABI fingerprints. Compile-time cross-language layout checks and deliberate startup mismatch refusal must pass.
   - [ ] Deliver missing typeDiagram view/generator support upstream and pin it. Verify current support at that revision: the reviewed zero-copy reader is not implemented. Require schema-driven canonical tdbin codecs, verify-once readers and numeric representation support needed by scene data; do not claim reserved wire kinds are available or change tdbin locally.
   - [ ] Same logical fixtures must read identically through all generated views. Canonical wire output expands shared DAGs within limits and matches a freshly built equivalent tree; reserved kinds, malformed lengths, hashes and excessive depth/expansion are refused. Shared identity is never written to disk.
3. **Shared allocator and publication**
   - [ ] Implement the single allocator/handle registry, immutable chunk builder/seal/retain/release protocol, checked references and iterative reclamation; expose generated lease wrappers. GC/ARC tests reject managed-heap references and prove host-held snapshots survive collection/replacement; force stale and double release failures.
   - [ ] Implement complete snapshots, protected owning-root acquisition and atomic logical commit of candidate state/root/commands. Tests prove every failed preparation leaves state unchanged and emits no command. ASan/TSan, Miri and loom cover the defined lifetime/interleaving invariants, not just happy-path rendering.
   - [ ] Implement finite startup budgets and completion reservations; exhausted queues/allocations return the specified errors without blocking UI, dropping accepted input or invalidating current snapshots. Retained old chunks and shared-DAG expansion count toward their respective budgets.
4. **Cut over every in-process host, one surface at a time**
   - [ ] Replace start/dispatch strings with typed events/application handles and leased snapshots on GPUI, SwiftUI and WinUI, including their iOS/Android integrations. Delete `encode.ospml`, GPUI `envelope.rs`, Swift `Protocol.swift`/`JSONValue.swift`, WinUI decoding equivalents and bridge-specific release-string calls once no caller remains; do not remove a general compiler ABI still used outside this bridge.
   - [ ] Remove round-tripped `saved` state; migrate persistence explicitly to versioned tdbin. Replace `native_slice`'s string map with snapshot views and `native_shape`'s per-shape mutex pushes with shared scene columns. Retain GPU payloads through actual completion; count necessary uploads/platform copies.
   - [ ] Implement ordered capability delivery, completion admission, precise pointer-motion coalescing and non-reentrant dispatch. Prove a request from a skipped render snapshot still executes once and its completion reaches the application; test cancellation, duplicate delivery and stale instances.
   - [ ] Compile authored machine JSON to generated constants at build time; direct-memory constants in-process and canonical tdbin only for external assets. Delete runtime JSON machine parsing from the bridge. External HTTP/MCP/Cast/config formats remain adapter responsibilities.
   - [ ] Implement close/admission stop, cancellation, ticket drain, executor join and lease drain before unload; delayed GPU/adapter completions cannot touch destroyed state. Prove `Busy` retains memory safely. Start on the UI executor; move to a worker only with measured benefit and the same conformance behavior.
5. **Web and process boundaries**
   - [ ] Browser reads leased wasm views; growth refreshes view epochs, same-thread operation works without isolation, shared-worker operation requires isolation and never blocks/spins the UI thread. Verify both modes.
   - [ ] Replace WASI JSON frames and webview UI envelopes with canonical tdbin; test the specified bounded base64 carrier over the actual VS Code transport and explicitly account for carrier copies. Verify once before exposing immutable views; packed input has bounded unpack ownership. Unknown versions and malformed input fail before presentation.
6. **Acceptance and deletion**
   - [ ] Satisfy [NATIVE-SHM-ACCESSIBILITY]: generate semantic models, mount coherent render/semantic revisions, preserve keyboard/accessibility focus independently, and deliver announcements through reliable queues. Verify retained AT callbacks and stale action refusal under publication/release races.
   - [ ] Record real VoiceOver, Narrator, TalkBack and browser screen-reader journeys on their supported hosts, plus keyboard/switch access, enlarged text, contrast and reduced-motion checks. Assert semantic state/actions and focus at every interaction; retain manual AT evidence where automation is insufficient. Web WCAG 2.2 A/AA applicability and results must be explicit; screenshots alone never pass this gate.
   - [ ] All hosts pass the same logical conformance fixtures and canonical tdbin comparison; process-local allocation IDs never affect the comparison. Keep tests and self-test scripts readable, converting their inputs at the driver edge.
   - [ ] Existing app journeys/goldens pass unchanged on every supported host. Add guards for zero JSON parsing/serialization on shared paths and no unlisted copies, separately counting unavoidable construction/platform/transport copies. Named external-format adapters are the only exclusions.
   - [ ] Run hostile-frame fuzzing, pinned-reader memory stress, shutdown races, GC/ARC lifetime suites and wasm memory-growth tests. Retain receipts for every [NATIVE-SHM-VERIFY] row and all source/schema/generator revisions.
   - [ ] Publish before/after measurements for the same four workloads. Require zero shared-path serialization and no instruction-count regression on any workload; record frame-time distributions rather than unstable wall-clock CI thresholds. A claimed performance improvement requires measured evidence. Run normal CI, then delete the obsolete bridge paths; no dual mode or JSON fallback remains.

Completed means the specification's ownership, publication, delivery, resource and boundary invariants are implemented and proven on the host matrix—not just that the JSON encoder was replaced.
